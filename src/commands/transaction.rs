use crate::bot::{Context, Error};
use crate::commands::utils::{envelope_name_autocomplete, send_mini_report_embed};
use crate::db;
use poise::serenity_prelude as serenity;
use tracing::{info, instrument, warn};

/// Record an expense from an envelope.
///
/// Allows a user to specify an envelope they own or a shared envelope,
/// an amount, and an optional description to record a spending transaction.
/// The envelope's balance will be debited by the amount, and a transaction
/// record will be created. A mini-report showing the updated envelope status
/// will be sent as a reply.
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
    #[description = "User to spend on behalf of (for individual envelopes)"] user: Option<
        serenity::User,
    >,
) -> Result<(), Error> {
    let author_id_str = ctx.author().id.to_string();
    let target_user_id_str = user
        .as_ref()
        .map_or(author_id_str.clone(), |u| u.id.to_string());

    info!(
        "Spend command received from user: {} ({}) for envelope: '{}', amount: {}, description: {:?}, target_user: {}",
        ctx.author().name,
        author_id_str,
        envelope_name,
        amount,
        description,
        target_user_id_str
    );

    if amount <= 0.0 {
        ctx.say("The amount must be a positive number.").await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;
    let app_config = &data.app_config;

    // 1. Fetch the target envelope to get its ID and current balance
    let target_envelope = match db::get_user_or_shared_envelope(
        db_pool,
        &envelope_name,
        &target_user_id_str,
    )
    .await?
    {
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

    // 2. Permission check for correct parameter usage
    if user.is_some() && !target_envelope.is_individual {
        ctx.say(format!(
            "Cannot specify a user for a shared envelope like '{}'.",
            envelope_name
        ))
        .await?;
        return Ok(());
    }

    let desc_str = description.as_deref().unwrap_or("No description");
    let discord_interaction_id = ctx.id().to_string();

    // 3. Call the atomic database function to execute the spend
    db::transactions::execute_spend_transaction(
        db_pool,
        target_envelope.id,
        target_envelope.balance,
        amount,
        desc_str,
        &author_id_str, // The initiator is always the author
        Some(&discord_interaction_id),
    )
    .await?;

    // 4. Fetch the updated state of the envelope for the mini-report
    let updated_envelope = match db::get_user_or_shared_envelope(
        db_pool,
        &envelope_name,
        &target_user_id_str,
    )
    .await?
    {
        Some(env) => env,
        None => {
            warn!(
                "Failed to fetch updated envelope '{}' for mini-report after successful spend. User: {}",
                envelope_name, author_id_str
            );
            ctx.say(format!(
                "Successfully spent ${:.2} from '{}'. (Could not generate detailed mini-report).",
                amount, envelope_name
            ))
            .await?;
            return Ok(());
        }
    };

    // 5. Send the mini-report using the utility function
    crate::commands::utils::send_mini_report_embed(ctx, &updated_envelope, app_config, db_pool)
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
    #[description = "User to add funds on behalf of (for individual envelopes)"] user: Option<
        serenity::User,
    >,
) -> Result<(), Error> {
    let author_id_str = ctx.author().id.to_string();
    let target_user_id_str = user
        .as_ref()
        .map_or(author_id_str.clone(), |u| u.id.to_string());

    info!(
        "AddFunds command received from user: {} ({}) for envelope: '{}', amount: {}, description: {:?}, target_user: {}",
        ctx.author().name,
        author_id_str,
        envelope_name,
        amount,
        description,
        target_user_id_str,
    );
    if amount <= 0.0 {
        ctx.say("The amount to add must be a positive number.")
            .await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;

    let target_envelope = match db::get_user_or_shared_envelope(
        db_pool,
        &envelope_name,
        &target_user_id_str,
    )
    .await?
    {
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

    if user.is_some() && !target_envelope.is_individual {
        ctx.say(format!(
            "Cannot specify a user for a shared envelope like '{}'.",
            envelope_name
        ))
        .await?;
        return Ok(());
    }

    let new_balance = target_envelope.balance + amount;

    db::update_envelope_balance(db_pool, target_envelope.id, new_balance).await?;

    let desc_str =
        description.unwrap_or_else(|| format!("Manual funds addition by {}", ctx.author().name));
    let discord_interaction_id = ctx.id().to_string();

    db::create_transaction(
        db_pool,
        target_envelope.id,
        amount,
        &desc_str,
        &author_id_str, // User who initiated the addition
        Some(&discord_interaction_id),
        "deposit",
    )
    .await?;

    let updated_envelope = match db::get_user_or_shared_envelope(
        db_pool,
        &envelope_name,
        &target_user_id_str,
    )
    .await?
    {
        Some(env) => env,
        None => {
            warn!(
                "Failed to fetch updated envelope for mini-report: {}",
                envelope_name
            );
            ctx.say(format!(
                "Added ${:.2} to '{}'. New balance: ${:.2}. (Could not fetch mini-report details).",
                amount, envelope_name, new_balance
            ))
            .await?;
            return Ok(());
        }
    };

    send_mini_report_embed(ctx, &updated_envelope, &ctx.data().app_config, db_pool).await?;

    Ok(())
}
