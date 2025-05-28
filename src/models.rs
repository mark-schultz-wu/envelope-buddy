use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Based on your design document's "envelopes" table
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Envelope {
    pub id: i64, // Primary Key, typically INTEGER
    pub name: String,
    pub category: String, // "necessary" or "quality_of_life" - consider an enum
    pub allocation: f64,  // REAL
    pub balance: f64,     // REAL
    pub is_individual: bool, // BOOLEAN
    pub user_id: Option<String>, // TEXT (Discord User ID, NULL if shared)
    // This implies that for Option A of individual envelopes,
    // two rows will exist for an individual type like "hobby",
    // one for each user ID.
    pub rollover: bool,   // BOOLEAN
    pub is_deleted: bool, // For soft deletes, BOOLEAN, default false
}

// Based on your "transactions" table
#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub id: i64,
    pub envelope_id: i64,
    pub amount: f64,
    pub description: String,
    pub timestamp: DateTime<Utc>,   // DATETIME
    pub user_id: String,            // TEXT (Discord User ID of spender)
    pub message_id: Option<String>, // TEXT (Discord Message ID for reference/editing)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Product {
    pub id: i64,
    pub name: String,
    pub price: f64,
    pub envelope_id: i64,
    pub description: Option<String>,
    // This field is not in the DB table but can be populated by JOINs for display
    #[serde(default)] // Handles cases where it might not be loaded from DB directly
    pub envelope_name: Option<String>,
}
