//! Typed TOML configuration and conversion into crate-owned runtime settings.

use aster_forge_logging::LoggingConfig;
use aster_fsd_core::CoreConfig;
use aster_fsd_persistence::{
    DatabaseConfig as RuntimeDatabaseConfig, SqlxLogLevel as RuntimeSqlxLogLevel,
};
use aster_fsd_protocol::ProtocolDialect;
use aster_fsd_server::{
    ListenerConfig as RuntimeListenerConfig, ServerConfig as RuntimeServerConfig,
};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use thiserror::Error;

/// Configuration validation failures detected before any listener binds.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("server name must not be empty")]
    EmptyServerName,
    #[error("server product message must not be empty or contain line delimiters")]
    InvalidProductMessage,
    #[error("at least one listener is required")]
    NoListeners,
    #[error("listener name and address/port pairs must be unique")]
    DuplicateListener,
    #[error("listener frame limits and timeouts must be greater than zero")]
    InvalidListenerLimit,
    #[error("classic listener frame limit must not exceed 511 bytes")]
    ClassicFrameLimit,
    #[error("server client/mailbox/maintenance limits must be greater than zero")]
    InvalidServerLimit,
    #[error("database connection limits are invalid")]
    InvalidDatabasePool,
    #[error("logging level must not be empty")]
    EmptyLoggingLevel,
}

/// Complete service configuration loaded from `config.toml` or defaults.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default = "default_listeners")]
    pub listeners: Vec<ListenerConfig>,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub database: DatabaseConfig,
}

/// Product identity and shared runtime limits.
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    #[serde(default = "default_server_name")]
    pub name: String,
    #[serde(default = "default_server_version")]
    pub version: String,
    /// First informational message delivered after a successful login.
    #[serde(default = "default_product_message")]
    pub product_message: String,
    #[serde(default = "default_max_clients")]
    pub max_clients: usize,
    #[serde(default = "default_mailbox_capacity")]
    pub mailbox_capacity: usize,
    #[serde(default = "default_wind_delta_interval_seconds")]
    pub wind_delta_interval_seconds: u64,
    #[serde(default)]
    pub motd: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: default_server_name(),
            version: default_server_version(),
            product_message: default_product_message(),
            max_clients: default_max_clients(),
            mailbox_capacity: default_mailbox_capacity(),
            wind_delta_interval_seconds: default_wind_delta_interval_seconds(),
            motd: Vec::new(),
        }
    }
}

/// Configuration of one named TCP protocol listener.
#[derive(Debug, Deserialize, Clone)]
pub struct ListenerConfig {
    pub name: String,
    pub protocol: ProtocolDialect,
    #[serde(default = "default_listener_address")]
    pub address: String,
    pub port: u16,
    #[serde(default = "default_classic_frame_bytes")]
    pub max_frame_bytes: usize,
    #[serde(default = "default_idle_timeout_seconds")]
    pub idle_timeout_seconds: u64,
}

/// User-facing `SQLx` statement-log level.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SqlxLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl SqlxLogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

impl From<SqlxLogLevel> for RuntimeSqlxLogLevel {
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

/// Persistent database pool and statement logging settings.
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    #[serde(default = "default_database_url")]
    pub url: String,
    #[serde(default = "default_database_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_database_min_connections")]
    pub min_connections: u32,
    #[serde(default)]
    pub sqlx_logging: bool,
    #[serde(default = "default_sqlx_log_level")]
    pub sqlx_logging_level: SqlxLogLevel,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_database_url(),
            max_connections: default_database_max_connections(),
            min_connections: default_database_min_connections(),
            sqlx_logging: false,
            sqlx_logging_level: default_sqlx_log_level(),
        }
    }
}

