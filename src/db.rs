use crate::config::AppConfig;
use crate::errors::{Error, Result};
use crate::models::Envelope;
use chrono::{NaiveDate, Utc};
use rusqlite::Error as RusqliteError;
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, instrument, warn};

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

#[instrument(skip(pool, config))]
pub async fn seed_initial_envelopes(pool: &DbPool, config: &Arc<AppConfig>) -> Result<()> {
    info!(
        "Starting to seed initial envelopes. Found {} configurations from TOML.",
        config.envelopes_from_toml.len()
    );
    let mut conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for seeding".to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for cfg_envelope in &config.envelopes_from_toml {
        debug!(
            "Processing config for envelope: '{}', individual: {}",
            cfg_envelope.name, cfg_envelope.is_individual
        );

        if cfg_envelope.is_individual {
            let user_ids_to_process = [&config.user_id_1, &config.user_id_2];
            for current_user_id_str in user_ids_to_process.iter() {
                let current_user_id: &str = current_user_id_str;

                // 1. Check for an ACTIVE envelope first
                let mut stmt_check_active = tx.prepare_cached(
                    "SELECT id FROM envelopes WHERE name = ?1 AND user_id = ?2 AND is_deleted = FALSE",
                )?;
                let active_exists: Option<i64> = stmt_check_active
                    .query_row(params![cfg_envelope.name, current_user_id], |row| {
                        row.get(0)
                    })
                    .optional()?;

                if active_exists.is_some() {
                    warn!(
                        "ACTIVE individual envelope '{}' for user {} already exists. Skipping.",
                        cfg_envelope.name, current_user_id
                    );
                    continue; // Move to the next user_id or config
                }

                // 2. No active one found, check for a SOFT-DELETED envelope
                let mut stmt_check_deleted = tx.prepare_cached(
                    "SELECT id FROM envelopes WHERE name = ?1 AND user_id = ?2 AND is_deleted = TRUE",
                )?;
                let deleted_envelope_id: Option<i64> = stmt_check_deleted
                    .query_row(params![cfg_envelope.name, current_user_id], |row| {
                        row.get(0)
                    })
                    .optional()?;

                if let Some(id_to_reenable) = deleted_envelope_id {
                    info!(
                        "Found soft-deleted individual envelope '{}' for user {}. Re-enabling and updating.",
                        cfg_envelope.name, current_user_id
                    );
                    let mut stmt_update = tx.prepare_cached(
                        "UPDATE envelopes SET category = ?1, allocation = ?2, balance = ?2, rollover = ?3, is_deleted = FALSE
                         WHERE id = ?4", // balance reset to new allocation
                    )?;
                    stmt_update.execute(params![
                        cfg_envelope.category,
                        cfg_envelope.allocation,
                        cfg_envelope.rollover,
                        id_to_reenable,
                    ])?;
                } else {
                    // 3. Neither active nor soft-deleted found, INSERT NEW
                    info!(
                        "Inserting NEW individual envelope '{}' for user {}",
                        cfg_envelope.name, current_user_id
                    );
                    let mut stmt_insert = tx.prepare_cached(
                        "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                         VALUES (?1, ?2, ?3, ?3, TRUE, ?4, ?5, FALSE)", // balance set to allocation
                    )?;
                    stmt_insert.execute(params![
                        cfg_envelope.name,
                        cfg_envelope.category,
                        cfg_envelope.allocation,
                        current_user_id,
                        cfg_envelope.rollover,
                    ])?;
                }
            }
        } else {
            // Shared envelope
            // 1. Check for an ACTIVE shared envelope
            let mut stmt_check_active = tx.prepare_cached(
                "SELECT id FROM envelopes WHERE name = ?1 AND user_id IS NULL AND is_deleted = FALSE",
            )?;
            let active_exists: Option<i64> = stmt_check_active
                .query_row(params![cfg_envelope.name], |row| row.get(0))
                .optional()?;

            if active_exists.is_some() {
                warn!(
                    "ACTIVE shared envelope '{}' already exists. Skipping.",
                    cfg_envelope.name
                );
                continue; // Move to the next config
            }

            // 2. No active one, check for SOFT-DELETED shared envelope
            let mut stmt_check_deleted = tx.prepare_cached(
                "SELECT id FROM envelopes WHERE name = ?1 AND user_id IS NULL AND is_deleted = TRUE",
            )?;
            let deleted_envelope_id: Option<i64> = stmt_check_deleted
                .query_row(params![cfg_envelope.name], |row| row.get(0))
                .optional()?;

            if let Some(id_to_reenable) = deleted_envelope_id {
                info!(
                    "Found soft-deleted shared envelope '{}'. Re-enabling and updating.",
                    cfg_envelope.name
                );
                let mut stmt_update = tx.prepare_cached(
                    "UPDATE envelopes SET category = ?1, allocation = ?2, balance = ?2, rollover = ?3, is_deleted = FALSE
                     WHERE id = ?4", // balance reset to new allocation
                )?;
                stmt_update.execute(params![
                    cfg_envelope.category,
                    cfg_envelope.allocation,
                    cfg_envelope.rollover,
                    id_to_reenable,
                ])?;
            } else {
                // 3. Neither active nor soft-deleted, INSERT NEW shared envelope
                info!("Inserting NEW shared envelope '{}'", cfg_envelope.name);
                let mut stmt_insert = tx.prepare_cached(
                    "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                     VALUES (?1, ?2, ?3, ?3, FALSE, NULL, ?4, FALSE)", // balance set to allocation
                )?;
                stmt_insert.execute(params![
                    cfg_envelope.name,
                    cfg_envelope.category,
                    cfg_envelope.allocation,
                    cfg_envelope.rollover,
                ])?;
            }
        }
    }

    tx.commit()
        .map_err(|e| Error::Database(format!("Failed to commit transaction for seeding: {}", e)))?;
    info!("Finished seeding initial envelopes with re-enable logic.");
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

