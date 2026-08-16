//! `SeaORM` persistence adapters for `AsterFSD` identity and client policy.
//!
//! This crate owns database connection policy, schema migration execution and
//! the persistent implementation of the authentication port. `SQLx` statement
//! logging is opt-in and remains disabled by default.

#![cfg_attr(
    not(test),
    deny(
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented,
        clippy::unreachable,
        clippy::unwrap_used
    )
)]

/// `SeaORM` entity definitions used by the persistence adapter.
pub mod entities;

use aster_fsd_auth::{AuthError, Authenticator, verify_password};
use aster_fsd_migration::{Migrator, MigratorTrait};
use aster_fsd_model::{AtcRating, AuthenticatedIdentity, PilotRating};
use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectOptions, Database, DatabaseConnection,
    DbErr, EntityTrait, QueryFilter,
};
use std::time::Duration;

/// `SQLx` statement-log verbosity used when statement logging is enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlxLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<SqlxLogLevel> for log::LevelFilter {
    fn from(value: SqlxLogLevel) -> Self {
        match value {
            SqlxLogLevel::Error => Self::Error,
            SqlxLogLevel::Warn => Self::Warn,
            SqlxLogLevel::Info => Self::Info,
            SqlxLogLevel::Debug => Self::Debug,
            SqlxLogLevel::Trace => Self::Trace,
        }
    }
}

/// Database pool and SQL statement logging configuration.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub sqlx_logging: bool,
    pub sqlx_logging_level: SqlxLogLevel,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://asterfsd.db".to_string(),
            max_connections: 100,
            min_connections: 5,
            sqlx_logging: false,
            sqlx_logging_level: SqlxLogLevel::Debug,
        }
    }
}

/// Opens the configured database and applies all pending migrations.
///
/// # Errors
///
/// Returns [`DbErr`] when the connection pool cannot be created or a migration
/// fails. A failed migration prevents the database handle from being returned.
pub async fn connect(config: &DatabaseConfig) -> Result<DatabaseConnection, DbErr> {
    tracing::info!(
        backend = database_backend(&config.url),
        max_connections = config.max_connections,
        min_connections = config.min_connections,
        sqlx_logging = config.sqlx_logging,
        ?config.sqlx_logging_level,
        "Connecting to database"
    );
    let mut options = ConnectOptions::new(config.url.clone());
    options
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .connect_timeout(Duration::from_secs(8))
        .acquire_timeout(Duration::from_secs(8))
        .idle_timeout(Duration::from_mins(5))
        .max_lifetime(Duration::from_mins(30))
        .sqlx_logging(config.sqlx_logging)
        .sqlx_logging_level(config.sqlx_logging_level.into());
    let database = Database::connect(options).await?;
    tracing::info!(
        backend = database_backend(&config.url),
        "Database connected"
    );
    tracing::info!("Running database migrations");
    Migrator::up(&database, None).await?;
    tracing::info!("Database migrations completed");
    Ok(database)
}

fn database_backend(url: &str) -> &'static str {
    match url.split_once(':').map(|(scheme, _)| scheme) {
        Some("sqlite") => "sqlite",
        Some("postgres" | "postgresql") => "postgresql",
        Some("mysql") => "mysql",
        _ => "unknown",
    }
}

/// Persistent implementation of the core authentication port.
#[derive(Clone)]
pub struct SeaOrmAuthenticator {
    database: DatabaseConnection,
}

impl SeaOrmAuthenticator {
    /// Creates an authenticator backed by an initialized database connection.
    #[must_use]
    pub fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }

    /// Inserts a new network user with a pre-hashed password.
    ///
    /// # Errors
    ///
    /// Returns [`DbErr`] when validation or insertion fails, including unique
    /// network-ID conflicts.
    pub async fn create_user(
        &self,
        network_id: String,
        password_hash: String,
        real_name: String,
        atc_rating: AtcRating,
        pilot_rating: PilotRating,
    ) -> Result<entities::user::Model, DbErr> {
        let now = chrono::Utc::now();
        entities::user::ActiveModel {
            network_id: Set(network_id),
            password_hash: Set(password_hash),
            real_name: Set(real_name),
            atc_rating: Set(atc_rating),
            pilot_rating: Set(pilot_rating),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.database)
        .await
    }
}

