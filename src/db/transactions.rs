use crate::db::DbPool;
use crate::errors::{Error, Result};
use chrono::{NaiveDate, Utc};
use rusqlite::params;
use tracing::{debug, info, instrument};

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
