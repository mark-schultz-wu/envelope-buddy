//! Envelope business logic - Handles all envelope-related operations.
//! Provides functions for creating, retrieving, updating, and managing envelopes.
//! All functions are async and return Result types for error handling.

use crate::{
    entities::*,
    errors::{Error, Result},
};
use sea_orm::*;

/// Retrieves all active (non-deleted) envelopes from the database, ordered alphabetically by name.
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

/// Creates a new envelope with the specified parameters, performing input validation.
/// This function validates that the name is not empty, the allocation is non-negative,
/// and trims whitespace from the name. It initializes the envelope with zero balance
/// and sets up proper timestamps for tracking creation and updates.
pub async fn create_envelope(
    db: &DatabaseConnection,
    name: String,
    user_id: Option<String>,
    category: String,
    monthly_allocation: f64,
) -> Result<envelope::Model> {
    // Validate inputs
    if name.trim().is_empty() {
        return Err(Error::Config {
            message: "Envelope name cannot be empty".to_string(),
        });
    }

    if monthly_allocation < 0.0 {
        return Err(Error::InvalidAmount {
            amount: monthly_allocation,
        });
    }

    let now = chrono::Utc::now().naive_utc();

    let envelope = envelope::ActiveModel {
        name: Set(name.trim().to_string()),
        user_id: Set(user_id),
        category: Set(category),
        monthly_allocation: Set(monthly_allocation),
        current_balance: Set(0.0),
        is_deleted: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = envelope.insert(db).await?;
    Ok(result)
}

/// Updates the current balance of an existing envelope and refreshes the updated timestamp.
/// This function is called after transactions to maintain accurate balance tracking.
/// It first verifies the envelope exists before attempting the update to prevent
/// orphaned balance changes.
pub async fn update_envelope_balance(
    db: &DatabaseConnection,
    envelope_id: i32,
    new_balance: f64,
) -> Result<envelope::Model> {
    let mut envelope: envelope::ActiveModel = Envelope::find_by_id(envelope_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })?
        .into();

    envelope.current_balance = Set(new_balance);
    envelope.updated_at = Set(chrono::Utc::now().naive_utc());

    envelope.update(db).await.map_err(Into::into)
}
#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{DatabaseBackend, MockDatabase};

    #[tokio::test]
    async fn test_create_envelope_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test empty name validation
        let result =
            create_envelope(&db, "".to_string(), None, "necessary".to_string(), 100.0).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test whitespace-only name validation
        let result =
            create_envelope(&db, "   ".to_string(), None, "necessary".to_string(), 100.0).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test negative allocation validation
        let result = create_envelope(
            &db,
            "Test".to_string(),
            None,
            "necessary".to_string(),
            -50.0,
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
        // Use real database to test actual envelope creation
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        let envelope = create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        assert_eq!(envelope.name, "Test Envelope");
        assert_eq!(envelope.monthly_allocation, 100.0);
        assert_eq!(envelope.current_balance, 0.0);
        assert!(!envelope.is_deleted);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_envelope_by_name_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope
        let created_envelope = create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

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
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create multiple envelopes
        let envelope0 = create_envelope(
            &db,
            "Envelope 0".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        let envelope1 = create_envelope(
            &db,
            "Envelope 1".to_string(),
            Some("user123".to_string()),
            "wants".to_string(),
            200.0,
        )
        .await?;

        // Test getting all active envelopes
        let envelopes = get_all_active_envelopes(&db).await?;
        assert_eq!(envelopes.len(), 2);

        // Test that they're ordered alphabetically
        assert_eq!(envelopes[0], envelope0);
        assert_eq!(envelopes[1], envelope1);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_envelope_by_name_and_user_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create user-specific envelope
        let user_envelope = create_envelope(
            &db,
            "Personal Envelope".to_string(),
            Some("user123".to_string()),
            "personal".to_string(),
            150.0,
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
        // Use real database to test actual update logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope
        let envelope = create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Update the balance
        let updated_envelope = update_envelope_balance(&db, envelope.id, 75.0).await?;
        assert_eq!(updated_envelope.current_balance, 75.0);
        assert_eq!(updated_envelope.id, envelope.id);

        // Verify the update persisted
        let retrieved = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(retrieved.current_balance, 75.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_envelope_balance_not_found() -> Result<()> {
        // Use real database to test error handling
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

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
    async fn test_get_envelope_by_name_deleted_envelope() -> Result<()> {
        // Use real database to test that deleted envelopes are not returned
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create an envelope
        let envelope = create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Manually mark as deleted (simulating soft delete)
        let mut envelope_model: envelope::ActiveModel = envelope.into();
        envelope_model.is_deleted = Set(true);
        envelope_model.update(&db).await?;

        // Test that deleted envelope is not found
        let not_found = get_envelope_by_name(&db, "Test Envelope").await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_active_envelopes_excludes_deleted() -> Result<()> {
        // Use real database to test that deleted envelopes are excluded
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create active envelope
        let active_envelope = create_envelope(
            &db,
            "Active Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Create and delete another envelope
        let deleted_envelope = create_envelope(
            &db,
            "Deleted Envelope".to_string(),
            None,
            "necessary".to_string(),
            200.0,
        )
        .await?;
        let mut deleted_model: envelope::ActiveModel = deleted_envelope.into();
        deleted_model.is_deleted = Set(true);
        deleted_model.update(&db).await?;

        // Test that only active envelope is returned
        let envelopes = get_all_active_envelopes(&db).await?;
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0], active_envelope);

        Ok(())
    }
}
