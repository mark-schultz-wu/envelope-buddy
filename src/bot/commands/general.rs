//! General Discord commands - ping, help, and other utility commands.
//! This module contains simple commands that don't require database operations
//! and provide basic bot functionality and user assistance.

// Inner module to suppress missing_docs warnings for poise macro-generated code
mod inner {
    #![allow(missing_docs)]

    use crate::{
        bot::BotData,
        errors::{Error, Result},
    };

    /// Responds with "Pong!" to test bot connectivity.
    ///
    /// This is a simple health check command that doesn't require any database operations.
    #[poise::command(slash_command, prefix_command)]
    pub async fn ping(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        ctx.say("Pong!").await?;
        Ok(())
    }

    /// Displays help information about available commands.
    ///
    /// This command provides users with information about all available bot commands
    /// and their usage, helping them understand the bot's capabilities.
    #[poise::command(slash_command, prefix_command)]
    pub async fn help(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        let help_text = "**EnvelopeBuddy Help**\n\
        Here is a summary of all available commands for EnvelopeBuddy.\n\n\
        **Action Commands**\n\
        • `/spend <envelope> <amount> [user] [desc]` - Records an expense from an envelope.\n\
        • `/addfunds <envelope> <amount> [user] [desc]` - Adds funds to an envelope.\n\
        • `/use_product <product> [quantity]` - Logs an expense using a predefined product.\n\
        • `/report` - Shows a full financial report of all envelopes.\n\n\
        **Management Commands**\n\
        • `/manage envelope <subcommand>` - Manage envelopes (create, delete, edit, list).\n\
        • `/manage product <subcommand>` - Manage products (add, delete, update, list).\n\n\
        **Utility Commands**\n\
        • `/update` - Runs the monthly rollover/reset process.\n\
        • `/ping` - Checks if the bot is responsive.\n\
        • `/help` - Shows this help message.\n\n\
        Remember to use the subcommands for `/manage`!";

        ctx.say(help_text).await?;
        Ok(())
    }
}

// Re-export all commands
pub use inner::*;
