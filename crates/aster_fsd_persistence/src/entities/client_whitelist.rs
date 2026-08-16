//! `client_whitelist` table entity.

use sea_orm::entity::prelude::*;

/// Persistent allow-list record for one client-software identifier.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "client_whitelist")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub client_id: String,
    pub client_name: String,
    pub enabled: bool,
    pub created_at: DateTimeUtc,
}

/// The client allow-list currently has no declared entity relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
