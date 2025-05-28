use crate::Result;
use crate::bot::Context;
use tracing::info;

/// A simple ping command to check if the bot is responsive.
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<()> {
    info!("Ping command received from user: {}", ctx.author().name);
    ctx.say("Pong!").await?;
    Ok(())
}
