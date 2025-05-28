use crate::db::DbPool;
use crate::errors::{Error, Result};
use rusqlite::{OptionalExtension, params};
use tracing::{debug, info, instrument};

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