#[instrument(skip(pool))]
pub async fn get_user_or_shared_envelope(
    pool: &DbPool,
    envelope_name: &str,
    user_id: &str, // The Discord User ID of the person trying to spend
) -> Result<Option<Envelope>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;

    // Try to find an individual envelope for this user OR a shared envelope
    // Prioritize individual envelope if names could clash (though our naming rules might prevent this)
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted
         FROM envelopes
         WHERE name = ?1 AND (user_id = ?2 OR user_id IS NULL) AND is_deleted = FALSE
         ORDER BY user_id DESC", // This prioritizes the user's specific envelope if a shared one has the same name
    )?;

    let envelope_result: Option<Envelope> = stmt
        .query_row(params![envelope_name, user_id], |row| {
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
        .optional()?; // Handles case where no envelope is found

    if envelope_result.is_some()
        && envelope_result.as_ref().unwrap().is_individual
        && envelope_result.as_ref().unwrap().user_id.as_deref() != Some(user_id)
    {
        // This case should ideally not happen if the query is correct and names are unique per user for individual types,
        // or if a shared envelope has the same name as an individual one (which is disallowed by your naming rules).
        // The query with "ORDER BY user_id DESC" (NULLs last) should pick the user's own first.
        // If it picked a shared one despite user_id matching, something is off or names aren't unique.
        // For now, we trust the query to fetch the correct one if it exists.
    }

    debug!(
        "Envelope lookup for '{}' for user '{}': {:?}",
        envelope_name,
        user_id,
        envelope_result.as_ref().map(|e| &e.name)
    );
    Ok(envelope_result)
}

#[instrument(skip(pool))]
pub async fn update_envelope_balance(
    pool: &DbPool,
    envelope_id: i64,
    new_balance: f64,
) -> Result<()> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    conn.execute(
        "UPDATE envelopes SET balance = ?1 WHERE id = ?2",
        params![new_balance, envelope_id],
    )?;
    info!(
        "Updated balance for envelope_id {}: new_balance = {}",
        envelope_id, new_balance
    );
    Ok(())
}

#[instrument(skip(pool, description))]
pub async fn create_transaction(
    pool: &DbPool,
    envelope_id: i64,
    amount: f64,
    description: &str,
    spender_user_id: &str,
    discord_message_id: Option<&str>,
    transaction_type: &str, // ADDED parameter
) -> Result<i64> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let current_timestamp = Utc::now();

    let mut stmt = conn.prepare_cached(
        "INSERT INTO transactions (envelope_id, amount, description, timestamp, user_id, message_id, transaction_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", // Updated SQL
    )?;
    let transaction_id = stmt.insert(params![
        envelope_id,
        amount,
        description,
        current_timestamp,
        spender_user_id,
        discord_message_id,
        transaction_type, // ADDED value
    ])?;
    info!(
        "Created transaction_id {} for envelope_id {}: type='{}', amount={}, user_id={}",
        transaction_id, envelope_id, transaction_type, amount, spender_user_id
    );
    Ok(transaction_id)
}

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

