use crate::errors::{Error, Result};
use rusqlite::Connection; // Using a direct connection for simplicity, consider r2d2 for pooling with Serenity if needed
use std::sync::{Arc, Mutex}; // For sharing connection across async tasks safely if not using a pool
use tracing::{debug, info, instrument};

// A simple wrapper for now. For a production bot with Serenity,
// you might want to use a connection pool like `r2d2_sqlite` or `sqlx`.
// For simplicity with rusqlite directly, ensure thread safety if shared.
// Arc<Mutex<Connection>> is a common pattern for sharing a single rusqlite connection.
pub type DbPool = Arc<Mutex<Connection>>;

#[instrument]
pub async fn init_db(db_path: &str) -> Result<DbPool> {
    debug!("Initializing database connection to: {}", db_path);
    let conn = Connection::open(db_path)
        .map_err(|e| Error::Database(format!("Failed to open database at {}: {}", db_path, e)))?;

    // Enable foreign keys if not enabled by default (good practice)
    conn.execute("PRAGMA foreign_keys = ON;", [])
        .map_err(|e| Error::Database(format!("Failed to enable foreign keys: {}", e)))?;

    info!("Database connection opened. Ensuring tables are created...");
    create_tables(&conn)?;

    Ok(Arc::new(Mutex::new(conn)))
}

#[instrument(skip(conn))]
fn create_tables(conn: &Connection) -> Result<()> {
    debug!("Executing CREATE TABLE statements if tables do not exist.");
    // Using "IF NOT EXISTS" to be idempotent
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS envelopes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            category TEXT NOT NULL, -- 'necessary', 'quality_of_life'
            allocation REAL NOT NULL,
            balance REAL NOT NULL,
            is_individual BOOLEAN NOT NULL DEFAULT FALSE,
            user_id TEXT, -- Discord User ID. NULL if shared.
            rollover BOOLEAN NOT NULL DEFAULT FALSE,
            is_deleted BOOLEAN NOT NULL DEFAULT FALSE -- For soft deletes
            -- Consider UNIQUE constraint on (name, user_id) if user_id is not NULL
            -- Or UNIQUE on (name) if user_id IS NULL for shared envelopes
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            envelope_id INTEGER NOT NULL,
            amount REAL NOT NULL,
            description TEXT NOT NULL,
            timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
            user_id TEXT NOT NULL, -- Discord User ID of the person who initiated the transaction
            message_id TEXT,       -- Discord message ID for reference/editing
            FOREIGN KEY (envelope_id) REFERENCES envelopes (id)
        );

        CREATE TABLE IF NOT EXISTS products (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            price REAL NOT NULL,
            envelope_id INTEGER NOT NULL,
            description TEXT,
            FOREIGN KEY (envelope_id) REFERENCES envelopes (id)
        );

        CREATE TABLE IF NOT EXISTS shortcuts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            command_template TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS system_state (
            key TEXT PRIMARY KEY,
            value TEXT
        );
        COMMIT;",
    )
    .map_err(|e| Error::Database(format!("Failed to create tables: {}", e)))?;
    info!("Database tables ensured.");
    Ok(())
}

// You'll add functions here to interact with the database, e.g.:
// pub async fn add_envelope(pool: &DbPool, envelope: &models::Envelope) -> Result<()> { ... }
// pub async fn get_envelope_balance(pool: &DbPool, envelope_name: &str, user_id: Option<&str>) -> Result<f64> { ... }
