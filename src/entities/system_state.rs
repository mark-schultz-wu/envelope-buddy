//! System state entity - Stores key-value pairs for system configuration.
//! Used for storing system-wide settings like monthly update status,
//! rollover settings, and other configuration data.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// System state database model - stores key-value configuration pairs
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "system_state")]
pub struct Model {
    /// Unique identifier
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Configuration key (e.g., `"last_monthly_update"`)
    pub key: String,
    /// Configuration value stored as string
    pub value: String,
    /// When this configuration was last modified
    pub updated_at: DateTime,
}

/// `SystemState` has no relationships with other entities
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