#[instrument(skip(pool))]
pub async fn prune_old_transactions(pool: &DbPool, cutoff_date: NaiveDate) -> Result<usize> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    // chrono NaiveDate needs to be converted to a format SQLite understands for comparison,
    // or compare against unix epoch timestamps if you store timestamps that way.
    // Assuming your timestamp column stores ISO8601 strings like "YYYY-MM-DD HH:MM:SS.sssZ"
    // or "YYYY-MM-DDTHH:MM:SS.sssZ" which chrono::DateTime<Utc> produces.
    // SQLite's date functions can work with these.
    let cutoff_date_str = cutoff_date.format("%Y-%m-%d").to_string();

    let rows_deleted = conn.execute(
        // Delete transactions where the date part of the timestamp is less than the cutoff date.
        "DELETE FROM transactions WHERE strftime('%Y-%m-%d', timestamp) < ?1",
        params![cutoff_date_str],
    )?;
    info!(
        "Pruned {} transactions older than {}",
        rows_deleted, cutoff_date_str
    );
    Ok(rows_deleted)
}

#[instrument(skip(pool))]
pub async fn get_actual_spending_this_month(
    pool: &DbPool,
    envelope_id: i64,
    year: i32,
    month: u32,
) -> Result<f64> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let month_str = format!("{:04}-{:02}", year, month); // Format as "YYYY-MM"

    let mut stmt = conn.prepare_cached(
        "SELECT COALESCE(SUM(amount), 0.0) FROM transactions
         WHERE envelope_id = ?1 AND transaction_type = 'spend' AND strftime('%Y-%m', timestamp) = ?2",
    )?;
    let total_spent: f64 = stmt.query_row(params![envelope_id, month_str], |row| row.get(0))?;

    debug!(
        "Actual spending for envelope_id {} in month {}: ${:.2}",
        envelope_id, month_str, total_spent
    );
    Ok(total_spent)
}

#[instrument(skip(pool))]
pub async fn soft_delete_envelope(
    pool: &DbPool,
    envelope_name: &str,
    user_id: &str, // The Discord User ID of the person trying to delete
) -> Result<bool> {
    // Returns true if an envelope was deleted, false otherwise
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for delete".to_string()))?;

    // Find the envelope: either a shared one by this name, or the user's individual one.
    // Prioritize the user's individual envelope if names could clash.
    let mut stmt_find = conn.prepare_cached(
        "SELECT id, user_id as owner_id, is_individual FROM envelopes
         WHERE name = ?1 AND (user_id = ?2 OR user_id IS NULL) AND is_deleted = FALSE
         ORDER BY user_id DESC LIMIT 1", // User's own DESC (not NULL) comes before NULL
    )?;

    struct FoundEnvelope {
        id: i64,
        owner_id: Option<String>,
        is_individual: bool,
    }

    let envelope_to_delete_result: Option<FoundEnvelope> = stmt_find
        .query_row(params![envelope_name, user_id], |row| {
            Ok(FoundEnvelope {
                id: row.get(0)?,
                owner_id: row.get(1)?,
                is_individual: row.get(2)?,
            })
        })
        .optional()?;

    if let Some(envelope_data) = envelope_to_delete_result {
        // If it's an individual envelope, ensure the deleter is the owner.
        // If it's shared, anyone in the couple (who can issue commands) can delete it.
        if envelope_data.is_individual && envelope_data.owner_id.as_deref() != Some(user_id) {
            warn!(
                "User {} attempted to delete individual envelope '{}' belonging to someone else ({}). Denied.",
                user_id,
                envelope_name,
                envelope_data.owner_id.as_deref().unwrap_or("unknown")
            );
            return Ok(false); // Or return an error indicating permission denied
        }

        let rows_affected = conn.execute(
            "UPDATE envelopes SET is_deleted = TRUE WHERE id = ?1 AND is_deleted = FALSE",
            params![envelope_data.id],
        )?;

        if rows_affected > 0 {
            info!(
                "Soft-deleted envelope '{}' (ID: {}) by user {}",
                envelope_name, envelope_data.id, user_id
            );
            return Ok(true);
        } else {
            // This case (found but not deleted) would be rare if the select query included is_deleted = FALSE
            warn!(
                "Envelope '{}' (ID: {}) was found but not soft-deleted (already deleted or race condition?).",
                envelope_name, envelope_data.id
            );
            return Ok(false);
        }
    } else {
        info!(
            "No active envelope named '{}' found for user {} to delete.",
            envelope_name, user_id
        );
        return Ok(false); // No active envelope found to delete
    }
}

