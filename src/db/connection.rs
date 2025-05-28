use crate::db::schema::create_tables;
use crate::errors::{Error, Result};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, instrument};

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
