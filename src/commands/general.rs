use crate::Result;
use crate::bot::Context;
use poise::serenity_prelude as serenity;
use tracing::info;

/// A simple ping command to check if the bot is responsive.
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<()> {
    info!("Ping command received from user: {}", ctx.author().name);
    ctx.say("Pong!").await?;
    Ok(())
}

/// Shows a summary of all available commands.
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_>) -> Result<()> {
    info!("Help command received from user: {}", ctx.author().name);

    let action_commands = "`/spend <envelope> <amount> [user] [desc]` - Records an expense from an envelope.\n\
                           `/addfunds <envelope> <amount> [user] [desc]` - Adds funds to an envelope.\n\
                           `/use_product <product> [quantity]` - Logs an expense using a predefined product.\n\
                           `/report` - Shows a full financial report of all envelopes.";

    let management_commands = "`/manage envelope <subcommand>` - Manage envelopes (`create`, `delete`, `edit`, `list`).\n\
                               `/manage product <subcommand>` - Manage products (`add`, `delete`, `update`, `list`).";

    let utility_commands = "`/update` - Runs the monthly rollover/reset process.\n\
                            `/ping` - Checks if the bot is responsive.\n\
                            `/help` - Shows this help message.";

    let embed = serenity::CreateEmbed::default()
        .title("EnvelopeBuddy Help")
        .description("Here is a summary of all available commands for EnvelopeBuddy.")
        .color(0x5865F2) // Discord blurple
        .field("Action Commands", action_commands, false)
        .field("Management Commands", management_commands, false)
        .field("Utility Commands", utility_commands, false)
        .footer(serenity::CreateEmbedFooter::new(
            "Remember to use the subcommands for /manage!",
        ));

    ctx.send(poise::CreateReply::default().embed(embed).ephemeral(true))
        .await?;
    Ok(())
}