impl Config {
    /// Reads and deserializes a TOML configuration file.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the file cannot be read or a TOML decode
    /// error when its contents do not match the typed configuration schema.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(toml::from_str(&fs::read_to_string(path)?)?)
    }

    #[must_use]
    /// Builds the effective tracing configuration, adding the `SQLx` directive
    /// only when statement logging was explicitly enabled.
    pub fn effective_logging_config(&self) -> LoggingConfig {
        let mut logging = self.logging.clone();
        if self.database.sqlx_logging {
            logging.level = format!(
                "{},sqlx={}",
                logging.level,
                self.database.sqlx_logging_level.as_str()
            );
        }
        logging
    }

    /// Validates cross-field limits and uniqueness before runtime construction.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for empty identities or listener sets, duplicate
    /// bind targets, invalid pool/capacity values, or unsafe classic frame size.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server.name.trim().is_empty() {
            return Err(ConfigError::EmptyServerName);
        }
        if self.server.product_message.trim().is_empty()
            || self.server.product_message.contains(['\r', '\n'])
        {
            return Err(ConfigError::InvalidProductMessage);
        }
        if self.logging.level.trim().is_empty() {
            return Err(ConfigError::EmptyLoggingLevel);
        }
        if self.server.max_clients == 0
            || self.server.mailbox_capacity == 0
            || self.server.wind_delta_interval_seconds == 0
        {
            return Err(ConfigError::InvalidServerLimit);
        }
        if self.listeners.is_empty() {
            return Err(ConfigError::NoListeners);
        }
        let mut names = HashSet::new();
        let mut addresses = HashSet::new();
        for listener in &self.listeners {
            if listener.max_frame_bytes == 0 || listener.idle_timeout_seconds == 0 {
                return Err(ConfigError::InvalidListenerLimit);
            }
            if listener.protocol == ProtocolDialect::Classic && listener.max_frame_bytes > 511 {
                return Err(ConfigError::ClassicFrameLimit);
            }
            if !names.insert(listener.name.as_str())
                || !addresses.insert((listener.address.as_str(), listener.port))
            {
                return Err(ConfigError::DuplicateListener);
            }
        }
        if self.database.min_connections == 0
            || self.database.max_connections == 0
            || self.database.min_connections > self.database.max_connections
        {
            return Err(ConfigError::InvalidDatabasePool);
        }
        Ok(())
    }

    #[must_use]
    /// Converts file configuration into the transport server's owned settings.
    pub fn runtime_server_config(&self) -> RuntimeServerConfig {
        RuntimeServerConfig {
            server_name: self.server.name.clone(),
            server_version: self.server.version.clone(),
            mailbox_capacity: self.server.mailbox_capacity,
            wind_delta_interval_seconds: self.server.wind_delta_interval_seconds,
            listeners: self
                .listeners
                .iter()
                .map(|listener| RuntimeListenerConfig {
                    name: listener.name.clone(),
                    address: listener.address.clone(),
                    port: listener.port,
                    protocol: listener.protocol,
                    max_frame_bytes: listener.max_frame_bytes,
                    idle_timeout_seconds: listener.idle_timeout_seconds,
                })
                .collect(),
        }
    }

    #[must_use]
    /// Converts file configuration into shared network-core policy.
    pub fn runtime_core_config(&self) -> CoreConfig {
        CoreConfig {
            max_clients: self.server.max_clients,
            product_message: self.server.product_message.clone(),
            motd: self.server.motd.clone(),
        }
    }

    #[must_use]
    /// Converts file configuration into the persistence adapter's settings.
    pub fn runtime_database_config(&self) -> RuntimeDatabaseConfig {
        RuntimeDatabaseConfig {
            url: self.database.url.clone(),
            max_connections: self.database.max_connections,
            min_connections: self.database.min_connections,
            sqlx_logging: self.database.sqlx_logging,
            sqlx_logging_level: self.database.sqlx_logging_level.into(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            listeners: default_listeners(),
            logging: LoggingConfig::default(),
            database: DatabaseConfig::default(),
        }
    }
}

fn default_listeners() -> Vec<ListenerConfig> {
    vec![ListenerConfig {
        name: "classic".to_string(),
        protocol: ProtocolDialect::Classic,
        address: default_listener_address(),
        port: 6809,
        max_frame_bytes: default_classic_frame_bytes(),
        idle_timeout_seconds: default_idle_timeout_seconds(),
    }]
}

