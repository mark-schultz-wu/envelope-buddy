//! Transaction Discord commands - `spend` and `addfunds`.
//!
//! This module contains commands that interact with the database through our core modules
//! to handle financial transactions and reporting within the envelope system.

// Inner module to suppress missing_docs warnings for poise macro-generated code
mod inner {
    #![allow(missing_docs)]

    use crate::{
        bot::{BotData, handlers::autocomplete},
        core::{envelope, transaction},
        errors::{Error, Result},
    };

    /// Records an expense from an envelope.
    ///
    /// This command deducts the specified amount from the envelope balance and creates
    /// a transaction record for tracking purposes. If no user is specified, it uses the
    /// command author's ID for individual envelopes or looks for shared envelopes.
    #[poise::command(slash_command, prefix_command)]
    pub async fn spend(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope to spend from"]
        #[autocomplete = "autocomplete::autocomplete_envelope_name"]
        envelope_name: String,
        #[description = "Amount to spend"] amount: f64,
        #[description = "Optional user ID (for individual envelopes)"] user: Option<String>,
        #[description = "Optional description of the expense"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Transaction";

        // Validate amount parameter
        if amount.is_nan() || amount.is_infinite() {
            ctx.say("❌ Invalid amount: must be a valid number").await?;
            return Ok(());
        }
        if amount <= 0.0 {
            ctx.say("❌ Invalid amount: must be greater than zero")
                .await?;
            return Ok(());
        }

        // Use provided user ID or default to the command author
        let author_id = ctx.author().id.to_string();
        let user_id = user.as_ref().unwrap_or(&author_id);
        let desc = description.as_deref().unwrap_or(DEFAULT_DESCRIPTION);

        // Get database connection from context
        let db = &ctx.data().database;

        // Find the envelope by name and user (or shared if no user specified)
        let envelope = if user.is_some() {
            envelope::get_envelope_by_name_and_user(db, &envelope_name, user_id).await?
        } else {
            // If no user specified, look for shared envelopes first
            envelope::get_envelope_by_name(db, &envelope_name).await?
        };

        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "❌ Envelope '{envelope_name}' not found. Use `/envelopes` to see available envelopes.",
            ))
            .await?;
            return Ok(());
        };

        // Check if envelope has sufficient funds
        if envelope.balance < amount {
            ctx.say(&format!(
                "❌ Insufficient funds! Envelope '{}' has ${:.2}, but you're trying to spend ${:.2}",
                envelope_name, envelope.balance, amount
            ))
            .await?;
            return Ok(());
        }

        // Create the transaction (negative amount for spending)
        let transaction_result = transaction::create_transaction(
            db,
            envelope.id,
            -amount, // Negative amount for spending
            desc.to_string(),
            author_id.clone(),
            None, // No Discord message ID
            "spend".to_string(),
        )
        .await?;

        ctx.say(&format!(
            "✅ Spent ${:.2} from envelope '{}' - {} (Transaction ID: {})",
            amount, envelope_name, desc, transaction_result.id
        ))
        .await?;

        Ok(())
    }

    /// Adds funds to an envelope.
    ///
    /// This command increases the envelope balance by the specified amount and creates
    /// a transaction record for tracking purposes. If no user is specified, it uses the
    /// command author's ID for individual envelopes or looks for shared envelopes.
    #[poise::command(slash_command, prefix_command)]
    pub async fn addfunds(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope to add funds to"]
        #[autocomplete = "autocomplete::autocomplete_envelope_name"]
        envelope_name: String,
        #[description = "Amount to add"] amount: f64,
        #[description = "Optional user ID (for individual envelopes)"] user: Option<String>,
        #[description = "Optional description of the income"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Income";

        // Validate amount parameter
        if amount.is_nan() || amount.is_infinite() {
            ctx.say("❌ Invalid amount: must be a valid number").await?;
            return Ok(());
        }
        if amount <= 0.0 {
            ctx.say("❌ Invalid amount: must be greater than zero")
                .await?;
            return Ok(());
        }

        // Use provided user ID or default to the command author
        let author_id = ctx.author().id.to_string();
        let user_id = user.as_ref().unwrap_or(&author_id);
        let desc = description.as_deref().unwrap_or(DEFAULT_DESCRIPTION);

        // Get database connection from context
        let db = &ctx.data().database;

        // Find the envelope by name and user (or shared if no user specified)
        let envelope = if user.is_some() {
            envelope::get_envelope_by_name_and_user(db, &envelope_name, user_id).await?
        } else {
            // If no user specified, look for shared envelopes first
            envelope::get_envelope_by_name(db, &envelope_name).await?
        };

        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "❌ Envelope '{envelope_name}' not found. Use `/envelopes` to see available envelopes.",
            ))
            .await?;
            return Ok(());
        };

        // Create the transaction (positive amount for adding funds)
        let transaction_result = transaction::create_transaction(
            db,
            envelope.id,
            amount, // Positive amount for adding funds
            desc.to_string(),
            author_id.clone(),
            None, // No Discord message ID
            "addfunds".to_string(),
        )
        .await?;

        ctx.say(&format!(
            "✅ Added ${:.2} to envelope '{}' - {} (Transaction ID: {})",
            amount, envelope_name, desc, transaction_result.id
        ))
        .await?;

        Ok(())
    }
}

// Re-export all commands
pub use inner::*;
