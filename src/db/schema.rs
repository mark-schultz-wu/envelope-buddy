use crate::errors::{Error, Result};
use rusqlite::Connection;
use tracing::{debug, info, instrument, warn};

#[instrument(skip(conn))]
pub(crate) fn create_tables(conn: &Connection) -> Result<()> {
    debug!("Executing CREATE TABLE statements if tables do not exist.");
    conn.execute_batch(
        "BEGIN;

        CREATE TABLE IF NOT EXISTS envelopes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            category TEXT NOT NULL,
            allocation REAL NOT NULL,
            balance REAL NOT NULL,
            is_individual BOOLEAN NOT NULL DEFAULT FALSE,
            user_id TEXT,
            rollover BOOLEAN NOT NULL DEFAULT FALSE,
            is_deleted BOOLEAN NOT NULL DEFAULT FALSE -- For soft deletes
        );

        -- Index for unique shared envelope names (name is unique when user_id IS NULL)
        -- This uniqueness applies whether the envelope is soft-deleted or not.
        CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_shared_envelope_name
            ON envelopes(name)
            WHERE user_id IS NULL;

        -- Index for unique individual envelope names per user (name + user_id is unique when user_id IS NOT NULL)
        -- This uniqueness applies whether the envelope is soft-deleted or not.
        CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_individual_envelope_name_user
            ON envelopes(name, user_id)
            WHERE user_id IS NOT NULL;

        CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            envelope_id INTEGER NOT NULL,
            amount REAL NOT NULL,
            description TEXT NOT NULL,
            timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            user_id TEXT NOT NULL,
            message_id TEXT,
            transaction_type TEXT NOT NULL,
            FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
        );

        -- ... (products, shortcuts, system_state tables as before) ...
        CREATE TABLE IF NOT EXISTS products ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, price REAL NOT NULL, envelope_id INTEGER NOT NULL, description TEXT, FOREIGN KEY (envelope_id) REFERENCES envelopes (id) );
        CREATE TABLE IF NOT EXISTS shortcuts ( id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, command_template TEXT NOT NULL );
        CREATE TABLE IF NOT EXISTS system_state ( key TEXT PRIMARY KEY, value TEXT );
        COMMIT;"
    )
    .map_err(|e| Error::Database(format!("Failed to create tables with updated unique constraints: {}", e)))?;
    info!(
        "Database tables ensured (envelope UNIQUE constraints updated for soft delete handling)."
    );
    Ok(())
}
