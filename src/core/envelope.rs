//! Envelope business logic - Handles all envelope-related operations.
//!
//! Provides functions for creating, retrieving, updating, and managing envelopes.
//! All functions are async and return Result types for error handling.

use crate::{
    entities::{Envelope, envelope},
    errors::{Error, Result},
};
use sea_orm::{QueryOrder, Set, prelude::*};

/// Retrieves all active (non-deleted) envelopes from the database, ordered alphabetically by name.
///
/// This function is commonly used to display the complete list of available envelopes
/// to users, such as in autocomplete suggestions or envelope selection interfaces.
pub async fn get_all_active_envelopes(db: &DatabaseConnection) -> Result<Vec<envelope::Model>> {
    Envelope::find()
        .filter(envelope::Column::IsDeleted.eq(false))
        .order_by_asc(envelope::Column::Name)
        .all(db)
        .await
        .map_err(Into::into)
}

/// Finds a specific envelope by its name, returning None if not found or deleted.
///
/// This function is used for envelope lookups when users reference envelopes by name
/// in commands, and ensures that deleted envelopes are not accessible.
pub async fn get_envelope_by_name(
    db: &DatabaseConnection,
    name: &str,
) -> Result<Option<envelope::Model>> {
    Envelope::find()
        .filter(envelope::Column::Name.eq(name))
        .filter(envelope::Column::IsDeleted.eq(false))
        .one(db)
        .await
        .map_err(Into::into)
}

/// Finds an envelope by name and user ID, used for user-specific envelope lookups.
///
/// This function is essential for personal envelopes where users can only access
/// their own envelopes, preventing unauthorized access to other users' personal finances.
pub async fn get_envelope_by_name_and_user(
    db: &DatabaseConnection,
    name: &str,
    user_id: &str,
) -> Result<Option<envelope::Model>> {
    Envelope::find()
        .filter(envelope::Column::Name.eq(name))
        .filter(envelope::Column::UserId.eq(user_id))
        .filter(envelope::Column::IsDeleted.eq(false))
        .one(db)
        .await
        .map_err(Into::into)
}

/// Finds an envelope by its unique ID, used for direct envelope lookups.
///
/// This function is used when the envelope ID is known, such as when
/// processing transactions or retrieving envelope details by primary key.
pub async fn get_envelope_by_id(
    db: &DatabaseConnection,
    envelope_id: i64,
) -> Result<Option<envelope::Model>> {
    Envelope::find_by_id(envelope_id)
        .one(db)
        .await
        .map_err(Into::into)
}

/// Creates a new envelope with the specified parameters, performing input validation.
///
/// This function validates that the name is not empty, the allocation is non-negative,
/// and trims whitespace from the name. It initializes the envelope with zero balance.
pub async fn create_envelope(
    db: &DatabaseConnection,
    name: String,
    user_id: Option<String>,
    category: String,
    allocation: f64,
    is_individual: bool,
    rollover: bool,
) -> Result<envelope::Model> {
    // Validate inputs
    if name.trim().is_empty() {
        return Err(Error::Config {
            message: "Envelope name cannot be empty".to_string(),
        });
    }

    if allocation < 0.0 {
        return Err(Error::InvalidAmount { amount: allocation });
    }

    let envelope = envelope::ActiveModel {
        name: Set(name.trim().to_string()),
        user_id: Set(user_id),
        category: Set(category),
        allocation: Set(allocation),
        balance: Set(0.0),
        is_individual: Set(is_individual),
        rollover: Set(rollover),
        is_deleted: Set(false),
        ..Default::default()
    };

    let result = envelope.insert(db).await?;
    Ok(result)
}

/// Updates the balance of an existing envelope by atomically adding an amount.
///
/// This function performs an atomic database-level update to prevent race conditions.
/// Instead of reading the current balance, modifying it, and writing it back (which
/// can lose updates in concurrent scenarios), this uses a single SQL UPDATE statement:
/// `UPDATE envelopes SET balance = balance + amount WHERE id = ?`
///
/// # Arguments
/// * `db` - Database connection or transaction
/// * `envelope_id` - ID of the envelope to update
/// * `amount_delta` - Amount to add to the balance (use negative for subtraction)
///
/// # Returns
/// The updated envelope model
pub async fn update_envelope_balance_atomic<C>(
    db: &C,
    envelope_id: i64,
    amount_delta: f64,
) -> Result<envelope::Model>
where
    C: ConnectionTrait,
{
    use sea_orm::sea_query::Expr;

    // First verify the envelope exists
    let _envelope = Envelope::find_by_id(envelope_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })?;

    // Perform atomic update: balance = balance + amount_delta
    Envelope::update_many()
        .col_expr(
            envelope::Column::Balance,
            Expr::col(envelope::Column::Balance).add(amount_delta),
        )
        .filter(envelope::Column::Id.eq(envelope_id))
        .exec(db)
        .await?;

    // Return the updated envelope
    Envelope::find_by_id(envelope_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })
}

