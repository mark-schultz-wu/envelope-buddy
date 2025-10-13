//! Product entity - Represents predefined products with fixed prices.
//!
//! Products are used for quick expense logging via `/use_product` command.
//! Each product has a name, price, envelope association, and metadata.
//! Products are templates - when used, they create transactions with the product name in description.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Product database model
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "products")]
pub struct Model {
    /// Unique identifier for the product
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Name of the product (e.g., "Coffee", "Movie Ticket")
    pub name: String,
    /// Fixed price per unit in dollars
    pub price: f64,
    /// ID of the envelope this product charges to
    pub envelope_id: i64,
    /// Soft delete flag - if true, product is hidden but data is preserved
    pub is_deleted: bool,
    /// When the product was created
    pub created_at: DateTime,
    /// When the product was last modified
    pub updated_at: DateTime,
}

/// Defines relationships between Product and other entities
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Each product belongs to one envelope
    #[sea_orm(
        belongs_to = "super::envelope::Entity",
        from = "Column::EnvelopeId",
        to = "super::envelope::Column::Id"
    )]
    Envelope,
}

impl Related<super::envelope::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Envelope.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
