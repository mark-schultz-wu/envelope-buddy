use crate::bot::{Context, Error};
use crate::commands::utils::{
    generate_single_envelope_report_field_data, get_current_month_date_info,
};
use crate::db::{self, CreateUpdateEnvelopeArgs};
use chrono::{Datelike, Duration, Local, NaiveDate};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tracing::{info, instrument};

/// Shows current balances, allocations, and spending progress for all active envelopes.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn report(ctx: Context<'_>) -> Result<(), Error> {
    info!(
        "Full Report command received from user: {}",
        ctx.author().name
    );
    let data = ctx.data();
    let app_config = Arc::clone(&data.app_config);
    let db_pool = &data.db_pool;

    let envelopes = db::get_all_active_envelopes(db_pool).await?;

    if envelopes.is_empty() {
        ctx.say("No envelopes found.").await?;
        return Ok(());
    }

    let (now_local_date, current_day_of_month, days_in_month, year, month) =
        get_current_month_date_info();

    if days_in_month == 0.0 {
        // Should not happen with new helper
        return Err(Error::Command(
            "Failed to determine days in month.".to_string(),
        ));
    }

    let mut embed_fields = Vec::new();
    for envelope in &envelopes {
        // Iterate by reference
        let (field_name, field_value) = generate_single_envelope_report_field_data(
            envelope,
            &app_config,
            db_pool,
            current_day_of_month,
            days_in_month,
            year,
            month,
        )
        .await?;
        embed_fields.push((field_name, field_value, false)); // false for not inline
    }

    let report_embed = serenity::CreateEmbed::default()
        .title("**Full Envelope Report**")
        .description(format!(
            "As of: {} (Day {:.0}/{:.0} of month)",
            now_local_date.format("%Y-%m-%d"),
            current_day_of_month,
            days_in_month
        ))
        .color(0x3498DB)
        .fields(embed_fields)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "EnvelopeBuddy | {} envelopes",
            envelopes.len()
        )));

    ctx.send(poise::CreateReply::default().embed(report_embed))
        .await?;
    Ok(())
}

/// Performs the monthly envelope reset/rollover and data pruning.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn update(ctx: Context<'_>) -> Result<(), Error> {
    let author_name = ctx.author().name.clone();
    info!("Update command received from user: {}", author_name);

    let data = ctx.data();
    let db_pool = &data.db_pool;

    let now_local_date = Local::now().date_naive();
    let current_month_str = now_local_date.format("%Y-%m").to_string();

    // 1. Check if update has already run for this month
    let last_update_key = "last_update_processed_month";
    if let Some(last_processed) = db::get_system_state_value(db_pool, last_update_key).await? {
        if last_processed == current_month_str {
            let reply = format!(
                "The monthly update for {} has already been processed.",
                current_month_str
            );
            ctx.say(reply).await?;
            info!(
                "Update for {} already processed. Aborting.",
                current_month_str
            );
            return Ok(());
        }
    }

    info!(
        "Proceeding with monthly update for {}...",
        current_month_str
    );
    ctx.defer_ephemeral().await?; // Acknowledge interaction; processing might take a moment

    // --- Perform updates within a transaction (conceptual here, db ops are individual) ---
    // For true atomicity, all DB writes below should be in one db.rs function using a transaction.
    // However, rusqlite's Arc<Mutex<Connection>> makes passing a `Transaction` object tricky across awaits.
    // We'll perform operations sequentially; if one fails, subsequent ones won't run,
    // but it won't automatically roll back previous ones without explicit transaction management.
    // A single db.rs function `perform_monthly_update` would be better for atomicity.

    let envelopes = db::get_all_active_envelopes(db_pool).await?;
    let mut envelopes_processed_count = 0;

    for envelope in envelopes {
        let new_balance = if envelope.rollover {
            envelope.balance + envelope.allocation
        } else {
            envelope.allocation
        };
        db::update_envelope_balance(db_pool, envelope.id, new_balance).await?;
        envelopes_processed_count += 1;
    }
    info!(
        "Processed {} envelopes for balance updates.",
        envelopes_processed_count
    );

    // 2. Prune old transactions (e.g., older than 13 months from the first day of the current month)
    let first_day_current_month =
        NaiveDate::from_ymd_opt(now_local_date.year(), now_local_date.month(), 1).unwrap();
    let cutoff_date_for_pruning = first_day_current_month - Duration::days(365 + 30); // Approx 13 months ago
    // A more precise way: subtract 13 months then go to first day of that month.
    // Or simply, any transaction whose month is < (current_month - 12_months).
    // For simplicity, let's use an approximate fixed duration.
    // A better date for pruning would be something like:
    // current_year, current_month, day 1, then subtract 1 year.
    let transactions_pruned_count =
        db::prune_old_transactions(db_pool, cutoff_date_for_pruning).await?;
    info!("Pruned {} old transactions.", transactions_pruned_count);

    // 3. Record successful update for this month
    db::set_system_state_value(db_pool, last_update_key, &current_month_str).await?;
    info!(
        "Successfully recorded update for month {}.",
        current_month_str
    );

    let success_reply = format!(
        "✅ Monthly update for {} completed!\n- {} envelopes processed.\n- {} old transactions pruned.",
        current_month_str, envelopes_processed_count, transactions_pruned_count
    );
    ctx.say(success_reply).await?;

    Ok(())
}

