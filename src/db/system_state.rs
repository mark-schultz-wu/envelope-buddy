use crate::db::DbPool;
use crate::errors::{Error, Result};
use rusqlite::{OptionalExtension, params};
use tracing::{debug, info, instrument};

/// Retrieves a value from the key-value `system_state` table.
///
/// This table is used for storing persistent system-wide settings or state,
/// such as the last processed month for updates.
///
/// # Parameters
///
/// * `pool`: The database connection pool.
/// * `key`: The key whose value is to be retrieved.
///
/// # Returns
///
/// Returns `Ok(Some(String))` if the key exists and a value is found.
/// Returns `Ok(None)` if the key does not exist in the table.
///
/// # Errors
///
/// Returns `Error::Database` if there's an issue acquiring the database lock,
/// preparing the SQL statement, or mapping the query result.
#[instrument(skip(pool))]
pub async fn get_system_state_value(pool: &DbPool, key: &str) -> Result<Option<String>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let mut stmt = conn.prepare_cached("SELECT value FROM system_state WHERE key = ?1")?;
    let value_result: Option<String> = stmt.query_row(params![key], |row| row.get(0)).optional()?;
    debug!("System state for key '{}': {:?}", key, value_result);
    Ok(value_result)
}

/// Sets or updates a value in the key-value `system_state` table.
///
/// If the key already exists, its value is updated. If the key does not exist,
/// a new key-value pair is inserted (UPSERT behavior).
///
/// # Parameters
///
/// * `pool`: The database connection pool.
/// * `key`: The key to set or update.
/// * `value`: The value to associate with the key.
///
/// # Errors
///
/// Returns `Error::Database` if there's an issue acquiring the database lock
/// or executing the insert/update statement.
#[instrument(skip(pool))]
pub async fn set_system_state_value(pool: &DbPool, key: &str, value: &str) -> Result<()> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    // Use INSERT OR REPLACE (UPSERT)
    conn.execute(
        "INSERT INTO system_state (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    info!("Set system state: {} = {}", key, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::{init_test_tracing, setup_test_db};
    use crate::errors::Result;

    #[tokio::test]
    async fn test_set_and_get_new_key() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_set_and_get_new_key for system_state");
        let db_pool = setup_test_db().await?;

        let test_key = "test_key_1";
        let test_value = "test_value_1";

        // Set a new key-value pair
        set_system_state_value(&db_pool, test_key, test_value).await?;

        // Get the value for the key
        let retrieved_value = get_system_state_value(&db_pool, test_key).await?;

        assert_eq!(
            retrieved_value,
            Some(test_value.to_string()),
            "Retrieved value should match the set value for a new key."
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_set_updates_existing_key() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_set_updates_existing_key for system_state");
        let db_pool = setup_test_db().await?;

        let test_key = "test_key_update";
        let initial_value = "initial_value";
        let updated_value = "updated_value";

        // Set an initial value
        set_system_state_value(&db_pool, test_key, initial_value).await?;
        let retrieved_initial = get_system_state_value(&db_pool, test_key).await?;
        assert_eq!(
            retrieved_initial,
            Some(initial_value.to_string()),
            "Initial value not set correctly."
        );

        // Update the value for the same key
        set_system_state_value(&db_pool, test_key, updated_value).await?;

        // Get the updated value
        let retrieved_updated = get_system_state_value(&db_pool, test_key).await?;

        assert_eq!(
            retrieved_updated,
            Some(updated_value.to_string()),
            "Retrieved value should be the updated value."
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_non_existent_key() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_get_non_existent_key for system_state");
        let db_pool = setup_test_db().await?;

        let non_existent_key = "this_key_does_not_exist";

        // Try to get a value for a key that hasn't been set
        let retrieved_value = get_system_state_value(&db_pool, non_existent_key).await?;

        assert!(
            retrieved_value.is_none(),
            "Retrieved value for a non-existent key should be None."
        );

        Ok(())
    }
}
