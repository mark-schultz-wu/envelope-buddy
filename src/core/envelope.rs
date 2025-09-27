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
    use sea_orm::{DatabaseBackend, MockDatabase, MockExecResult};

    #[tokio::test]
    async fn test_create_envelope() -> Result<()> {
        let now = chrono::Utc::now().naive_utc();
        let expected_envelope = envelope::Model {
            id: 1,
            name: "Test Envelope".to_string(),
            user_id: None,
            category: "necessary".to_string(),
            monthly_allocation: 100.0,
            current_balance: 0.0,
            is_deleted: false,
            created_at: now,
            updated_at: now,
        };

        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_exec_results([MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            }])
            .append_query_results([vec![expected_envelope]])
            .into_connection();

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

        Ok(())
    }

    #[tokio::test]
    async fn test_get_envelope_by_name() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_query_results([vec![envelope::Model {
                id: 1,
                name: "Test Envelope".to_string(),
                user_id: None,
                category: "necessary".to_string(),
                monthly_allocation: 100.0,
                current_balance: 0.0,
                is_deleted: false,
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
            }]])
            .into_connection();

        let envelope = get_envelope_by_name(&db, "Test Envelope").await?;
        assert!(envelope.is_some());
        assert_eq!(envelope.unwrap().name, "Test Envelope");

        Ok(())
    }

    #[tokio::test]
    async fn test_create_envelope_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test empty name
        let result =
            create_envelope(&db, "".to_string(), None, "necessary".to_string(), 100.0).await;
        assert!(result.is_err());

        // Test negative allocation
        let result = create_envelope(
            &db,
            "Test".to_string(),
            None,
            "necessary".to_string(),
            -50.0,
        )
        .await;
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_create_envelope_with_mock_validation() -> Result<()> {
        let now = chrono::Utc::now().naive_utc();
        let expected_envelope = envelope::Model {
            id: 1,
            name: "Test Envelope".to_string(),
            user_id: None,
            category: "necessary".to_string(),
            monthly_allocation: 100.0,
            current_balance: 0.0,
            is_deleted: false,
            created_at: now,
            updated_at: now,
        };

        let db = MockDatabase::new(DatabaseBackend::Sqlite)
            .append_exec_results([MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            }])
            .append_query_results([vec![expected_envelope]])
            .into_connection();

        let _envelope = create_envelope(
            &db,
            "Test Envelope".to_string(),
            None,
            "necessary".to_string(),
            100.0,
        )
        .await?;

        // Validate that the operation was executed successfully
        Ok(())
    }
}