// Argument struct for creating/updating an envelope instance with optional fields
pub struct EnvelopeInstanceOptionalArgs<'a> {
    pub name: &'a str,
    pub category: Option<&'a str>,
    pub allocation: Option<f64>,
    pub is_individual: Option<bool>,
    pub user_id: Option<&'a str>, // For specific instance (User1, User2, or None for shared)
    pub rollover: Option<bool>,
}

#[instrument(skip(tx, args))]
fn manage_envelope_instance_in_transaction(
    tx: &rusqlite::Transaction,
    args: &EnvelopeInstanceOptionalArgs<'_>, // user_id in args determines if we are acting on a shared or specific user's individual envelope
) -> Result<String> {
    // Determine the target is_individual status based on args.user_id (more reliable for lookup)
    let target_is_individual_type = args.user_id.is_some();

    // 1. Check for an ACTIVE envelope first
    let mut stmt_check_active = tx.prepare_cached(
        "SELECT id FROM envelopes WHERE name = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND is_individual = ?3 AND is_deleted = FALSE",
    )?;
    if stmt_check_active.exists(params![args.name, args.user_id, target_is_individual_type])? {
        let msg = format!(
            "ACTIVE envelope '{}' for user {:?} (individual: {}) already exists.",
            args.name,
            args.user_id.unwrap_or("Shared"),
            target_is_individual_type
        );
        warn!("{}", msg);
        return Ok(format!("{} (Skipped).", msg));
    }

    // 2. No active one, check for a SOFT-DELETED envelope
    let mut stmt_get_deleted = tx.prepare_cached(
        "SELECT id, category, allocation, is_individual, rollover FROM envelopes
         WHERE name = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND is_individual = ?3 AND is_deleted = TRUE",
    )?;
    // Fetch existing properties of the soft-deleted envelope
    let deleted_envelope_data: Option<(i64, String, f64, bool, bool)> = stmt_get_deleted
        .query_row(
            params![args.name, args.user_id, target_is_individual_type],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;

    if let Some((
        id_to_reenable,
        old_category,
        old_allocation,
        existing_is_individual_db,
        old_rollover,
    )) = deleted_envelope_data
    {
        // CRITICAL: The is_individual status is taken from the DB record.
        // The args.is_individual (if Some) should match target_is_individual_type, which comes from args.user_id presence.
        // This ensures we found the correct type of envelope (shared vs individual for a specific user).
        if args.is_individual.is_some() && args.is_individual.unwrap() != existing_is_individual_db
        {
            // This case should be rare if the SELECT query correctly used target_is_individual_type.
            // It indicates an attempt to change the fundamental shared/individual nature on re-enable, which we are disallowing.
            let msg = format!(
                "Error: Found soft-deleted envelope '{}' for user {:?} with is_individual={}, but tried to re-enable with is_individual={}. This change is not allowed.",
                args.name,
                args.user_id.unwrap_or("Shared"),
                existing_is_individual_db,
                args.is_individual.unwrap()
            );
            error!("{}", msg);
            return Err(Error::Command(msg));
        }

        let category = args.category.unwrap_or(&old_category);
        let allocation = args.allocation.unwrap_or(old_allocation);
        let rollover = args.rollover.unwrap_or(old_rollover);
        let new_balance = allocation; // Reset balance to new/effective allocation

        info!(
            "Re-enabling soft-deleted envelope '{}' for {:?} (individual: {}) with updated attributes.",
            args.name,
            args.user_id.unwrap_or("Shared"),
            existing_is_individual_db
        );
        let mut stmt_update = tx.prepare_cached(
            "UPDATE envelopes SET category = ?1, allocation = ?2, balance = ?3, rollover = ?4, is_deleted = FALSE
             WHERE id = ?5", // is_individual is NOT updated from args here, it's fixed from DB.
        )?;
        stmt_update.execute(params![
            category,
            allocation,
            new_balance,
            rollover,
            id_to_reenable,
        ])?;
        return Ok(format!(
            "Re-enabled and updated envelope '{}' for {:?} (individual: {}).",
            args.name,
            args.user_id.unwrap_or("Shared"),
            existing_is_individual_db
        ));
    } else {
        // 3. Neither active nor soft-deleted found, INSERT NEW
        // For a new envelope, is_individual status comes from args (defaulting if None)
        let is_individual_for_new = args.is_individual.unwrap_or(target_is_individual_type);
        if is_individual_for_new != target_is_individual_type {
            return Err(Error::Command(format!(
                "Inconsistency for new envelope '{}': command's is_individual flag ({:?}) mismatches expectation based on user_id presence ({}).",
                args.name, args.is_individual, target_is_individual_type
            )));
        }

        let category = args.category.unwrap_or("uncategorized");
        let allocation = args.allocation.unwrap_or(0.0);
        let rollover = args.rollover.unwrap_or(false);
        let new_balance = allocation;

        info!(
            "Inserting NEW envelope '{}' for {:?} (individual: {}).",
            args.name,
            args.user_id.unwrap_or("Shared"),
            is_individual_for_new
        );
        let mut stmt_insert = tx.prepare_cached(
            "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, FALSE)",
        )?;
        stmt_insert.execute(params![
            args.name,
            category,
            allocation,
            new_balance,
            is_individual_for_new,
            args.user_id, // This will be None for shared, Some(id) for individual
            rollover,
        ])?;
        return Ok(format!(
            "Created new envelope '{}' for {:?} (individual: {}).",
            args.name,
            args.user_id.unwrap_or("Shared"),
            is_individual_for_new
        ));
    }
}

// New struct to group arguments for creating/flexible-updating an envelope
#[derive(Debug)] // Add Debug for easier logging if needed
pub struct CreateUpdateEnvelopeArgs<'a> {
    pub name: &'a str,
    pub category_opt: Option<&'a str>,
    pub allocation_opt: Option<f64>,
    pub is_individual_cmd_opt: Option<bool>, // The user's intent for individual status
    pub rollover_opt: Option<bool>,
}

