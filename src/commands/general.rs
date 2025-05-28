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

use poise::serenity_prelude::AutocompleteChoice;
use tracing::trace; // For logging

// Assume your existing envelope_name_autocomplete function is accessible
// If it's in another module, you'll need to make it pub and import it,
// or temporarily copy its simplified/hardcoded version here for this test.
// For this example, let's assume it's in scope or defined here:
async fn envelope_name_autocomplete(ctx: Context<'_>, partial: &str) -> Vec<AutocompleteChoice> {
    trace!(user = %ctx.author().name, partial_input = partial, "TESTAUTO Autocomplete request received");
    // Using a simplified hardcoded version for this specific test
    let choices = ["Groceries_Test", "Hobby_Test", "Eating Out_Test"]
        .into_iter()
        .filter(|name| name.to_lowercase().starts_with(&partial.to_lowercase()))
        .map(|name| AutocompleteChoice::new(name.to_string(), name.to_string()))
        .collect::<Vec<_>>();
    trace!(returned_choices = ?choices, "TESTAUTO Returning hardcoded choices");
    choices
}

/// A minimal command to test autocomplete.
#[poise::command(slash_command, guild_only)] // Add guild_only if you want it only in dev guild
pub async fn testautocomplete(
    ctx: Context<'_>,
    #[description = "Some input with autocomplete"]
    #[autocomplete = "envelope_name_autocomplete"] // Reference the SAME autocomplete function
    my_input: String,
) -> Result<()> {
    ctx.say(format!("You selected: {}", my_input)).await?;
    Ok(())
}
