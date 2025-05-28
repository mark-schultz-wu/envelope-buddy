use crate::bot::{Context, Error};
use crate::config::AppConfig;
use crate::db::DbPool;
use crate::db::{self, CreateUpdateEnvelopeArgs};
use crate::models::Envelope;
use chrono::{Datelike, Duration, Local, NaiveDate};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tracing::{info, instrument, warn};

/// A simple ping command to check if the bot is responsive.
#[poise::command(slash_command)]
pub async fn ping(
    // This 'ping' function is what the macro processes
    ctx: Context<'_>,
) -> Result<(), Error> {
    info!("Ping command received from user: {}", ctx.author().name);
    // ...
    ctx.say("Pong!").await?;
    Ok(())
}

// Helper function to get current date info
fn get_current_month_date_info() -> (NaiveDate, f64, f64, i32, u32) {
    let now_local_date = Local::now().date_naive();
    let current_day_of_month = now_local_date.day() as f64;
    let year = now_local_date.year();
    let month = now_local_date.month();
    let days_in_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .pred_opt()
    .unwrap()
    .day() as f64;
    (
        now_local_date,
        current_day_of_month,
        days_in_month,
        year,
        month,
    )
}

// Helper function to generate report field data for a single envelope
async fn generate_single_envelope_report_field_data(
    envelope: &Envelope,
    app_config: &Arc<AppConfig>, // Pass Arc<AppConfig>
    db_pool: &DbPool,            // Pass DbPool
    current_day_of_month: f64,
    days_in_month: f64,
    year: i32,
    month: u32,
) -> Result<(String, String), Error> {
    // Returns (field_name, field_value)
    let user_indicator = if let Some(uid) = &envelope.user_id {
        if uid == &app_config.user_id_1 {
            format!("({})", app_config.user_nickname_1)
        } else if uid == &app_config.user_id_2 {
            format!("({})", app_config.user_nickname_2)
        } else {
            format!("(ID: ...{})", &uid[uid.len().saturating_sub(4)..])
        }
    } else {
        "(Shared)".to_string()
    };

    let actual_monthly_spending =
        db::get_actual_spending_this_month(db_pool, envelope.id, year, month).await?;
    let spent_for_indicator = f64::max(0.0, envelope.allocation - envelope.balance);
    let daily_allocation = if envelope.allocation > 0.0 {
        envelope.allocation / days_in_month
    } else {
        0.0
    };
    let expected_spending_to_date = daily_allocation * current_day_of_month;

    let status_emoji = if envelope.allocation <= 0.0 {
        "⚪"
    } else if spent_for_indicator <= expected_spending_to_date * 0.90 {
        "🟢"
    } else if spent_for_indicator > expected_spending_to_date * 1.10 {
        "🔴"
    } else {
        "🟡"
    };

    let field_name = format!("{} {}", envelope.name, user_indicator);
    let field_value = format!(
        "Balance: ${:.2} / Alloc: ${:.2}\nSpent (Actual): ${:.2}\nExpected Pace: ${:.2}\nStatus: {}",
        envelope.balance,
        envelope.allocation,
        actual_monthly_spending,
        expected_spending_to_date,
        status_emoji
    );
    Ok((field_name, field_value))
}

