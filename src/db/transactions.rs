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

#[cfg(test)]
mod tests {
    use super::*; // Import items from parent module (transactions.rs)
    use crate::db::test_utils::{
        DirectInsertArgs, direct_insert_envelope, get_transaction_by_id_for_test,
        init_test_tracing, setup_test_db,
    };
    use crate::errors::Result;
    use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc}; // Added TimeZone

    #[tokio::test]
    async fn test_create_spend_transaction() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_create_spend_transaction");
        let db_pool = setup_test_db().await?;
        let envelope_id;
        {
            let conn = db_pool.lock().unwrap();
            let insert_args = DirectInsertArgs {
                conn: &conn,
                name: "GrocerySpend",
                category: "food",
                allocation: 200.0,
                balance: 150.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            };
            envelope_id = direct_insert_envelope(&insert_args)?;
        }

        let amount = 25.50;
        let description = "Weekly groceries";
        let spender_user_id = "user123_spend";
        let message_id = Some("msg_spend_123");
        let transaction_type = "spend";

        let before_creation = Utc::now();
        let tx_id = create_transaction(
            &db_pool,
            envelope_id,
            amount,
            description,
            spender_user_id,
            message_id,
            transaction_type,
        )
        .await?;
        let after_creation = Utc::now();

        assert!(tx_id > 0, "Transaction ID should be positive");

        {
            let conn = db_pool.lock().unwrap();
            let created_tx = get_transaction_by_id_for_test(&conn, tx_id)?
                .expect("Transaction not found after creation");

            assert_eq!(created_tx.amount, amount);
            assert_eq!(created_tx.description, description);
            assert_eq!(created_tx.transaction_type, transaction_type);
            assert_eq!(created_tx.user_id, spender_user_id);
            assert!(
                created_tx.timestamp >= before_creation && created_tx.timestamp <= after_creation
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_create_deposit_transaction() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_create_deposit_transaction");
        let db_pool = setup_test_db().await?;
        let envelope_id;
        {
            let conn = db_pool.lock().unwrap();
            let insert_args = DirectInsertArgs {
                conn: &conn,
                name: "SavingsDeposit",
                category: "savings",
                allocation: 1000.0,
                balance: 500.0,
                is_individual: true,
                user_id: Some("user456_deposit"),
                rollover: true,
                is_deleted: false,
            };
            envelope_id = direct_insert_envelope(&insert_args)?;
        }

        let amount = 100.00;
        let description = "Monthly savings deposit";
        let depositor_user_id = "user456_deposit";
        let transaction_type = "deposit";

        let tx_id = create_transaction(
            &db_pool,
            envelope_id,
            amount,
            description,
            depositor_user_id,
            None,
            transaction_type,
        )
        .await?;

        assert!(tx_id > 0);
        {
            let conn = db_pool.lock().unwrap();
            let created_tx =
                get_transaction_by_id_for_test(&conn, tx_id)?.expect("Transaction not found");
            assert_eq!(created_tx.amount, amount);
            assert_eq!(created_tx.description, description);
            assert_eq!(created_tx.transaction_type, transaction_type);
        }
        Ok(())
    }

    // --- get_actual_spending_this_month Tests ---

    #[tokio::test]
    async fn test_get_actual_spending_this_month_logic() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_get_actual_spending_this_month_logic");
        let db_pool = setup_test_db().await?;
        let env1_id;
        let env2_id;
        let user_id = "spending_user";

        let current_year = 2025;
        let current_month = 5;

        let this_month_ts1_utc: DateTime<Utc> = Utc
            .with_ymd_and_hms(current_year, current_month, 2, 10, 0, 0)
            .unwrap();
        let this_month_ts2_utc: DateTime<Utc> = Utc
            .with_ymd_and_hms(current_year, current_month, 15, 12, 0, 0)
            .unwrap();

        let first_of_this_month_naive =
            NaiveDate::from_ymd_opt(current_year, current_month, 1).unwrap();
        let last_day_of_last_month_naive = first_of_this_month_naive.pred_opt().unwrap(); // April 30, 2025
        let last_month_ts_utc: DateTime<Utc> = Utc
            .with_ymd_and_hms(
                last_day_of_last_month_naive.year(),
                last_day_of_last_month_naive.month(),
                15,
                10,
                0,
                0,
            )
            .unwrap(); // April 15, 2025

        {
            // Setup scope
            let conn = db_pool.lock().unwrap();
            env1_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Env1Spending",
                category: "cat1",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
            env2_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Env2Spending",
                category: "cat2",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;

            let mut insert_stmt = conn.prepare(
                "INSERT INTO transactions (envelope_id, amount, description, user_id, transaction_type, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            )?;

            insert_stmt.execute(params![
                env1_id,
                10.50,
                "spend1_this_month",
                user_id,
                "spend",
                this_month_ts1_utc
            ])?;
            insert_stmt.execute(params![
                env1_id,
                15.25,
                "spend2_this_month",
                user_id,
                "spend",
                this_month_ts2_utc
            ])?;
            insert_stmt.execute(params![
                env1_id,
                50.0,
                "deposit_this_month",
                user_id,
                "deposit",
                this_month_ts1_utc
            ])?;
            insert_stmt.execute(params![
                env1_id,
                20.0,
                "spend_last_month",
                user_id,
                "spend",
                last_month_ts_utc
            ])?;
            insert_stmt.execute(params![
                env2_id,
                5.75,
                "env2_spend_this_month",
                user_id,
                "spend",
                this_month_ts1_utc
            ])?;
        }

        let total_spent_env1 =
            get_actual_spending_this_month(&db_pool, env1_id, current_year, current_month).await?;
        assert_eq!(
            total_spent_env1, 25.75,
            "Spending for Env1 this month should be 10.50 + 15.25 = 25.75"
        );

        let total_spent_env2 =
            get_actual_spending_this_month(&db_pool, env2_id, current_year, current_month).await?;
        assert_eq!(
            total_spent_env2, 5.75,
            "Spending for Env2 this month should be 5.75"
        );

        let total_spent_non_existent_env =
            get_actual_spending_this_month(&db_pool, 999, current_year, current_month).await?;
        assert_eq!(
            total_spent_non_existent_env, 0.0,
            "Spending for non-existent envelope should be 0"
        );

        Ok(())
    }

    // --- prune_old_transactions Tests ---
    #[tokio::test]
    async fn test_prune_old_transactions_logic() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_prune_old_transactions_logic");
        let db_pool = setup_test_db().await?;
        let env_id;
        let user_id = "prune_user";

        // let now_utc: DateTime<Utc> = Utc::now(); // This was marked as unused, remove if not needed for date logic
        let today_naive = Utc::now().date_naive();

        let very_old_ts: DateTime<Utc> = Utc.from_utc_datetime(
            &(today_naive - Duration::days(400))
                .and_hms_opt(1, 0, 0)
                .unwrap(),
        );
        let recent_ts: DateTime<Utc> = Utc.from_utc_datetime(
            &(today_naive - Duration::days(10))
                .and_hms_opt(1, 0, 0)
                .unwrap(),
        );
        let borderline_ts: DateTime<Utc> = Utc.from_utc_datetime(
            &(today_naive - Duration::days(380))
                .and_hms_opt(1, 0, 0)
                .unwrap(),
        );

        {
            // Setup scope
            let conn = db_pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "PruneEnv",
                category: "cat_prune",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
            let mut stmt = conn.prepare(
                 "INSERT INTO transactions (envelope_id, amount, description, user_id, transaction_type, timestamp)
                  VALUES (?1, ?2, ?3, ?4, 'spend', ?5)"
            )?;
            stmt.execute(params![env_id, 1.0, "very_old", user_id, very_old_ts])?;
            stmt.execute(params![env_id, 2.0, "recent", user_id, recent_ts])?;
            stmt.execute(params![
                env_id,
                3.0,
                "borderline_old",
                user_id,
                borderline_ts
            ])?;
        }

        let cutoff_date_for_pruning = today_naive - Duration::days(390);

        let num_deleted = prune_old_transactions(&db_pool, cutoff_date_for_pruning).await?;
        assert_eq!(
            num_deleted, 1,
            "Should delete only the 'very_old' transaction. Num deleted: {}",
            num_deleted
        );

        {
            // Verification scope
            let conn = db_pool.lock().unwrap();
            let mut stmt_count =
                conn.prepare("SELECT COUNT(*) FROM transactions WHERE envelope_id = ?1")?;
            let remaining_count: i64 = stmt_count.query_row(params![env_id], |row| row.get(0))?;
            assert_eq!(
                remaining_count, 2,
                "Should have 2 transactions remaining. Found: {}",
                remaining_count
            );

            let mut stmt_check_desc =
                conn.prepare("SELECT COUNT(*) FROM transactions WHERE description = ?1")?;

            // Corrected assertions:
            let count_very_old: i64 =
                stmt_check_desc.query_row(params!["very_old"], |r| r.get(0))?;
            assert_eq!(
                count_very_old, 0_i64,
                "'very_old' should be gone. Count: {}",
                count_very_old
            );

            let count_recent: i64 = stmt_check_desc.query_row(params!["recent"], |r| r.get(0))?;
            assert_eq!(
                count_recent, 1_i64,
                "'recent' should exist. Count: {}",
                count_recent
            );

            let count_borderline_old: i64 =
                stmt_check_desc.query_row(params!["borderline_old"], |r| r.get(0))?;
            assert_eq!(
                count_borderline_old, 1_i64,
                "'borderline_old' should exist. Count: {}",
                count_borderline_old
            );
        }
        Ok(())
    }
}