#[async_trait]
impl Authenticator for SeaOrmAuthenticator {
    async fn authorize_client(&self, client_id: &str) -> Result<(), AuthError> {
        tracing::debug!(%client_id, "Checking client software authorization");
        let found = entities::client_whitelist::Entity::find()
            .filter(entities::client_whitelist::Column::ClientId.eq(client_id))
            .filter(entities::client_whitelist::Column::Enabled.eq(true))
            .one(&self.database)
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))?;
        found
            .map(|_| {
                tracing::debug!(%client_id, "Client software authorization accepted");
            })
            .ok_or(AuthError::ClientNotAuthorized)
    }

    async fn authenticate(
        &self,
        network_id: &str,
        password: &str,
    ) -> Result<AuthenticatedIdentity, AuthError> {
        tracing::debug!(%network_id, "Loading network identity");
        let user = entities::user::Entity::find()
            .filter(entities::user::Column::NetworkId.eq(network_id))
            .one(&self.database)
            .await
            .map_err(|error| AuthError::Backend(error.to_string()))?
            .ok_or(AuthError::InvalidCredentials)?;

        let valid = verify_password(password.to_string(), user.password_hash.clone()).await?;
        if !valid {
            return Err(AuthError::InvalidCredentials);
        }
        tracing::debug!(%network_id, "Network identity password verified");
        Ok(AuthenticatedIdentity {
            network_id: user.network_id,
            real_name: user.real_name,
            atc_rating: user.atc_rating,
            pilot_rating: user.pilot_rating,
            suspended: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectionTrait, DbBackend, Iterable, Statement};

    #[tokio::test]
    async fn migrations_and_database_authentication_work_on_sqlite() {
        let database = connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            min_connections: 1,
            max_connections: 1,
            ..DatabaseConfig::default()
        })
        .await
        .unwrap();
        let authenticator = SeaOrmAuthenticator::new(database);
        assert!(authenticator.authorize_client("48e2").await.is_ok());
        assert!(matches!(
            authenticator.authorize_client("unknown-client").await,
            Err(AuthError::ClientNotAuthorized)
        ));
        let hash = aster_fsd_auth::hash_password("secret").unwrap();
        authenticator
            .create_user(
                "ECP1547".to_string(),
                hash,
                "Test User".to_string(),
                AtcRating::Controller1,
                PilotRating::PrivatePilotLicense,
            )
            .await
            .unwrap();

        let identity = authenticator
            .authenticate("ECP1547", "secret")
            .await
            .unwrap();
        assert_eq!(identity.real_name, "Test User");
        assert_eq!(identity.atc_rating, AtcRating::Controller1);
        assert_eq!(identity.pilot_rating, PilotRating::PrivatePilotLicense);

        let stored = authenticator
            .database
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT atc_rating, pilot_rating FROM users WHERE network_id = 'ECP1547'",
            ))
            .await
            .unwrap()
            .unwrap();
        let stored_atc: String = stored.try_get_by_index(0).unwrap();
        let stored_pilot: String = stored.try_get_by_index(1).unwrap();
        assert_eq!(stored_atc, "controller_1");
        assert_eq!(stored_pilot, "private_pilot_license");

        authenticator
            .database
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO users (network_id, password_hash, real_name) \
                 VALUES ('DEFAULTS', 'unused', 'Default Ratings')",
            ))
            .await
            .unwrap();
        let defaults = authenticator
            .database
            .query_one_raw(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT atc_rating, pilot_rating FROM users WHERE network_id = 'DEFAULTS'",
            ))
            .await
            .unwrap()
            .unwrap();
        let default_atc: String = defaults.try_get_by_index(0).unwrap();
        let default_pilot: String = defaults.try_get_by_index(1).unwrap();
        assert_eq!(default_atc, "observer");
        assert_eq!(default_pilot, "private_pilot_license");

        assert!(authenticator.authenticate("ECP1547", "bad").await.is_err());
        assert!(matches!(
            authenticator.authenticate("MISSING", "secret").await,
            Err(AuthError::InvalidCredentials)
        ));

        Migrator::up(&authenticator.database, None).await.unwrap();
    }

    #[tokio::test]
    async fn every_rating_enum_round_trips_through_sqlite() {
        let database = connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            min_connections: 1,
            max_connections: 1,
            ..DatabaseConfig::default()
        })
        .await
        .unwrap();
        let authenticator = SeaOrmAuthenticator::new(database);

        for (index, rating) in AtcRating::iter().enumerate() {
            let model = authenticator
                .create_user(
                    format!("ATC{index}"),
                    "unused".to_string(),
                    "ATC Rating".to_string(),
                    rating,
                    PilotRating::PrivatePilotLicense,
                )
                .await
                .unwrap();
            assert_eq!(model.atc_rating, rating);
        }
        for (index, rating) in PilotRating::iter().enumerate() {
            let model = authenticator
                .create_user(
                    format!("PILOT{index}"),
                    "unused".to_string(),
                    "Pilot Rating".to_string(),
                    AtcRating::Observer,
                    rating,
                )
                .await
                .unwrap();
            assert_eq!(model.pilot_rating, rating);
        }
    }

    #[tokio::test]
    async fn unknown_database_rating_strings_are_rejected_by_the_typed_entity() {
        let database = connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            min_connections: 1,
            max_connections: 1,
            ..DatabaseConfig::default()
        })
        .await
        .unwrap();
        database
            .execute_raw(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO users \
                 (network_id, password_hash, real_name, atc_rating, pilot_rating) \
                 VALUES ('INVALID', 'unused', 'Invalid Rating', 'root', 'unrated')",
            ))
            .await
            .unwrap();
        assert!(entities::user::Entity::find().one(&database).await.is_err());
    }

    #[test]
    fn database_backend_classification_never_exposes_connection_details() {
        assert_eq!(database_backend("sqlite://asterfsd.db"), "sqlite");
        assert_eq!(
            database_backend("postgres://user:secret@db.example/asterfsd"),
            "postgresql"
        );
        assert_eq!(
            database_backend("postgresql://user:secret@db.example/asterfsd"),
            "postgresql"
        );
        assert_eq!(
            database_backend("mysql://user:secret@db.example/asterfsd"),
            "mysql"
        );
        assert_eq!(database_backend("secret-without-a-scheme"), "unknown");
    }

    #[tokio::test]
    async fn malformed_password_hash_is_a_structured_authentication_error() {
        let database = connect(&DatabaseConfig {
            url: "sqlite::memory:".to_string(),
            min_connections: 1,
            max_connections: 1,
            ..DatabaseConfig::default()
        })
        .await
        .unwrap();
        let authenticator = SeaOrmAuthenticator::new(database);
        authenticator
            .create_user(
                "BROKEN".to_string(),
                "not-an-argon2-hash".to_string(),
                "Broken Hash".to_string(),
                AtcRating::Observer,
                PilotRating::PrivatePilotLicense,
            )
            .await
            .unwrap();
        assert!(matches!(
            authenticator.authenticate("BROKEN", "secret").await,
            Err(AuthError::InvalidPasswordHash)
        ));
    }
}
