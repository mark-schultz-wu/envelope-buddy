//! Transaction Discord commands - `spend`, `addfunds`, `use_product`, and `report`.
//!
//! This module contains commands that interact with the database through our core modules
//! to handle financial transactions and reporting within the envelope system.

// Inner module to suppress missing_docs warnings for poise macro-generated code
mod inner {
    #![allow(missing_docs)]

    use crate::{
        bot::BotData,
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
        #[description = "Name of the envelope to spend from"] envelope_name: String,
        #[description = "Amount to spend"] amount: f64,
        #[description = "Optional user ID (for individual envelopes)"] user: Option<String>,
        #[description = "Optional description of the expense"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Transaction";

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
        #[description = "Name of the envelope to add funds to"] envelope_name: String,
        #[description = "Amount to add"] amount: f64,
        #[description = "Optional user ID (for individual envelopes)"] user: Option<String>,
        #[description = "Optional description of the income"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Income";

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

    /// Logs an expense using a predefined product.
    ///
    /// This command creates a transaction using a product's fixed price, making it
    /// quick and easy to log common expenses without typing amounts.
    #[poise::command(slash_command, prefix_command)]
    pub async fn use_product(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the product to use"] product_name: String,
        #[description = "Optional quantity (defaults to 1)"] quantity: Option<i32>,
        #[description = "Optional description"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Product usage";

        let desc: &str = description.as_ref().map_or(DEFAULT_DESCRIPTION, |d| d);
        let qty = quantity.unwrap_or(1);

        // Get database connection from context
        let db = &ctx.data().database;

        // Find the product
        let product = crate::core::product::get_product_by_name(db, &product_name).await?;
        let Some(product) = product else {
            ctx.say(&format!("Product '{product_name}' not found"))
                .await?;
            return Ok(());
        };

        // Calculate total cost
        let total_cost = product.price * f64::from(qty);

        // Get the envelope associated with this product
        let envelope = envelope::get_envelope_by_id(db, product.envelope_id).await?;
        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "Envelope for product '{product_name}' not found"
            ))
            .await?;
            return Ok(());
        };

        // Check if envelope has sufficient funds
        if envelope.balance < total_cost {
            ctx.say(&format!(
                "Insufficient funds! Envelope '{}' has ${:.2}, but '{}' costs ${:.2}",
                envelope.name, envelope.balance, product_name, total_cost
            ))
            .await?;
            return Ok(());
        }

        // Create the transaction
        // Note: Product name is now stored in description, not via product_id FK
        let transaction = transaction::create_transaction(
            db,
            envelope.id,
            -total_cost, // Negative amount for spending
            format!("{qty}x {product_name} - {desc}"),
            ctx.author().id.to_string(),
            Some(ctx.id().to_string()),
            "use_product".to_string(),
        )
        .await?;

        ctx.say(&format!(
            "✅ Used {}x '{}' (${:.2} each) = ${:.2} from envelope '{}' - {} (Transaction ID: {})",
            qty, product_name, product.price, total_cost, envelope.name, desc, transaction.id
        ))
        .await?;

        Ok(())
    }
}

// Re-export all commands
pub use inner::*;
