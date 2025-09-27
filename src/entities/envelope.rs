//! Envelope entity - Represents the envelope budgeting system.
//! Each envelope has a name, user_id (optional for shared envelopes), category,
//! monthly allocation, current balance, and metadata. Envelopes can have many transactions.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "envelopes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub name: String,
    pub user_id: Option<String>,
    pub category: String,
    pub monthly_allocation: f64,
    pub current_balance: f64,
    pub is_deleted: bool,
    pub created_at: DateTime,
    pub updated_at: DateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::transaction::Entity")]
    Transactions,
}

impl Related<super::transaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Transactions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
