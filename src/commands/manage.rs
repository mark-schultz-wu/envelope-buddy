use crate::Result;
use crate::bot::Context;
use crate::commands::envelope::envelope_manage;
use crate::commands::product::product_manage;

/// Parent command for managing all other commands.
///
/// TODO: doc comment
#[poise::command(slash_command, subcommands("product_manage", "envelope_manage"))]
pub async fn manage(ctx: Context<'_>) -> Result<()> {
    // TODO: help text
    let response_text = "Welcome to the management command center!\n\
                         Available command groups:\n\
                         - `product`: Manage predefined products (e.g., `/manage product list`).\n\
                         - `envelope`: Manage envelopes (e.g., `/manage envelope create ...`).";

    ctx.send(
        poise::CreateReply::default()
            .content(response_text)
            .ephemeral(true),
    )
    .await?;
    Ok(())
}
