//! Transaction business logic - Handles all transaction-related operations.
//! This module provides functions for creating, retrieving, updating, and managing transactions
//! within the envelope system. All transaction operations automatically update envelope balances
//! to maintain data consistency. The module includes comprehensive validation to prevent invalid
//! transactions such as zero amounts or transactions that would result in negative envelope balances.
//! All functions are async and return Result types for proper error handling throughout the system.

use crate::{
    entities::*,
    errors::{Error, Result},
};
use sea_orm::*;

/// Creates a new transaction and automatically updates the envelope balance.
/// This function validates the transaction amount, ensures the envelope exists and is not deleted,
/// optionally verifies the product exists if provided, and checks that the transaction won't result
/// in a negative envelope balance. Upon successful creation, the envelope's current balance is
/// automatically updated to reflect the new transaction amount.
pub async fn create_transaction(
    db: &DatabaseConnection,
    envelope_id: i32,
    product_id: Option<i32>,
    amount: f64,
    description: Option<String>,
) -> Result<transaction::Model> {
    if amount == 0.0 {
        return Err(Error::InvalidAmount { amount });
    }

    if !amount.is_finite() {
        return Err(Error::InvalidAmount { amount });
    }

    let envelope = Envelope::find_by_id(envelope_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })?;

    if envelope.is_deleted {
        return Err(Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        });
    }

    if let Some(pid) = product_id {
        let product =
            Product::find_by_id(pid)
                .one(db)
                .await?
                .ok_or_else(|| Error::ProductNotFound {
                    name: pid.to_string(),
                })?;

        if product.is_deleted {
            return Err(Error::ProductNotFound {
                name: pid.to_string(),
            });
        }
    }

    let new_balance = envelope.current_balance + amount;
    if new_balance < 0.0 {
        return Err(Error::InsufficientFunds {
            current: envelope.current_balance,
            required: -amount,
        });
    }

    let now = chrono::Utc::now().naive_utc();
    let transaction = transaction::ActiveModel {
        envelope_id: Set(envelope_id),
        product_id: Set(product_id),
        amount: Set(amount),
        description: Set(description),
        created_at: Set(now),
        ..Default::default()
    };

    let result = transaction.insert(db).await?;
    crate::core::envelope::update_envelope_balance(db, envelope_id, new_balance).await?;
    Ok(result)
}

/// Retrieves all transactions for a specific envelope, ordered by creation date (newest first).
/// This function is commonly used to display transaction history for an envelope, allowing users
/// to see all financial activity associated with a particular envelope. The results are ordered
/// chronologically with the most recent transactions appearing first for better user experience.
pub async fn get_transactions_for_envelope(
    db: &DatabaseConnection,
    envelope_id: i32,
) -> Result<Vec<transaction::Model>> {
    crate::entities::Transaction::find()
        .filter(transaction::Column::EnvelopeId.eq(envelope_id))
        .order_by_desc(transaction::Column::CreatedAt)
        .all(db)
        .await
        .map_err(Into::into)
}

/// Retrieves a specific transaction by its unique ID.
/// This function is used for transaction lookups when users need to view, update, or delete
/// a particular transaction. It returns None if the transaction doesn't exist, allowing callers
/// to handle missing transactions gracefully without throwing errors.
pub async fn get_transaction_by_id(
    db: &DatabaseConnection,
    transaction_id: i32,
) -> Result<Option<transaction::Model>> {
    crate::entities::Transaction::find_by_id(transaction_id)
        .one(db)
        .await
        .map_err(Into::into)
}

