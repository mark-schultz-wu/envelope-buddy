use crate::bot::{Context, Error};
use crate::db;
use std::fmt::Write;
use tracing::{info, instrument};

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

/// Shows current balances and allocations for all active envelopes.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn report(ctx: Context<'_>) -> Result<(), Error> {
    info!("Report command received from user: {}", ctx.author().name);
    let data = ctx.data(); // Access shared data (AppConfig, DbPool)

    let envelopes = db::get_all_active_envelopes(&data.db_pool).await?;

    if envelopes.is_empty() {
        ctx.say("No envelopes found. Create some with `/create-envelope` or check your initial `config.toml`.").await?;
        return Ok(());
    }

    // For distinguishing users for individual envelopes, get the configured user IDs.
    // These should ideally be part of your `Data` struct for easy access.
    // Let's assume they are available (e.g., loaded into AppConfig or directly in Data).
    // For this example, we'll just show user_id if present.
    // You would fetch these from .env in main.rs and store them in your `Data` struct.
    // For now, let's imagine you have them:
    // let user_id_1_str = &data.user_id_1; // Example: Assuming these are in Data
    // let user_id_2_str = &data.user_id_2;

    let mut reply_content = String::from("**Envelope Report**\n```\n");
    reply_content.push_str(&format!(
        "{:<20} | {:>10} / {:<10} | User\n",
        "Envelope Name", "Balance", "Allocation"
    ));
    reply_content.push_str(&"-".repeat(60)); // Separator line
    reply_content.push('\n');

    let user_id_1_str = &data.app_config.user_id_1;
    let user_id_2_str = &data.app_config.user_id_2;

    for envelope in envelopes {
        let user_indicator = if let Some(uid) = &envelope.user_id {
            if uid == user_id_1_str {
                &data.app_config.user_nickname_1
            } else if uid == user_id_2_str {
                &data.app_config.user_nickname_2
            } else {
                &format!("(User: ...{})", &uid[uid.len().saturating_sub(4)..])
            }
        } else {
            &"(Shared)".to_string()
        };

        // You could add your spending progress indicators (🟢🟡🔴) here later
        // For now, just balance / allocation

        let _ = writeln!(
            reply_content,
            "{:<20} | ${:>9.2} / ${:<9.2} | {}",
            envelope.name, envelope.balance, envelope.allocation, user_indicator
        );
    }
    reply_content.push_str("```");

    ctx.say(reply_content).await?;

    Ok(())
}