/// Legacy wrapper for backwards compatibility with tests that pass absolute balance values.
/// New code should use `update_envelope_balance_atomic` with amount deltas instead.
#[doc(hidden)]
pub async fn update_envelope_balance(
    db: &DatabaseConnection,
    envelope_id: i64,
    new_balance: f64,
) -> Result<envelope::Model> {
    // Calculate the delta from the current balance
    let envelope = Envelope::find_by_id(envelope_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })?;

    let delta = new_balance - envelope.balance;
    update_envelope_balance_atomic(db, envelope_id, delta).await
}
#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::test_utils::*;
    use sea_orm::{DatabaseBackend, MockDatabase};

    #[tokio::test]
    async fn test_create_envelope_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test empty name validation
        let result = create_envelope(
            &db,
            String::new(),
            None,
            "necessary".to_string(),
            100.0,
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test whitespace-only name validation
        let result = create_envelope(
            &db,
            "   ".to_string(),
            None,
            "necessary".to_string(),
            100.0,
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test negative allocation validation
        let result = create_envelope(
            &db,
            "Test".to_string(),
            None,
            "necessary".to_string(),
            -50.0,
            false,
            false,
        )
        .await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: -50.0 }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_envelope_integration() -> Result<()> {
        let db = setup_test_db().await?;

        let envelope = create_test_envelope(&db, "Test Envelope").await?;

        assert_eq!(envelope.name, "Test Envelope");
        assert_eq!(envelope.allocation, 100.0);
        assert_eq!(envelope.balance, 0.0);
        assert!(!envelope.is_deleted);
        assert!(!envelope.is_individual);
        assert!(!envelope.rollover);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_envelope_by_name_integration() -> Result<()> {
        let db = setup_test_db().await?;

        // Create an envelope
        let created_envelope = create_test_envelope(&db, "Test Envelope").await?;

        // Test finding it by name
        let found_envelope = get_envelope_by_name(&db, "Test Envelope").await?;
        assert!(found_envelope.is_some());
        assert_eq!(found_envelope.unwrap().id, created_envelope.id);

        // Test finding non-existent envelope
        let not_found = get_envelope_by_name(&db, "Non-existent").await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_active_envelopes_integration() -> Result<()> {
        let db = setup_test_db().await?;

        // Create multiple envelopes
        let envelope0 = create_test_envelope(&db, "Envelope 0").await?;

        let envelope1 = create_envelope(
            &db,
            "Envelope 1".to_string(),
            Some("user123".to_string()),
            "wants".to_string(),
            200.0,
            true, // is_individual
            false,
        )
        .await?;

        // Test getting all active envelopes
        let active_envelopes = get_all_active_envelopes(&db).await?;
        assert_eq!(active_envelopes.len(), 2);

        // Test that they're ordered alphabetically
        assert_eq!(active_envelopes[0], envelope0);
        assert_eq!(active_envelopes[1], envelope1);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_envelope_by_name_and_user_integration() -> Result<()> {
        let db = setup_test_db().await?;

        // Create user-specific envelope
        let user_envelope = create_custom_envelope(
            &db,
            "Personal Envelope",
            Some("user123".to_string()),
            "personal",
            150.0,
            true, // is_individual
            false,
        )
        .await?;

        // Test finding by name and user
        let found = get_envelope_by_name_and_user(&db, "Personal Envelope", "user123").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, user_envelope.id);

        // Test wrong user
        let wrong_user =
            get_envelope_by_name_and_user(&db, "Personal Envelope", "wrong_user").await?;
        assert!(wrong_user.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_update_envelope_balance_integration() -> Result<()> {
        let db = setup_test_db().await?;

        // Create an envelope
        let envelope = create_test_envelope(&db, "Test Envelope").await?;

        // Update the balance
        let updated_envelope = update_envelope_balance(&db, envelope.id, 75.0).await?;
        assert_eq!(updated_envelope.balance, 75.0);
        assert_eq!(updated_envelope.id, envelope.id);

        // Verify the update persisted
        let retrieved = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(retrieved.balance, 75.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_envelope_balance_not_found() -> Result<()> {
        let db = setup_test_db().await?;

        // Try to update non-existent envelope
        let result = update_envelope_balance(&db, 999, 75.0).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::EnvelopeNotFound { name: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_soft_delete_filtering() -> Result<()> {
        let db = setup_test_db().await?;

        // Create an envelope
        let envelope = create_test_envelope(&db, "Test Envelope").await?;

        // Manually mark as deleted (simulating soft delete)
        let mut envelope_model: envelope::ActiveModel = envelope.into();
        envelope_model.is_deleted = Set(true);
        envelope_model.update(&db).await?;

        // Test that deleted envelope is not found by name
        let not_found = get_envelope_by_name(&db, "Test Envelope").await?;
        assert!(not_found.is_none());

        // Create active envelope
        let active_envelope = create_test_envelope(&db, "Active Envelope").await?;

        // Test that only active envelope is returned in list
        let envelopes = get_all_active_envelopes(&db).await?;
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0], active_envelope);

        Ok(())
    }

    #[tokio::test]
    async fn test_is_individual_field_stored_correctly() -> Result<()> {
        let db = setup_test_db().await?;

        // Create shared envelope (is_individual = false)
        let shared_envelope = create_test_envelope(&db, "Shared Envelope").await?;
        assert!(!shared_envelope.is_individual);
        assert!(shared_envelope.user_id.is_none());

        // Create individual envelope (is_individual = true)
        let individual_envelope = create_custom_envelope(
            &db,
            "Individual Envelope",
            Some("user123".to_string()),
            "personal",
            150.0,
            true,
            false,
        )
        .await?;
        assert!(individual_envelope.is_individual);
        assert_eq!(individual_envelope.user_id, Some("user123".to_string()));

        // Verify persistence
        let retrieved_shared = Envelope::find_by_id(shared_envelope.id)
            .one(&db)
            .await?
            .unwrap();
        assert!(!retrieved_shared.is_individual);

        let retrieved_individual = Envelope::find_by_id(individual_envelope.id)
            .one(&db)
            .await?
            .unwrap();
        assert!(retrieved_individual.is_individual);

        Ok(())
    }

    #[tokio::test]
    async fn test_rollover_field_stored_correctly() -> Result<()> {
        let db = setup_test_db().await?;

        // Create envelope without rollover
        let no_rollover = create_test_envelope(&db, "No Rollover").await?;
        assert!(!no_rollover.rollover);

        // Create envelope with rollover
        let with_rollover = create_custom_envelope(
            &db,
            "With Rollover",
            None,
            "savings",
            200.0,
            false,
            true, // rollover enabled
        )
        .await?;
        assert!(with_rollover.rollover);

        // Verify persistence
        let retrieved_no_rollover = Envelope::find_by_id(no_rollover.id)
            .one(&db)
            .await?
            .unwrap();
        assert!(!retrieved_no_rollover.rollover);

        let retrieved_with_rollover = Envelope::find_by_id(with_rollover.id)
            .one(&db)
            .await?
            .unwrap();
        assert!(retrieved_with_rollover.rollover);

        Ok(())
    }

    #[tokio::test]
    async fn test_envelope_balance_field_stored_correctly() -> Result<()> {
        let db = setup_test_db().await?;

        let envelope = create_test_envelope(&db, "Test Envelope").await?;

        // Initial balance should be 0
        assert_eq!(envelope.balance, 0.0);

        // Update balance
        let updated = update_envelope_balance(&db, envelope.id, 123.45).await?;
        assert_eq!(updated.balance, 123.45);

        // Verify persistence
        let retrieved = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(retrieved.balance, 123.45);

        Ok(())
    }

    #[tokio::test]
    async fn test_envelope_allocation_field_stored_correctly() -> Result<()> {
        let db = setup_test_db().await?;

        let envelope = create_custom_envelope(
            &db,
            "Test Allocation",
            None,
            "necessary",
            250.75,
            false,
            false,
        )
        .await?;

        assert_eq!(envelope.allocation, 250.75);

        // Verify persistence
        let retrieved = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(retrieved.allocation, 250.75);

        Ok(())
    }
}