/// Deletes a transaction and automatically reverses its effect on the envelope balance.
/// This function is used for transaction corrections and cancellations. When a transaction is
/// deleted, the envelope's balance is automatically adjusted by subtracting the transaction amount,
/// ensuring that the envelope balance remains accurate and consistent with the remaining transactions.
pub async fn delete_transaction(db: &DatabaseConnection, transaction_id: i32) -> Result<()> {
    let transaction = crate::entities::Transaction::find_by_id(transaction_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::Config {
            message: "Transaction not found".to_string(),
        })?;

    let envelope = Envelope::find_by_id(transaction.envelope_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: transaction.envelope_id.to_string(),
        })?;

    let new_balance = envelope.current_balance - transaction.amount;
    let envelope_id = transaction.envelope_id; // Store the ID before moving transaction

    transaction.delete(db).await?;
    crate::core::envelope::update_envelope_balance(db, envelope_id, new_balance).await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase};

    #[tokio::test]
    async fn test_create_transaction_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test zero amount validation
        let result = create_transaction(&db, 1, None, 0.0, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: 0.0 }
        ));

        // Test NaN validation
        let result = create_transaction(&db, 1, None, f64::NAN, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        // Test infinity validation
        let result = create_transaction(&db, 1, None, f64::INFINITY, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        // Test negative infinity validation
        let result = create_transaction(&db, 1, None, f64::NEG_INFINITY, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_transaction_envelope_not_found() -> Result<()> {
        // Configure MockDatabase to return no envelope (simulating not found)
        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_query_results([Vec::<envelope::Model>::new()])
            .into_connection();

        let result = create_transaction(&db, 999, None, 50.0, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::EnvelopeNotFound { name: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_transaction_insufficient_funds() -> Result<()> {
        let now = chrono::Utc::now().naive_utc();
        let envelope_with_low_balance = envelope::Model {
            id: 1,
            name: "Low Balance Envelope".to_string(),
            user_id: None,
            category: "necessary".to_string(),
            monthly_allocation: 100.0,
            current_balance: 10.0, // Low balance
            is_deleted: false,
            created_at: now,
            updated_at: now,
        };

        // Configure MockDatabase to return envelope with low balance
        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_query_results([vec![envelope_with_low_balance]])
            .into_connection();

        // Try to spend more than available balance - this WILL test our business logic
        let result = create_transaction(&db, 1, None, -20.0, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InsufficientFunds {
                current: 10.0,
                required: 20.0
            }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_transaction_product_not_found() -> Result<()> {
        // Use real database for this test since MockDatabase has type conflicts
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope first
        let envelope = crate::core::envelope::create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Try to create transaction with non-existent product
        let result = create_transaction(&db, envelope.id, Some(999), 30.0, None).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::ProductNotFound { name: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transactions_for_envelope_empty() -> Result<()> {
        // Use real database to test empty result handling
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope
        let envelope = crate::core::envelope::create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Test getting transactions for envelope with no transactions
        let transactions = get_transactions_for_envelope(&db, envelope.id).await?;
        assert_eq!(transactions.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transactions_for_envelope_different_envelopes() -> Result<()> {
        // Use real database to test that transactions are filtered by envelope
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create two envelopes
        let envelope1 = crate::core::envelope::create_envelope(
            &db,
            "Envelope 1".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        let envelope2 = crate::core::envelope::create_envelope(
            &db,
            "Envelope 2".to_string(),
            None,
            "necessary".to_string(),
            200.0,
        )
        .await?;

        // Create transactions for different envelopes
        let transaction1 = create_transaction(
            &db,
            envelope1.id,
            None,
            50.0,
            Some("Envelope 1 transaction".to_string()),
        )
        .await?;

        let transaction2 = create_transaction(
            &db,
            envelope2.id,
            None,
            75.0,
            Some("Envelope 2 transaction".to_string()),
        )
        .await?;

        // Test that each envelope only gets its own transactions
        let transactions1 = get_transactions_for_envelope(&db, envelope1.id).await?;
        let transactions2 = get_transactions_for_envelope(&db, envelope2.id).await?;

        assert_eq!(transactions1.len(), 1);
        assert_eq!(transactions1[0], transaction1);

        assert_eq!(transactions2.len(), 1);
        assert_eq!(transactions2[0], transaction2);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transactions_for_envelope_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope
        let envelope = crate::core::envelope::create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Create multiple transactions
        let transaction1 = create_transaction(
            &db,
            envelope.id,
            None,
            50.0,
            Some("Transaction 1".to_string()),
        )
        .await?;

        let transaction2 = create_transaction(
            &db,
            envelope.id,
            None,
            -25.0,
            Some("Transaction 2".to_string()),
        )
        .await?;

        // Test getting transactions for the envelope
        let transactions = get_transactions_for_envelope(&db, envelope.id).await?;
        assert_eq!(transactions.len(), 2);

        // Test that they're ordered by creation date (newest first)
        assert_eq!(transactions[0], transaction2);
        assert_eq!(transactions[1], transaction1);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transaction_by_id_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope
        let envelope = crate::core::envelope::create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Create a transaction
        let transaction = create_transaction(
            &db,
            envelope.id,
            None,
            50.0,
            Some("Test transaction".to_string()),
        )
        .await?;

        // Test finding the transaction by ID
        let found_transaction = get_transaction_by_id(&db, transaction.id).await?;
        assert!(found_transaction.is_some());
        let found = found_transaction.unwrap();
        assert_eq!(found, transaction);

        // Test finding non-existent transaction
        let not_found = get_transaction_by_id(&db, 999).await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transaction_by_id_not_found() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_query_results([Vec::<transaction::Model>::new()])
            .into_connection();

        // This WILL execute our query logic and test the None case
        let transaction = get_transaction_by_id(&db, 999).await?;
        assert!(transaction.is_none());

        Ok(())
    }

    // Keep the integration tests as they are - they're already good!
    #[tokio::test]
    async fn test_create_transaction_integration() -> Result<()> {
        // Use real database for integration test
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope first
        let envelope = crate::core::envelope::create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Create a transaction
        let transaction = create_transaction(
            &db,
            envelope.id,
            None,
            50.0,
            Some("Test transaction".to_string()),
        )
        .await?;

        assert_eq!(transaction.envelope_id, envelope.id);
        assert_eq!(transaction.amount, 50.0);

        // Verify envelope balance was updated
        let updated_envelope = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(updated_envelope.current_balance, 50.0);

        Ok(())
    }
}