#[poise::command(
    slash_command,
    subcommands("create_envelope", "delete_envelope"), // Add "edit_envelope", "set_balance_envelope" here in the future
    rename = "envelope"
)]
#[instrument(skip(ctx))]
pub async fn envelope_manage(ctx: Context<'_>) -> Result<(), Error> {
    let help_text = "Envelope management subcommands: `create`, `delete`.\n\
                     (Soon: `edit`, `set_balance`)\n\
                     Example: `/manage envelope create name:\"New Budget\" ...`";
    ctx.send(
        poise::CreateReply::default()
            .content(help_text)
            .ephemeral(true),
    )
    .await?;
    Ok(())
}

/// Creates a new envelope or re-enables/updates a soft-deleted one.
#[poise::command(slash_command, rename = "create")]
#[instrument(skip(ctx))]
pub async fn create_envelope(
    ctx: Context<'_>,
    #[description = "Name for the envelope (required)"] name: String,
    #[description = "Category (e.g., necessary, QoL). If re-enabling, leave blank to keep old."]
    category: Option<String>,
    #[description = "Monthly allocation. If re-enabling, leave blank to keep old."]
    allocation: Option<f64>,
    #[description = "Is this an individual envelope? If re-enabling, leave blank to keep old."]
    is_individual: Option<bool>,
    #[description = "Balance rolls over? If re-enabling, leave blank to keep old."]
    rollover: Option<bool>,
) -> Result<(), Error> {
    let author_name = ctx.author().name.clone();
    // Log which options were provided
    tracing::info!(
        "Create_envelope from {}: name='{}', category={:?}, alloc={:?}, indiv={:?}, rollover={:?}",
        author_name,
        name,
        category,
        allocation,
        is_individual,
        rollover
    );

    if name.trim().is_empty() {
        ctx.say("Envelope name cannot be empty.").await?;
        return Ok(());
    }
    if let Some(alloc) = allocation {
        if alloc < 0.0 {
            ctx.say("Allocation amount cannot be negative.").await?;
            return Ok(());
        }
    }
    if let Some(cat) = &category {
        if cat.trim().is_empty() {
            ctx.say("Category, if provided, cannot be empty.").await?;
            return Ok(());
        }
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;
    let app_config = Arc::clone(&data.app_config);

    let args: CreateUpdateEnvelopeArgs = CreateUpdateEnvelopeArgs {
        name: &name,
        category_opt: category.as_deref(),
        allocation_opt: allocation,
        is_individual_cmd_opt: is_individual,
        rollover_opt: rollover,
    };
    let results = db::create_or_reenable_envelope_flexible(
        db_pool,
        &args,
        &app_config.user_id_1,
        &app_config.user_id_2,
    )
    .await?;

    let reply = results.join("\n");
    ctx.say(format!(
        "**Envelope Creation/Re-enable Results:**\n{}",
        reply
    ))
    .await?;

    Ok(())
}

/// Soft-deletes an envelope. The envelope can be re-enabled by trying to create it again.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn delete_envelope(
    ctx: Context<'_>,
    #[description = "Name of the envelope to delete"] name: String,
) -> Result<(), Error> {
    let author_id_str = ctx.author().id.to_string();
    info!(
        "Delete_envelope command received from user: {} ({}) for envelope: '{}'",
        ctx.author().name,
        author_id_str,
        name
    );

    let db_pool = &ctx.data().db_pool;

    match db::soft_delete_envelope(db_pool, &name, &author_id_str).await? {
        true => {
            ctx.say(format!("Envelope '{}' has been soft-deleted. You can re-enable it by trying to create it again with the same name.", name)).await?;
            info!(
                "Successfully soft-deleted envelope '{}' for user {}",
                name, author_id_str
            );
        }
        false => {
            ctx.say(format!("Could not find an active envelope named '{}' that you own or is shared, or it was already deleted.", name)).await?;
            info!(
                "Failed to soft-delete envelope '{}' for user {} (not found or permission issue).",
                name, author_id_str
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::db::{self, DbPool};
    use crate::errors::Result;
    use crate::models::Envelope;
    use chrono::{DateTime, TimeZone, Utc};
    use rusqlite::params;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::EnvFilter;

    // Helper to init tracing for tests
    fn init_test_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("trace")),
            )
            .with_test_writer()
            .try_init();
    }

    // You'll need your setup_test_db() helper here or from a shared location
    async fn setup_test_db_for_commands() -> Result<DbPool> {
        let conn = rusqlite::Connection::open_in_memory()?;
        db::schema::create_tables(&conn)?; // Make sure create_tables is accessible
        Ok(Arc::new(Mutex::new(conn)))
    }

    #[tokio::test]
    async fn test_generate_single_envelope_report_field_data_shared() -> Result<()> {
        init_test_tracing();
        tracing::info!("--- Running test_generate_single_envelope_report_field_data_shared ---");

        let db_pool = setup_test_db_for_commands().await?;
        let app_config = Arc::new(AppConfig {
            envelopes_from_toml: vec![],
            user_id_1: "user1_test_id".to_string(), // From .env for AppConfig
            user_id_2: "user2_test_id".to_string(), // From .env for AppConfig
            user_nickname_1: "UserOneTest".to_string(),
            user_nickname_2: "UserTwoTest".to_string(),
            database_path: String::new(), // Not directly used by the function under test
        });

        // Envelope properties for the test
        let envelope_name = "Groceries";
        let envelope_category = "Necessary";
        let envelope_allocation = 500.0;
        let envelope_balance = 450.0; // Initial balance for testing logic
        let envelope_is_individual = false;
        let envelope_user_id: Option<String> = None;
        let envelope_rollover = false;
        let envelope_is_deleted = false;

        let db_generated_envelope_id;

        // Insert the envelope and get its DB-generated ID
        {
            let conn = db_pool.lock().unwrap();
            let mut stmt_env_insert = conn.prepare_cached(
                "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
            )?;
            db_generated_envelope_id = stmt_env_insert.insert(params![
                envelope_name,
                envelope_category,
                envelope_allocation,
                envelope_balance,
                envelope_is_individual,
                envelope_user_id, // Option<String> is fine here with rusqlite
                envelope_rollover,
                envelope_is_deleted
            ])?;
            tracing::debug!(
                "Inserted test envelope '{}', ID from DB: {}",
                envelope_name,
                db_generated_envelope_id
            );
        }

        // Construct the Envelope struct with the DB-generated ID to pass to the function
        let test_envelope = Envelope {
            id: db_generated_envelope_id,
            name: envelope_name.to_string(),
            category: envelope_category.to_string(),
            allocation: envelope_allocation,
            balance: envelope_balance,
            is_individual: envelope_is_individual,
            user_id: envelope_user_id,
            rollover: envelope_rollover,
            is_deleted: envelope_is_deleted,
        };

        // Fixed date parameters for consistent testing
        let year_for_test = 2025;
        let month_for_test = 5; // May
        let day_for_test = 15.0; // 15th day
        let days_in_may_2025 = 31.0; // May has 31 days
        tracing::debug!(
            year_for_test,
            month_for_test,
            day_for_test,
            days_in_may_2025,
            "Date parameters for test"
        );

        // Insert a transaction for "actual spending" using the DB-generated envelope ID
        let transaction_ts: DateTime<Utc> = Utc
            .with_ymd_and_hms(year_for_test, month_for_test, 10, 12, 0, 0)
            .unwrap();
        {
            let conn = db_pool.lock().unwrap();
            let mut stmt_tx = conn.prepare(
                "INSERT INTO transactions (envelope_id, amount, description, user_id, transaction_type, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            )?;
            stmt_tx.execute(params![
                db_generated_envelope_id, // Use the ID from the database insert
                50.0,
                "test spend",
                "test_user_ spender", // A distinct user ID for the transaction
                "spend",
                transaction_ts
            ])?;
            tracing::debug!(
                "Inserted test transaction for envelope_id: {}",
                db_generated_envelope_id
            );
        }

        // Call the function under test
        let (field_name, field_value) = generate_single_envelope_report_field_data(
            &test_envelope,
            &app_config,
            &db_pool,
            day_for_test,
            days_in_may_2025,
            year_for_test,
            month_for_test,
        )
        .await?;

        eprintln!(
            "Asserting Field Name: Expected 'Groceries (Shared)', Got: '{}'",
            field_name
        );
        eprintln!(
            "Asserting Field Value:\nExpected Contains:\nBalance: $450.00 / Alloc: $500.00\nSpent (Actual): $50.00\nExpected Pace: $241.94\nStatus: 🟢\nActual Field Value:\n{}",
            field_value
        );

        assert_eq!(field_name, "Groceries (Shared)");

        // Check for key components in the field value
        assert!(
            field_value.contains("Balance: $450.00 / Alloc: $500.00"),
            "Checking Balance/Alloc. Actual: {}",
            field_value
        );
        assert!(
            field_value.contains("Spent (Actual): $50.00"),
            "Checking Spent (Actual). Actual: {}",
            field_value
        );
        // Calculation for expected pace: (500.0 / 31.0) * 15.0 = 241.93548...
        assert!(
            field_value.contains("Expected Pace: $241.94"),
            "Checking Expected Pace. Actual: {}",
            field_value
        );
        assert!(
            field_value.contains("Status: 🟢"),
            "Checking Status Emoji. Actual: {}",
            field_value
        );

        Ok(())
    }

    // Add more tests for individual envelopes, different spending scenarios for the emoji, etc.
}