fn default_server_name() -> String {
    "AsterFSD".to_string()
}
fn default_server_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
fn default_product_message() -> String {
    format!("AsterFSD {}", env!("CARGO_PKG_VERSION"))
}
const fn default_max_clients() -> usize {
    1_000
}
const fn default_mailbox_capacity() -> usize {
    256
}
const fn default_wind_delta_interval_seconds() -> u64 {
    70
}
fn default_listener_address() -> String {
    "0.0.0.0".to_string()
}
const fn default_classic_frame_bytes() -> usize {
    511
}
const fn default_idle_timeout_seconds() -> u64 {
    500
}
fn default_database_url() -> String {
    "sqlite://asterfsd.db".to_string()
}
const fn default_database_max_connections() -> u32 {
    100
}
const fn default_database_min_connections() -> u32 {
    5
}
const fn default_sqlx_log_level() -> SqlxLogLevel {
    SqlxLogLevel::Debug
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_enable_only_classic_listener_and_info_logging() {
        let config = Config::default();
        assert_eq!(config.logging.level, "info");
        assert_eq!(config.server.product_message, "AsterFSD 0.2.0");
        assert_eq!(config.listeners.len(), 1);
        assert_eq!(config.listeners[0].protocol, ProtocolDialect::Classic);
        assert_eq!(config.listeners[0].max_frame_bytes, 511);
        assert!(!config.database.sqlx_logging);
    }

    #[test]
    fn explicit_multi_protocol_listeners_parse() {
        let config: Config = toml::from_str(
            r#"
                [[listeners]]
                name = "classic"
                protocol = "classic"
                port = 6809

                [[listeners]]
                name = "aster"
                protocol = "aster_v1"
                port = 6811
                max_frame_bytes = 16384
            "#,
        )
        .unwrap();
        assert_eq!(config.listeners.len(), 2);
        assert_eq!(config.listeners[1].protocol, ProtocolDialect::AsterV1);
    }

    #[test]
    fn sqlx_filter_is_only_added_when_statement_logging_is_enabled() {
        let mut config = Config::default();
        assert_eq!(config.effective_logging_config().level, "info");
        config.logging.level = "debug".to_string();
        assert_eq!(config.effective_logging_config().level, "debug");
        config.database.sqlx_logging = true;
        assert_eq!(config.effective_logging_config().level, "debug,sqlx=debug");
        config.database.sqlx_logging_level = SqlxLogLevel::Trace;
        assert_eq!(config.effective_logging_config().level, "debug,sqlx=trace");
    }

    #[test]
    fn invalid_listener_and_pool_limits_fail_before_bind() {
        let mut config = Config::default();
        config.listeners[0].max_frame_bytes = 4_096;
        assert_eq!(config.validate(), Err(ConfigError::ClassicFrameLimit));

        let mut config = Config::default();
        config.database.min_connections = 10;
        config.database.max_connections = 5;
        assert_eq!(config.validate(), Err(ConfigError::InvalidDatabasePool));

        let mut config = Config::default();
        config.logging.level = "  ".to_string();
        assert_eq!(config.validate(), Err(ConfigError::EmptyLoggingLevel));
    }

    #[test]
    fn validation_covers_identity_capacity_and_listener_uniqueness() {
        let mut config = Config::default();
        config.server.name = "  ".to_string();
        assert_eq!(config.validate(), Err(ConfigError::EmptyServerName));

        let mut config = Config::default();
        config.server.product_message = "\r\n".to_string();
        assert_eq!(config.validate(), Err(ConfigError::InvalidProductMessage));

        let mut config = Config::default();
        config.server.max_clients = 0;
        assert_eq!(config.validate(), Err(ConfigError::InvalidServerLimit));

        let mut config = Config::default();
        config.listeners.clear();
        assert_eq!(config.validate(), Err(ConfigError::NoListeners));

        let mut config = Config::default();
        config.listeners[0].idle_timeout_seconds = 0;
        assert_eq!(config.validate(), Err(ConfigError::InvalidListenerLimit));

        let mut config = Config::default();
        config.listeners.push(config.listeners[0].clone());
        assert_eq!(config.validate(), Err(ConfigError::DuplicateListener));

        let mut config = Config::default();
        let mut duplicate_address = config.listeners[0].clone();
        duplicate_address.name = "classic-copy".to_string();
        config.listeners.push(duplicate_address);
        assert_eq!(config.validate(), Err(ConfigError::DuplicateListener));

        let mut config = Config::default();
        config.database.min_connections = 0;
        assert_eq!(config.validate(), Err(ConfigError::InvalidDatabasePool));
    }

    #[test]
    fn product_message_is_configurable_and_propagates_to_core() {
        let config: Config = toml::from_str(
            r#"
                [server]
                product_message = "Welcome to the test network"
            "#,
        )
        .unwrap();

        assert_eq!(
            config.runtime_core_config().product_message,
            "Welcome to the test network"
        );
    }
}
