//! Transaction business logic - Handles all transaction-related operations.
//!
//! This module provides functions for creating, retrieving, updating, and managing transactions
//! within the envelope system. All transaction operations automatically update envelope balances
//! to maintain data consistency. The module includes comprehensive validation to prevent invalid
//! transactions such as zero amounts or transactions that would result in negative envelope balances.
//! All functions are async and return Result types for proper error handling throughout the system.

use crate::{
    entities::{Envelope, transaction},
    errors::{Error, Result},
};
use sea_orm::{QueryOrder, Set, TransactionTrait, prelude::*};

/// Creates a new transaction and automatically updates the envelope balance.
///
/// This function validates the transaction amount, ensures the envelope exists and is not deleted,
/// and checks that the transaction won't result in a negative envelope balance. Upon successful
/// creation, the envelope's balance is automatically updated to reflect the new transaction amount.
///
/// # Arguments
/// * `envelope_id` - The envelope to transact against
/// * `amount` - Transaction amount (positive for income, negative for expenses)
/// * `description` - Description of the transaction
/// * `user_id` - Discord user ID who created the transaction
/// * `message_id` - Optional Discord message ID for reference
/// * `transaction_type` - Type of transaction ("spend", "addfunds", etc.)
pub async fn create_transaction(
    db: &DatabaseConnection,
    envelope_id: i64,
    amount: f64,
    description: String,
    user_id: String,
    message_id: Option<String>,
    transaction_type: String,
) -> Result<transaction::Model> {
    if amount == 0.0 {
        return Err(Error::InvalidAmount { amount });
    }

    if !amount.is_finite() {
        return Err(Error::InvalidAmount { amount });
    }

    // Use a transaction to ensure atomicity
    let txn = db.begin().await?;

    let envelope = Envelope::find_by_id(envelope_id)
        .one(&txn)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })?;

    if envelope.is_deleted {
        return Err(Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        });
    }

    // Check if the resulting balance would be negative (for spending)
    // This is a preliminary check - the atomic update will ensure consistency
    let new_balance = envelope.balance + amount;
    if new_balance < 0.0 {
        return Err(Error::InsufficientFunds {
            current: envelope.balance,
            required: -amount,
        });
    }

    let now = chrono::Utc::now();
    let transaction_model = transaction::ActiveModel {
        envelope_id: Set(envelope_id),
        amount: Set(amount),
        description: Set(description),
        timestamp: Set(now),
        user_id: Set(user_id),
        message_id: Set(message_id),
        transaction_type: Set(transaction_type),
        ..Default::default()
    };

    let result = transaction_model.insert(&txn).await?;

    // Atomically update the balance
    crate::core::envelope::update_envelope_balance_atomic(&txn, envelope_id, amount).await?;

    // Commit the transaction
    txn.commit().await?;

    Ok(result)
}

/// Retrieves all transactions for a specific envelope, ordered by timestamp (newest first).
///
/// This function is commonly used to display transaction history for an envelope, allowing users
/// to see all financial activity associated with a particular envelope. The results are ordered
/// chronologically with the most recent transactions appearing first for better user experience.
pub async fn get_transactions_for_envelope(
    db: &DatabaseConnection,
    envelope_id: i64,
) -> Result<Vec<transaction::Model>> {
    crate::entities::Transaction::find()
        .filter(transaction::Column::EnvelopeId.eq(envelope_id))
        .order_by_desc(transaction::Column::Timestamp)
        .all(db)
        .await
        .map_err(Into::into)
}

/// Retrieves a specific transaction by its unique ID.
///
/// This function is used for transaction lookups when users need to view, update, or delete
/// a particular transaction. It returns None if the transaction doesn't exist, allowing callers
/// to handle missing transactions gracefully without throwing errors.
pub async fn get_transaction_by_id(
    db: &DatabaseConnection,
    transaction_id: i64,
) -> Result<Option<transaction::Model>> {
    crate::entities::Transaction::find_by_id(transaction_id)
        .one(db)
        .await
        .map_err(Into::into)
}

