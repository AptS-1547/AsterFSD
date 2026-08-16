//! `users` table entity.

use aster_fsd_model::{AtcRating, PilotRating};
use sea_orm::entity::prelude::*;

/// Persistent network identity and its Argon2 password hash.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub network_id: String,
    pub password_hash: String,
    pub real_name: String,
    pub atc_rating: AtcRating,
    pub pilot_rating: PilotRating,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

/// The user table currently has no declared entity relations.
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
