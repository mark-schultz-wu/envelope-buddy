use crate::config::AppConfig;
use crate::errors::{Error, Result};
use crate::models::Envelope;
use rusqlite::Error as RusqliteError;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::{Arc, Mutex};
use tracing::{debug, info, instrument, warn};

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

#[instrument(skip(pool, arc_app_config))]
pub async fn seed_initial_envelopes(pool: &DbPool, arc_app_config: &Arc<AppConfig>) -> Result<()> {
    let envelope_configs = &arc_app_config.envelopes_from_toml;
    let user_id_1 = &arc_app_config.user_id_1;
    let user_id_2 = &arc_app_config.user_id_2;
    info!(
        "Starting to seed initial envelopes. Found {} configurations.",
        envelope_configs.len()
    );
    let mut conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for seeding".to_string()))?;

    // Start a transaction
    let tx = conn
        .transaction()
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for cfg_envelope in envelope_configs {
        debug!(
            "Processing config for envelope: '{}', individual: {}",
            cfg_envelope.name, cfg_envelope.is_individual
        );
        if cfg_envelope.is_individual {
            let user_ids_to_process = [user_id_1, user_id_2];
            for &current_user_id in user_ids_to_process.iter() {
                // Check if this specific user's individual envelope already exists
                let mut stmt_check = tx.prepare_cached(
                    "SELECT id FROM envelopes WHERE name = ?1 AND user_id = ?2 AND is_deleted = FALSE",
                )?;
                let exists: Option<i64> = stmt_check
                    .query_row(params![cfg_envelope.name, current_user_id], |row| {
                        row.get(0)
                    })
                    .optional()?; // Allows for no rows found without erroring

                if exists.is_none() {
                    info!(
                        "Inserting individual envelope '{}' for user {}",
                        cfg_envelope.name, current_user_id
                    );
                    let mut stmt_insert = tx.prepare_cached(
                        "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                         VALUES (?1, ?2, ?3, ?4, TRUE, ?5, ?6, FALSE)",
                    )?;
                    stmt_insert.execute(params![
                        cfg_envelope.name,
                        cfg_envelope.category,
                        cfg_envelope.allocation,
                        cfg_envelope.allocation, // Initial balance is the allocation amount
                        current_user_id,
                        cfg_envelope.rollover,
                    ])?;
                } else {
                    warn!(
                        "Individual envelope '{}' for user {} already exists. Skipping.",
                        cfg_envelope.name, current_user_id
                    );
                }
            }
        } else {
            // Shared envelope
            // Check if this shared envelope already exists
            let mut stmt_check = tx.prepare_cached(
                "SELECT id FROM envelopes WHERE name = ?1 AND user_id IS NULL AND is_deleted = FALSE",
            )?;
            let exists: Option<i64> = stmt_check
                .query_row(params![cfg_envelope.name], |row| row.get(0))
                .optional()?;

            if exists.is_none() {
                info!("Inserting shared envelope '{}'", cfg_envelope.name);
                let mut stmt_insert = tx.prepare_cached(
                    "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                     VALUES (?1, ?2, ?3, ?4, FALSE, NULL, ?5, FALSE)",
                )?;
                stmt_insert.execute(params![
                    cfg_envelope.name,
                    cfg_envelope.category,
                    cfg_envelope.allocation,
                    cfg_envelope.allocation, // Initial balance is the allocation amount
                    cfg_envelope.rollover,
                ])?;
            } else {
                warn!(
                    "Shared envelope '{}' already exists. Skipping.",
                    cfg_envelope.name
                );
            }
        }
    }

    tx.commit()
        .map_err(|e| Error::Database(format!("Failed to commit transaction: {}", e)))?;
    info!("Finished seeding initial envelopes.");
    Ok(())
}

#[instrument(skip(pool))]
pub async fn get_all_active_envelopes(pool: &DbPool) -> Result<Vec<Envelope>> {
    let conn = pool.lock().map_err(|_| {
        Error::Database("Failed to acquire DB lock for getting envelopes".to_string())
    })?;

    let mut stmt = conn.prepare_cached("SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted FROM envelopes WHERE is_deleted = FALSE ORDER BY name, user_id")?;

    let envelope_iter = stmt.query_map([], |row| {
        Ok(Envelope {
            id: row.get(0)?,
            name: row.get(1)?,
            category: row.get(2)?,
            allocation: row.get(3)?,
            balance: row.get(4)?,
            is_individual: row.get(5)?,
            user_id: row.get(6)?,
            rollover: row.get(7)?,
            is_deleted: row.get(8)?,
        })
    })?;

    let mut envelopes = Vec::new();
    for envelope_result in envelope_iter {
        envelopes.push(envelope_result.map_err(|e: RusqliteError| {
            Error::Database(format!("Failed to map envelope row: {}", e))
        })?);
    }

    debug!("Fetched {} active envelopes.", envelopes.len());
    Ok(envelopes)
}

// You'll add functions here to interact with the database, e.g.:
// pub async fn add_envelope(pool: &DbPool, envelope: &models::Envelope) -> Result<()> { ... }
// pub async fn get_envelope_balance(pool: &DbPool, envelope_name: &str, user_id: Option<&str>) -> Result<f64> { ... }
