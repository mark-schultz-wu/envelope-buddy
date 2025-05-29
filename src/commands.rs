use crate::bot::{Context, Error};
use tracing::info;

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