#[instrument(skip(pool, envelope_data, config_user_id_1, config_user_id_2))]
pub async fn create_or_reenable_envelope_flexible(
    pool: &DbPool,
    envelope_data: &CreateUpdateEnvelopeArgs<'_>,
    config_user_id_1: &str,
    config_user_id_2: &str,
) -> Result<Vec<String>> {
    let mut conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;
    let mut results = Vec::new();

    let intended_is_individual_type = envelope_data.is_individual_cmd_opt.unwrap_or(false);

    let category = envelope_data.category_opt;
    let allocation = envelope_data.allocation_opt;
    let rollover = envelope_data.rollover_opt;
    let name = envelope_data.name;

    if intended_is_individual_type {
        // Base arguments for individual type. is_individual in the struct itself will be Some(true).
        // user_id will be filled per user.
        let base_args_for_individual = EnvelopeInstanceOptionalArgs {
            name,
            category,
            allocation,
            is_individual: Some(true), // This instance IS individual
            user_id: None,             // Placeholder, set below
            rollover,
        };
        results.push(manage_envelope_instance_in_transaction(
            &tx,
            &EnvelopeInstanceOptionalArgs {
                user_id: Some(config_user_id_1),
                ..base_args_for_individual
            },
        )?);
        results.push(manage_envelope_instance_in_transaction(
            &tx,
            &EnvelopeInstanceOptionalArgs {
                user_id: Some(config_user_id_2),
                ..base_args_for_individual
            },
        )?);
    } else {
        // Shared envelope
        let shared_args = EnvelopeInstanceOptionalArgs {
            name,
            category,
            allocation,
            is_individual: Some(false), // This instance IS shared
            user_id: None,              // No user_id for shared
            rollover,
        };
        results.push(manage_envelope_instance_in_transaction(&tx, &shared_args)?);
    }

    tx.commit()
        .map_err(|e| Error::Database(format!("Failed to commit transaction: {}", e)))?;
    Ok(results)
}
