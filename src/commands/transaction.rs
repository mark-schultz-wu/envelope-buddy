use crate::bot::{Context, Error};
use crate::commands::utils::{
    generate_single_envelope_report_field_data, get_current_month_date_info,
};
use crate::db;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::AutocompleteChoice;
use tracing::{error, info, instrument, trace, warn};

/// Record an expense from an envelope.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn spend(
    ctx: Context<'_>,
    #[description = "Name of the envelope to spend from"]
    #[autocomplete = "envelope_name_autocomplete"]
    envelope_name: String,
    #[description = "Amount to spend (e.g., 50.00)"] amount: f64,
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
    #[description = "Name of the envelope to add funds to"]
    #[autocomplete = "envelope_name_autocomplete"]
    envelope_name: String,
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

async fn envelope_name_autocomplete(ctx: Context<'_>, partial: &str) -> Vec<AutocompleteChoice> {
    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request received for envelope_name");

    let data = ctx.data();
    let db_pool = &data.db_pool;
    let author_id_str = ctx.author().id.to_string();
    trace!(author_id = %author_id_str, "Author ID for autocomplete query");

    match db::suggest_accessible_envelope_names(db_pool, &author_id_str, partial).await {
        Ok(names) => {
            trace!(fetched_names = ?names, "Names fetched from DB for autocomplete");
            let choices: Vec<AutocompleteChoice> = names
                .into_iter()
                .map(|name_str| {
                    trace!(name = %name_str, value = %name_str, "Mapping to AutocompleteChoice");
                    AutocompleteChoice::new(name_str.clone(), name_str)
                })
                .collect();
            trace!(returned_choices = ?choices, "Returning choices for autocomplete");
            choices
        }
        Err(e) => {
            error!(
                "Autocomplete: Failed to fetch envelope suggestions: {:?}",
                e
            );
            Vec::new()
        }
    }
}
