//! Transaction entity - Represents all financial transactions in the system.
//!
//! Each transaction has an `envelope_id`, amount, description, timestamp, `user_id`,
//! optional `message_id` (Discord reference), and `transaction_type` (spend/addfunds).
//! Backticks are used for field names to enable proper documentation linking.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Transaction database model
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    /// Unique identifier for the transaction
    #[sea_orm(primary_key)]
    pub id: i64,
    /// ID of the envelope this transaction belongs to
    pub envelope_id: i64,
    /// Transaction amount (positive for income, negative for spending)
    pub amount: f64,
    /// Human-readable description of the transaction
    pub description: String,
    /// When the transaction was created
    pub timestamp: DateTimeUtc,
    /// Discord user ID who created the transaction
    pub user_id: String,
    /// Optional Discord message ID for tracking original command
    pub message_id: Option<String>,
    /// Type of transaction: `"spend"`, `"addfunds"`, or `"use_product"`
    pub transaction_type: String,
}

/// Defines relationships between Transaction and other entities
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    /// Each transaction belongs to one envelope
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