/// Record an expense from an envelope.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn spend(
    ctx: Context<'_>,
    #[description = "Name of the envelope to spend from"] envelope_name: String,
    #[description = "Amount to spend (e.g., 10.50)"] amount: f64,
    #[description = "Description of the expense (optional)"] description: Option<String>,
) -> Result<(), Error> {
    let author_id_str = ctx.author().id.to_string();
    info!(
        "Spend command received from user: {} ({}) for envelope: '{}', amount: {}, description: {:?}",
        ctx.author().name,
        author_id_str,
        envelope_name,
        amount,
        description
    );

    if amount <= 0.0 {
        ctx.say("The amount must be a positive number.").await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;

    // Start a database transaction for atomicity
    // To do this properly with Arc<Mutex<Connection>>, you'd need a helper
    // or to pass the locked connection through. For simplicity now, we'll do sequential operations.
    // A better approach would be a db::perform_spend_transaction function.

    let target_envelope =
        match db::get_user_or_shared_envelope(db_pool, &envelope_name, &author_id_str).await? {
            Some(env) => env,
            None => {
                ctx.say(format!(
                    "Could not find an envelope named '{}' that you can use.",
                    envelope_name
                ))
                .await?;
                return Ok(());
            }
        };

    // Check if the envelope actually belongs to the user if it's individual
    // The query get_user_or_shared_envelope should handle this by prioritizing user's own.
    if target_envelope.is_individual && target_envelope.user_id.as_deref() != Some(&author_id_str) {
        warn!(
            "User {} tried to spend from {}'s envelope '{}'. Denied.",
            author_id_str,
            target_envelope.user_id.as_deref().unwrap_or("unknown"),
            envelope_name
        );
        ctx.say(format!(
            "You cannot spend from '{}' as it does not belong to you.",
            envelope_name
        ))
        .await?;
        return Ok(());
    }

    let new_balance = target_envelope.balance - amount;
    // Optional: Check for overdraft if you want to prevent it
    // if new_balance < 0.0 {
    //     ctx.say(format!("Spending ${:.2} from '{}' would overdraft it (current balance ${:.2}). Transaction cancelled.", amount, envelope_name, target_envelope.balance)).await?;
    //     return Ok(());
    // }

    // Update balance
    db::update_envelope_balance(db_pool, target_envelope.id, new_balance).await?;

    // Create transaction record
    let desc_str = description.as_deref().unwrap_or("No description");
    let discord_message_id = ctx.id().to_string(); // Gets the interaction ID, which can serve as a message_id ref

    db::create_transaction(
        db_pool,
        target_envelope.id,
        amount, // Store the positive spending amount
        desc_str,
        &author_id_str,
        Some(&discord_message_id),
        "spend",
    )
    .await?;

    // --- Mini-Report Logic ---
    // Fetch the *updated* state of the envelope
    let updated_envelope = match db::get_user_or_shared_envelope(
        db_pool,
        &envelope_name,
        &author_id_str,
    )
    .await?
    {
        Some(env) => env,
        None => {
            // This shouldn't happen if the above logic was correct, but handle defensively
            warn!(
                "Failed to fetch updated envelope for mini-report: {}",
                envelope_name
            );
            ctx.say(format!(
                "Spent ${:.2} from '{}'. New balance: ${:.2}. (Could not fetch mini-report details).",
                amount, envelope_name, new_balance
            )).await?;
            return Ok(());
        }
    };

    let (_now_local_date, current_day_of_month, days_in_month, year, month) =
        get_current_month_date_info();
    let (field_name, field_value) = generate_single_envelope_report_field_data(
        &updated_envelope,
        &ctx.data().app_config,
        db_pool,
        current_day_of_month,
        days_in_month,
        year,
        month,
    )
    .await?;

    let mini_report_embed = serenity::CreateEmbed::default()
        .title(format!("Update for: {}", field_name)) // field_name includes user indicator
        .description(field_value) // The multi-line details
        .color(0xF1C40F); // Yellow color for spend/update

    ctx.send(
        poise::CreateReply::default()
            .embed(mini_report_embed)
            .ephemeral(false), // Make it visible to everyone in the channel
    )
    .await?;

    Ok(())
}

/// Adds funds to a specified envelope.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn addfunds(
    ctx: Context<'_>,
    #[description = "Name of the envelope to add funds to"] envelope_name: String,
    #[description = "Amount to add (e.g., 50.00)"] amount: f64,
    #[description = "Reason for the addition (optional)"] description: Option<String>,
) -> Result<(), Error> {
    let author_id_str = ctx.author().id.to_string();
    info!(
        "AddFunds command received from user: {} ({}) for envelope: '{}', amount: {}, description: {:?}",
        ctx.author().name,
        author_id_str,
        envelope_name,
        amount,
        description
    );

    if amount <= 0.0 {
        ctx.say("The amount to add must be a positive number.")
            .await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;

    let target_envelope =
        match db::get_user_or_shared_envelope(db_pool, &envelope_name, &author_id_str).await? {
            Some(env) => env,
            None => {
                ctx.say(format!(
                    "Could not find an envelope named '{}' that you can use.",
                    envelope_name
                ))
                .await?;
                return Ok(());
            }
        };

    // Similar to /spend, ensure the user can modify this envelope if it's individual
    if target_envelope.is_individual && target_envelope.user_id.as_deref() != Some(&author_id_str) {
        warn!(
            "User {} tried to add funds to {}'s envelope '{}'. Denied.",
            author_id_str,
            target_envelope.user_id.as_deref().unwrap_or("unknown"),
            envelope_name
        );
        ctx.say(format!(
            "You cannot add funds to '{}' as it does not belong to you or is not shared.",
            envelope_name
        ))
        .await?;
        return Ok(());
    }

    let new_balance = target_envelope.balance + amount;

    // Update balance
    db::update_envelope_balance(db_pool, target_envelope.id, new_balance).await?;

    // Create transaction record
    let desc_str =
        description.unwrap_or_else(|| format!("Manual funds addition by {}", ctx.author().name));
    let discord_interaction_id = ctx.id().to_string();

    db::create_transaction(
        db_pool,
        target_envelope.id,
        amount, // Store the positive added amount
        &desc_str,
        &author_id_str, // User who initiated the addition
        Some(&discord_interaction_id),
        "deposit",
    )
    .await?;

    // --- Mini-Report Logic ---
    let updated_envelope =
        match db::get_user_or_shared_envelope(db_pool, &envelope_name, &author_id_str).await? {
            Some(env) => env,
            None => {
                warn!(
                    "Failed to fetch updated envelope for mini-report: {}",
                    envelope_name
                );
                ctx.say(format!(
                "Added ${:.2} to '{}'. New balance: ${:.2}. (Could not fetch mini-report details).",
                amount, envelope_name, new_balance
            )).await?;
                return Ok(());
            }
        };

    let (_now_local_date, current_day_of_month, days_in_month, year, month) =
        get_current_month_date_info();
    let (field_name, field_value) = generate_single_envelope_report_field_data(
        &updated_envelope,
        &data.app_config,
        db_pool,
        current_day_of_month,
        days_in_month,
        year,
        month,
    )
    .await?;

    let mini_report_embed = serenity::CreateEmbed::default()
        .title(format!("Update for: {}", field_name))
        .description(field_value)
        .color(0x2ECC71); // Green color for additions

    ctx.send(
        poise::CreateReply::default()
            .embed(mini_report_embed)
            .ephemeral(false), // Make it visible
    )
    .await?;

    Ok(())
}

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

/// Creates a new envelope or re-enables/updates a soft-deleted one.
#[poise::command(slash_command)]
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
