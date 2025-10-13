//! Envelope entity - Represents the envelope budgeting system.
//!
//! Each envelope has a name, category, allocation, balance, and metadata.
//! Envelopes can be individual (per-user) or shared, and support rollover behavior.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Envelope database model
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "envelopes")]
pub struct Model {
    /// Unique identifier for the envelope
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Human-readable name of the envelope (e.g., "Groceries", "Entertainment")
    pub name: String,
    /// Budget category for organization (e.g., "food", "leisure")
    pub category: String,
    /// Monthly allocation amount in dollars
    pub allocation: f64,
    /// Current balance in dollars
    pub balance: f64,
    /// Whether this envelope is user-specific (true) or shared (false)
    pub is_individual: bool,
    /// Discord user ID if this is an individual envelope, None for shared
    pub user_id: Option<String>,
    /// Whether unused funds roll over to next month (true) or reset (false)
    pub rollover: bool,
    /// Soft delete flag - if true, envelope is hidden but data is preserved
    pub is_deleted: bool,
}

/// Defines relationships between Envelope and other entities
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// One envelope has many transactions
    #[sea_orm(has_many = "super::transaction::Entity")]
    Transactions,
    /// One envelope has many products
    #[sea_orm(has_many = "super::product::Entity")]
    Products,
}

impl Related<super::transaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Transactions.def()
    }
}

impl Related<super::product::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Products.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
