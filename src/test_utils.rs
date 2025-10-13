//! Shared test utilities for `EnvelopeBuddy`.
//!
//! This module provides common helper functions for setting up test databases
//! and creating test entities with sensible defaults.

use crate::{
    core::{envelope, product, transaction},
    entities,
    errors::Result,
};
use sea_orm::DatabaseConnection;

/// Creates an in-memory `SQLite` database with all tables initialized.
/// This is the standard setup for all integration tests.
pub async fn setup_test_db() -> Result<DatabaseConnection> {
    let db = sea_orm::Database::connect("sqlite::memory:").await?;
    crate::config::database::create_tables(&db).await?;
    Ok(db)
}

/// Creates a test envelope with sensible defaults.
///
/// # Arguments
/// * `db` - Database connection
/// * `name` - Envelope name
///
/// # Defaults
/// * `user_id`: None (shared envelope)
/// * `category`: "necessary"
/// * `allocation`: 100.0
/// * `is_individual`: false
/// * `rollover`: false
pub async fn create_test_envelope(
    db: &DatabaseConnection,
    name: &str,
) -> Result<entities::envelope::Model> {
    envelope::create_envelope(
        db,
        name.to_string(),
        None,
        "necessary".to_string(),
        100.0,
        false, // is_individual
        false, // rollover
    )
    .await
}

/// Creates a test envelope with custom parameters.
/// Use this when you need to test specific envelope configurations.
pub async fn create_custom_envelope(
    db: &DatabaseConnection,
    name: &str,
    user_id: Option<String>,
    category: &str,
    allocation: f64,
    is_individual: bool,
    rollover: bool,
) -> Result<entities::envelope::Model> {
    envelope::create_envelope(
        db,
        name.to_string(),
        user_id,
        category.to_string(),
        allocation,
        is_individual,
        rollover,
    )
    .await
}

/// Creates a test product with sensible defaults.
///
/// # Arguments
/// * `db` - Database connection
/// * `name` - Product name
/// * `envelope_id` - Associated envelope ID
///
/// # Defaults
/// * price: 10.0
pub async fn create_test_product(
    db: &DatabaseConnection,
    name: &str,
    envelope_id: i64,
) -> Result<entities::product::Model> {
    product::create_product(db, name.to_string(), 10.0, envelope_id).await
}

/// Creates a test product with custom price.
pub async fn create_custom_product(
    db: &DatabaseConnection,
    name: &str,
    price: f64,
    envelope_id: i64,
) -> Result<entities::product::Model> {
    product::create_product(db, name.to_string(), price, envelope_id).await
}

/// Creates a test transaction with sensible defaults.
///
/// # Arguments
/// * `db` - Database connection
/// * `envelope_id` - Associated envelope ID
/// * `amount` - Transaction amount (positive for income, negative for expense)
///
/// # Defaults
/// * `description`: `"Test transaction"`
/// * `user_id`: `"test_user"`
/// * `message_id`: None
/// * `transaction_type`: "spend" (if negative) or "addfunds" (if positive)
pub async fn create_test_transaction(
    db: &DatabaseConnection,
    envelope_id: i64,
    amount: f64,
) -> Result<entities::transaction::Model> {
    let transaction_type = if amount < 0.0 { "spend" } else { "addfunds" };

    transaction::create_transaction(
        db,
        envelope_id,
        amount,
        "Test transaction".to_string(),
        "test_user".to_string(),
        None,
        transaction_type.to_string(),
    )
    .await
}

/// Creates a test transaction with custom parameters.
pub async fn create_custom_transaction(
    db: &DatabaseConnection,
    envelope_id: i64,
    amount: f64,
    description: &str,
    user_id: &str,
    message_id: Option<String>,
    transaction_type: &str,
) -> Result<entities::transaction::Model> {
    transaction::create_transaction(
        db,
        envelope_id,
        amount,
        description.to_string(),
        user_id.to_string(),
        message_id,
        transaction_type.to_string(),
    )
    .await
}

/// Sets up a complete test environment with an envelope.
/// Returns (db, envelope) for common test scenarios.
pub async fn setup_with_envelope() -> Result<(DatabaseConnection, entities::envelope::Model)> {
    let db = setup_test_db().await?;
    let envelope = create_test_envelope(&db, "Test Envelope").await?;
    Ok((db, envelope))
}

/// Sets up a complete test environment with envelope and product.
/// Returns (db, envelope, product) for product-related tests.
pub async fn setup_with_product() -> Result<(
    DatabaseConnection,
    entities::envelope::Model,
    entities::product::Model,
)> {
    let db = setup_test_db().await?;
    let envelope = create_test_envelope(&db, "Test Envelope").await?;
    let product = create_test_product(&db, "Test Product", envelope.id).await?;
    Ok((db, envelope, product))
}
