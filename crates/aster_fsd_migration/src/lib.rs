//! Append-only `SeaORM` schema history for `AsterFSD` persistence.
//!
//! Migration registration order is part of the database contract. Application
//! repositories consume the resulting schema but do not own schema evolution.

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

pub use sea_orm_migration::prelude::*;

mod m20250101_000001_create_users;
mod m20250101_000002_create_client_whitelist;

/// Ordered migration registry executed by the persistence composition root.
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250101_000001_create_users::Migration),
            Box::new(m20250101_000002_create_client_whitelist::Migration),
        ]
    }
}
