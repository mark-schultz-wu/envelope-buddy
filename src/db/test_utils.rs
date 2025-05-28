#![allow(dead_code)]
use crate::db::{DbPool, schema};
use crate::errors::{Error, Result};
use crate::models::Envelope;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use rusqlite::{OptionalExtension, params};
use std::sync::Arc;
use std::sync::Mutex;
use tracing_subscriber::EnvFilter;

pub(crate) fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("trace")), // Default to TRACE for tests if RUST_LOG is not set
        )
        .with_test_writer() // Crucial for `cargo test` output
        .try_init(); // Use try_init to avoid panic if already initialized
}

// Helper to create an in-memory DbPool for testing
// This helper should set up the schema as well.
pub(crate) async fn setup_test_db() -> Result<DbPool> {
    // Using :memory: for a fresh, temporary database for each test run (or test module)
    // init_db already creates tables.
    // If init_db requires a path, we can use a unique name or handle in-memory setup differently
    // For simplicity, if init_db can handle ":memory:":
    // let pool = init_db(":memory:").await?;

    // Or, more directly for tests if init_db is complex:
    let conn = Connection::open_in_memory()
        .map_err(|e| Error::Database(format!("Test DB: Failed to open in-memory: {}", e)))?;
    schema::create_tables(&conn)?; // Make sure create_tables is accessible
    Ok(Arc::new(Mutex::new(conn)))
}

pub(crate) struct DirectInsertArgs<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) name: &'a str,
    pub(crate) category: &'a str,
    pub(crate) allocation: f64,
    pub(crate) balance: f64,
    pub(crate) is_individual: bool,
    pub(crate) user_id: Option<&'a str>,
    pub(crate) rollover: bool,
    pub(crate) is_deleted: bool,
}

// Helper to quickly insert a test envelope for setup (not using seed_initial_envelopes for focused tests)
// This is a simplified insert, real seeding has more logic.
pub(crate) fn direct_insert_envelope(args: &DirectInsertArgs) -> Result<i64> {
    let mut stmt = args.conn.prepare_cached(
            "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
    let id = stmt.insert(params![
        args.name,
        args.category,
        args.allocation,
        args.balance,
        args.is_individual,
        args.user_id,
        args.rollover,
        args.is_deleted
    ])?;
    Ok(id)
}

// Helper to fetch any envelope by ID, including deleted ones, for test verification
pub(crate) fn get_envelope_by_id_for_test(conn: &Connection, id: i64) -> Result<Option<Envelope>> {
    let mut stmt = conn.prepare_cached(
             "SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted
              FROM envelopes WHERE id = ?1",
        )?;
    stmt.query_row(params![id], |row| {
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
    })
    .optional()
    .map_err(Error::from)
}

#[derive(Debug)]
pub(crate) struct TestTransaction {
    #[allow(dead_code)]
    pub(crate) id: i64,
    pub(crate) amount: f64,
    pub(crate) description: String,
    pub(crate) transaction_type: String,
    pub(crate) user_id: String,
    pub(crate) timestamp: DateTime<Utc>,
}

pub(crate) fn get_transaction_by_id_for_test(
    conn: &Connection,
    tx_id: i64,
) -> Result<Option<TestTransaction>> {
    let mut stmt = conn.prepare_cached(
            "SELECT id, amount, description, transaction_type, user_id, timestamp FROM transactions WHERE id = ?1"
        )?;
    stmt.query_row(params![tx_id], |row| {
        Ok(TestTransaction {
            id: row.get(0)?,
            amount: row.get(1)?,
            description: row.get(2)?,
            transaction_type: row.get(3)?,
            user_id: row.get(4)?,
            timestamp: row.get(5)?,
        })
    })
    .optional()
    .map_err(Error::from)
}