/// Deletes a transaction and automatically reverses its effect on the envelope balance.
///
/// This function is used for transaction corrections and cancellations. When a transaction is
/// deleted, the envelope's balance is automatically adjusted by subtracting the transaction amount,
/// ensuring that the envelope balance remains accurate and consistent with the remaining transactions.
pub async fn delete_transaction(db: &DatabaseConnection, transaction_id: i64) -> Result<()> {
    // Use a transaction to ensure atomicity
    let txn = db.begin().await?;

    let transaction = crate::entities::Transaction::find_by_id(transaction_id)
        .one(&txn)
        .await?
        .ok_or_else(|| Error::Config {
            message: "Transaction not found".to_string(),
        })?;

    // Verify envelope exists
    Envelope::find_by_id(transaction.envelope_id)
        .one(&txn)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: transaction.envelope_id.to_string(),
        })?;

    let envelope_id = transaction.envelope_id;
    let amount_to_reverse = -transaction.amount; // Negate to reverse the transaction

    // Delete the transaction
    transaction.delete(&txn).await?;

    // Atomically update the balance by reversing the transaction amount
    crate::core::envelope::update_envelope_balance_atomic(&txn, envelope_id, amount_to_reverse)
        .await?;

    // Commit the transaction
    txn.commit().await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::entities::envelope;
    use crate::test_utils::*;
    use sea_orm::{DatabaseBackend, MockDatabase};

    #[tokio::test]
    async fn test_create_transaction_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test zero amount validation
        let result = create_transaction(
            &db,
            1,
            0.0,
            "test".to_string(),
            "user1".to_string(),
            None,
            "spend".to_string(),
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: 0.0 }
        ));

        // Test NaN validation
        let result = create_transaction(
            &db,
            1,
            f64::NAN,
            "test".to_string(),
            "user1".to_string(),
            None,
            "spend".to_string(),
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        // Test infinity validation
        let result = create_transaction(
            &db,
            1,
            f64::INFINITY,
            "test".to_string(),
            "user1".to_string(),
            None,
            "spend".to_string(),
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        // Test negative infinity validation
        let result = create_transaction(
            &db,
            1,
            f64::NEG_INFINITY,
            "test".to_string(),
            "user1".to_string(),
            None,
            "spend".to_string(),
        )
        .await;
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

        let result = create_transaction(
            &db,
            999,
            50.0,
            "test".to_string(),
            "user1".to_string(),
            None,
            "spend".to_string(),
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::EnvelopeNotFound { name: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_transaction_insufficient_funds() -> Result<()> {
        let envelope_with_low_balance = envelope::Model {
            id: 1,
            name: "Low Balance Envelope".to_string(),
            category: "necessary".to_string(),
            allocation: 100.0,
            balance: 10.0, // Low balance
            is_individual: false,
            user_id: None,
            rollover: false,
            is_deleted: false,
        };

        // Configure MockDatabase to return envelope with low balance
        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_query_results([vec![envelope_with_low_balance]])
            .into_connection();

        // Try to spend more than available balance
        let result = create_transaction(
            &db,
            1,
            -20.0,
            "test".to_string(),
            "user1".to_string(),
            None,
            "spend".to_string(),
        )
        .await;
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
    async fn test_get_transactions_for_envelope_empty() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Test getting transactions for envelope with no transactions
        let transactions = get_transactions_for_envelope(&db, envelope.id).await?;
        assert_eq!(transactions.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transactions_for_envelope_different_envelopes() -> Result<()> {
        let db = setup_test_db().await?;

        // Create two envelopes
        let envelope1 = create_test_envelope(&db, "Envelope 1").await?;
        let envelope2 = create_test_envelope(&db, "Envelope 2").await?;

        // Create transactions for different envelopes
        let created_transaction1 = create_test_transaction(&db, envelope1.id, 50.0).await?;
        let created_transaction2 = create_test_transaction(&db, envelope2.id, 75.0).await?;

        // Test that each envelope only gets its own transactions
        let queried_transactions1 = get_transactions_for_envelope(&db, envelope1.id).await?;
        let queried_transactions2 = get_transactions_for_envelope(&db, envelope2.id).await?;

        assert_eq!(queried_transactions1.len(), 1);
        assert_eq!(queried_transactions1[0], created_transaction1);

        assert_eq!(queried_transactions2.len(), 1);
        assert_eq!(queried_transactions2[0], created_transaction2);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transactions_for_envelope_integration() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create multiple transactions
        let transaction1 = create_test_transaction(&db, envelope.id, 50.0).await?;
        let transaction2 = create_test_transaction(&db, envelope.id, -25.0).await?;

        // Test getting transactions for the envelope
        let all_transactions = get_transactions_for_envelope(&db, envelope.id).await?;
        assert_eq!(all_transactions.len(), 2);

        // Test that they're ordered by timestamp (newest first)
        assert_eq!(all_transactions[0], transaction2);
        assert_eq!(all_transactions[1], transaction1);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_transaction_by_id_integration() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create a transaction
        let transaction = create_test_transaction(&db, envelope.id, 50.0).await?;

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

        let transaction = get_transaction_by_id(&db, 999).await?;
        assert!(transaction.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_transaction_integration() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create a transaction
        let transaction = create_test_transaction(&db, envelope.id, 50.0).await?;

        assert_eq!(transaction.envelope_id, envelope.id);
        assert_eq!(transaction.amount, 50.0);
        assert_eq!(transaction.description, "Test transaction");
        assert_eq!(transaction.user_id, "test_user");
        assert_eq!(transaction.transaction_type, "addfunds");

        // Verify envelope balance was updated
        let updated_envelope = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(updated_envelope.balance, 50.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_user_id_stored_correctly() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create transaction with specific user_id
        let transaction = create_custom_transaction(
            &db,
            envelope.id,
            25.0,
            "Test transaction",
            "user456",
            None,
            "addfunds",
        )
        .await?;

        assert_eq!(transaction.user_id, "user456");

        // Verify persistence
        let retrieved = crate::entities::Transaction::find_by_id(transaction.id)
            .one(&db)
            .await?
            .unwrap();
        assert_eq!(retrieved.user_id, "user456");

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_message_id_stored_correctly() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create transaction with message_id
        let with_message = create_custom_transaction(
            &db,
            envelope.id,
            30.0,
            "With message",
            "user1",
            Some("msg_12345".to_string()),
            "spend",
        )
        .await?;

        assert_eq!(with_message.message_id, Some("msg_12345".to_string()));

        // Create transaction without message_id
        let without_message = create_custom_transaction(
            &db,
            envelope.id,
            20.0,
            "Without message",
            "user1",
            None,
            "spend",
        )
        .await?;

        assert_eq!(without_message.message_id, None);

        // Verify persistence
        let retrieved_with = crate::entities::Transaction::find_by_id(with_message.id)
            .one(&db)
            .await?
            .unwrap();
        assert_eq!(retrieved_with.message_id, Some("msg_12345".to_string()));

        let retrieved_without = crate::entities::Transaction::find_by_id(without_message.id)
            .one(&db)
            .await?
            .unwrap();
        assert_eq!(retrieved_without.message_id, None);

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_type_stored_correctly() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // First add some funds so we can spend
        crate::core::envelope::update_envelope_balance_atomic(&db, envelope.id, 100.0).await?;

        // Create spend transaction
        let spend = create_custom_transaction(
            &db,
            envelope.id,
            -15.0,
            "Spend transaction",
            "user1",
            None,
            "spend",
        )
        .await?;

        assert_eq!(spend.transaction_type, "spend");

        // Create addfunds transaction
        let addfunds = create_custom_transaction(
            &db,
            envelope.id,
            50.0,
            "Add funds transaction",
            "user1",
            None,
            "addfunds",
        )
        .await?;

        assert_eq!(addfunds.transaction_type, "addfunds");

        // Create use_product transaction
        let use_product = create_custom_transaction(
            &db,
            envelope.id,
            -10.0,
            "2x Coffee",
            "user1",
            None,
            "use_product",
        )
        .await?;

        assert_eq!(use_product.transaction_type, "use_product");

        // Verify persistence
        let retrieved_spend = crate::entities::Transaction::find_by_id(spend.id)
            .one(&db)
            .await?
            .unwrap();
        assert_eq!(retrieved_spend.transaction_type, "spend");

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_description_required() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Description is now required (not Option<String>)
        let transaction = create_custom_transaction(
            &db,
            envelope.id,
            25.0,
            "This is a required description",
            "user1",
            None,
            "addfunds",
        )
        .await?;

        assert_eq!(transaction.description, "This is a required description");

        Ok(())
    }

    #[tokio::test]
    async fn test_transaction_timestamp_field() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        let before = chrono::Utc::now();
        let transaction = create_test_transaction(&db, envelope.id, 100.0).await?;
        let after = chrono::Utc::now();

        // Timestamp should be between before and after
        assert!(transaction.timestamp >= before);
        assert!(transaction.timestamp <= after);

        // Verify persistence
        let retrieved = crate::entities::Transaction::find_by_id(transaction.id)
            .one(&db)
            .await?
            .unwrap();
        assert_eq!(retrieved.timestamp, transaction.timestamp);

        Ok(())
    }
}
