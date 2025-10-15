//! Product Discord commands - `product_manage` and `use_product`.
//!
//! This module contains commands for managing predefined products and using them
//! to quickly log expenses.

// Inner module to suppress missing_docs warnings for poise macro-generated code
mod inner {
    #![allow(missing_docs)]

    use crate::{
        bot::{BotData, handlers::autocomplete},
        core::{envelope, product, transaction},
        errors::{Error, Result},
    };
    use poise::serenity_prelude as serenity;

    /// Parent command for managing predefined, fixed-price products.
    ///
    /// Products allow for quick logging of expenses by linking a named item to a
    /// specific envelope and unit price. This command groups subcommands for adding,
    /// listing, updating, and deleting products.
    #[poise::command(
        slash_command,
        subcommands("product_add", "product_list", "product_update", "product_delete")
    )]
    pub async fn product_manage(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        let help_text = "Product management command. Available subcommands:\n\
            `/product_manage add` - Add a new product\n\
            `/product_manage list` - List all products\n\
            `/product_manage update` - Update a product's price\n\
            `/product_manage delete` - Delete a product";

        ctx.say(help_text).await?;
        Ok(())
    }

    /// Adds a new fixed-price product to the system.
    ///
    /// The unit price of the product is automatically calculated from the `total_price`
    /// and `quantity` provided (quantity defaults to 1 if not specified).
    #[poise::command(slash_command, rename = "add")]
    pub async fn product_add(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Unique name for the product (e.g., '12-Pack Soda')"] name: String,
        #[description = "Total price paid for the given quantity (e.g., 6.99)"] total_price: f64,
        #[description = "Envelope this product's cost should come from"]
        #[autocomplete = "autocomplete::autocomplete_envelope_name"]
        envelope_name: String,
        #[description = "Quantity for the total price (e.g., 12). Defaults to 1."] quantity: Option<
            f64,
        >,
    ) -> Result<()> {
        let author_id_str = ctx.author().id.to_string();
        let qty_value = quantity.unwrap_or(1.0);

        // Validate inputs
        if name.trim().is_empty() {
            ctx.say("❌ Product name cannot be empty.").await?;
            return Ok(());
        }
        if total_price.is_nan() || total_price.is_infinite() {
            ctx.say("❌ Invalid total price: must be a valid number")
                .await?;
            return Ok(());
        }
        if total_price < 0.0 {
            ctx.say("❌ Total price cannot be negative.").await?;
            return Ok(());
        }
        if qty_value.is_nan() || qty_value.is_infinite() {
            ctx.say("❌ Invalid quantity: must be a valid number")
                .await?;
            return Ok(());
        }
        if qty_value <= 0.0 {
            ctx.say("❌ Quantity must be a positive number.").await?;
            return Ok(());
        }

        let unit_price = total_price / qty_value;
        let db = &ctx.data().database;

        // Find the envelope by name (try user-specific first, then shared)
        let envelope = if let Some(env) =
            envelope::get_envelope_by_name_and_user(db, &envelope_name, &author_id_str).await?
        {
            Some(env)
        } else {
            envelope::get_envelope_by_name(db, &envelope_name).await?
        };

        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "❌ Could not find an envelope named '{envelope_name}' that you can use.",
            ))
            .await?;
            return Ok(());
        };

        // Create the product
        match product::create_product(db, name.clone(), unit_price, envelope.id).await {
            Ok(_) => {
                let message = quantity.map_or_else(
                    || {
                        format!(
                            "✅ Product '{name}' added with unit price **${unit_price:.2}** and linked to envelope '{}'.",
                            envelope.name
                        )
                    },
                    |qty| {
                        format!(
                            "✅ Product '{name}' added with unit price **${unit_price:.2}** (calculated from ${total_price:.2} for {qty:.1} items) and linked to envelope '{}'.",
                            envelope.name
                        )
                    },
                );

                ctx.say(&message).await?;
            }
            Err(e @ Error::Database(_)) => {
                let err_msg = format!("{e:?}");
                if err_msg.contains("UNIQUE") || err_msg.contains("unique") {
                    ctx.say(&format!(
                        "⚠️ A product named '{name}' already exists. Product names must be unique.",
                    ))
                    .await?;
                } else {
                    ctx.say(&format!(
                        "❌ Failed to add product '{name}'. Please try again later.",
                    ))
                    .await?;
                    return Err(e);
                }
            }
            Err(e) => {
                ctx.say(&format!(
                    "❌ Failed to add product '{name}'. Please try again later.",
                ))
                .await?;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Lists all defined products with their unit prices and linked envelopes.
    #[poise::command(slash_command, rename = "list")]
    pub async fn product_list(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        let db = &ctx.data().database;

        let products = product::get_all_active_products(db).await?;

        if products.is_empty() {
            ctx.say("No products have been defined yet. Use `/product_manage add` to create some!")
                .await?;
            return Ok(());
        }

        // Fetch envelope names for each product
        let mut embed_fields = Vec::new();
        for prod in products {
            let envelope_name =
                if let Ok(Some(env)) = envelope::get_envelope_by_id(db, prod.envelope_id).await {
                    env.name
                } else {
                    "Unknown Envelope".to_string()
                };

            let field_name = format!("{} (${:.2})", prod.name, prod.price);
            let field_value = format!("Linked to: {envelope_name}");
            embed_fields.push((field_name, field_value, false));
        }

        let list_embed = serenity::CreateEmbed::default()
            .title("**Product List**")
            .color(0x0058_65F2) // Discord purple
            .fields(embed_fields);

        ctx.send(poise::CreateReply::default().embed(list_embed))
            .await?;
        Ok(())
    }

    /// Updates the unit price of an existing product.
    ///
    /// The new unit price is calculated from the provided `total_price` and
    /// `quantity` (defaults to 1 if omitted).
    #[poise::command(slash_command, rename = "update")]
    pub async fn product_update(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the product to update"]
        #[autocomplete = "autocomplete::autocomplete_product_name"]
        name: String,
        #[description = "New total price paid (e.g., 10.00)"] total_price: f64,
        #[description = "Quantity purchased (e.g., 3). Defaults to 1 if omitted."] quantity: Option<
            f64,
        >,
    ) -> Result<()> {
        let qty_value = quantity.unwrap_or(1.0);

        if total_price.is_nan() || total_price.is_infinite() {
            ctx.say("❌ Invalid total price: must be a valid number")
                .await?;
            return Ok(());
        }
        if total_price < 0.0 {
            ctx.say("❌ Total price cannot be negative.").await?;
            return Ok(());
        }
        if qty_value.is_nan() || qty_value.is_infinite() {
            ctx.say("❌ Invalid quantity: must be a valid number")
                .await?;
            return Ok(());
        }
        if qty_value <= 0.0 {
            ctx.say("❌ Quantity must be a positive number.").await?;
            return Ok(());
        }

        let unit_price = total_price / qty_value;
        let db = &ctx.data().database;

        // Find the product by name
        let Some(product) = product::get_product_by_name(db, &name).await? else {
            ctx.say(&format!("❌ Product '{name}' not found.")).await?;
            return Ok(());
        };

        // Update the product's price (keeping the same name)
        match product::update_product(db, product.id, product.name.clone(), unit_price).await {
            Ok(_) => {
                let message = quantity.map_or_else(
                    || format!("✅ Price for product '{name}' updated to **${unit_price:.2} per item**."),
                    |qty| {
                        format!(
                            "✅ Price for product '{name}' updated to **${unit_price:.2} per item** (calculated from {qty:.1} items for ${total_price:.2}).",
                        )
                    },
                );

                ctx.say(&message).await?;
            }
            Err(e) => {
                ctx.say(&format!("❌ Failed to update price for product '{name}'."))
                    .await?;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Deletes a product from the system by its name.
    #[poise::command(slash_command, rename = "delete")]
    pub async fn product_delete(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the product to delete"]
        #[autocomplete = "autocomplete::autocomplete_product_name"]
        name: String,
    ) -> Result<()> {
        let db = &ctx.data().database;

        // Find the product by name
        let Some(product) = product::get_product_by_name(db, &name).await? else {
            ctx.say(&format!("❌ Product '{name}' not found.")).await?;
            return Ok(());
        };

        // Delete the product
        match product::delete_product(db, product.id).await {
            Ok(_) => {
                ctx.say(&format!("✅ Product '{name}' has been deleted."))
                    .await?;
            }
            Err(e) => {
                ctx.say(&format!("❌ Failed to delete product '{name}'."))
                    .await?;
                return Err(e);
            }
        }

        Ok(())
    }

    /// Records an expense by using a predefined product.
    ///
    /// This command deducts the total cost (unit price * quantity) of the specified
    /// product from the appropriate envelope. If a user is specified, the expense
    /// is recorded for that user (useful for one partner recording expenses for the other).
    #[poise::command(slash_command)]
    pub async fn use_product(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the product to use"]
        #[autocomplete = "autocomplete::autocomplete_product_name"]
        name: String,
        #[description = "Quantity of the product (defaults to 1)"] quantity: Option<i64>,
        #[description = "Optional: user ID to record the expense for"] user: Option<String>,
    ) -> Result<()> {
        let author_id = ctx.author().id.to_string();
        let target_user_id = user.as_ref().unwrap_or(&author_id);
        let quantity = quantity.unwrap_or(1);

        if quantity <= 0 {
            ctx.say("❌ Quantity must be a positive number.").await?;
            return Ok(());
        }

        let db = &ctx.data().database;

        // 1. Fetch the product
        let Some(prod) = product::get_product_by_name(db, &name).await? else {
            ctx.say(&format!("❌ Product '{name}' not found.")).await?;
            return Ok(());
        };

        // 2. Determine the target envelope for spending
        let target_envelope = resolve_product_envelope(ctx, db, &prod, target_user_id).await?;

        // 3. Calculate cost and warn about overdraft if needed
        // Cast is safe: for quantities < 2^53, no precision loss occurs in f64
        #[allow(clippy::cast_precision_loss)]
        let total_cost = prod.price * (quantity as f64);
        check_and_warn_overdraft(ctx, &target_envelope, total_cost).await?;

        // 4. Create the transaction
        let transaction_description = if user.is_some() {
            format!(
                "Product: {} (x{}) - recorded by {}",
                prod.name, quantity, author_id
            )
        } else {
            format!("Product: {} (x{})", prod.name, quantity)
        };
        transaction::create_transaction(
            db,
            target_envelope.id,
            -total_cost,
            transaction_description,
            target_user_id.to_string(),
            Some(ctx.id().to_string()),
            "use_product".to_string(),
        )
        .await?;

        // 5. Send confirmation with mini-report
        send_product_usage_report(ctx, db, &prod, quantity, total_cost, target_envelope.id).await?;

        Ok(())
    }

    /// Resolves which envelope to use for a product (handles individual vs shared envelopes).
    async fn resolve_product_envelope(
        ctx: poise::Context<'_, BotData, Error>,
        db: &sea_orm::DatabaseConnection,
        prod: &crate::entities::product::Model,
        author_id: &str,
    ) -> Result<crate::entities::envelope::Model> {
        let Some(template_envelope) = envelope::get_envelope_by_id(db, prod.envelope_id).await?
        else {
            ctx.say(&format!(
                "❌ Error: The envelope linked to product '{}' is missing or inactive.",
                prod.name
            ))
            .await?;
            return Err(Error::EnvelopeNotFound {
                name: format!("ID {}", prod.envelope_id),
            });
        };

        if template_envelope.is_individual {
            resolve_individual_envelope(ctx, db, &template_envelope, &prod.name, author_id).await
        } else {
            Ok(template_envelope)
        }
    }

    /// Resolves the user's individual envelope instance.
    async fn resolve_individual_envelope(
        ctx: poise::Context<'_, BotData, Error>,
        db: &sea_orm::DatabaseConnection,
        template_envelope: &crate::entities::envelope::Model,
        product_name: &str,
        author_id: &str,
    ) -> Result<crate::entities::envelope::Model> {
        if let Some(user_env) =
            envelope::get_envelope_by_name_and_user(db, &template_envelope.name, author_id).await?
        {
            if user_env.user_id.as_deref() == Some(author_id) {
                return Ok(user_env);
            }
        }

        ctx.say(&format!(
            "❌ You don't have an individual envelope named '{}' to use with product '{}'.",
            template_envelope.name, product_name
        ))
        .await?;

        Err(Error::EnvelopeNotFound {
            name: template_envelope.name.clone(),
        })
    }

    /// Checks if envelope has sufficient funds and warns the user if not.
    async fn check_and_warn_overdraft(
        ctx: poise::Context<'_, BotData, Error>,
        envelope: &crate::entities::envelope::Model,
        total_cost: f64,
    ) -> Result<()> {
        if envelope.balance < total_cost {
            ctx.say(&format!(
                "⚠️ Warning: Envelope '{}' has insufficient funds (${:.2}). Spending ${:.2} will overdraft it.",
                envelope.name, envelope.balance, total_cost
            ))
            .await?;
        }
        Ok(())
    }

    /// Sends a mini-report embed showing the product usage result.
    async fn send_product_usage_report(
        ctx: poise::Context<'_, BotData, Error>,
        db: &sea_orm::DatabaseConnection,
        prod: &crate::entities::product::Model,
        quantity: i64,
        total_cost: f64,
        envelope_id: i64,
    ) -> Result<()> {
        let final_envelope = envelope::get_envelope_by_id(db, envelope_id)
            .await?
            .ok_or_else(|| Error::EnvelopeNotFound {
                name: format!("ID {envelope_id}"),
            })?;

        let progress_bar = generate_progress_bar(final_envelope.balance, final_envelope.allocation);
        let balance_emoji = if final_envelope.balance >= 0.0 {
            "💰"
        } else {
            "⚠️"
        };

        let embed = serenity::CreateEmbed::default()
            .title(format!("Used Product: {}", prod.name))
            .color(0x0058_65F2)
            .field("Envelope", format!("**{}**", final_envelope.name), false)
            .field(
                "Cost",
                format!("${:.2} × {} = ${:.2}", prod.price, quantity, total_cost),
                true,
            )
            .field(
                format!("{balance_emoji} Balance"),
                format!(
                    "${:.2} / ${:.2}",
                    final_envelope.balance, final_envelope.allocation
                ),
                true,
            )
            .field("Progress", progress_bar, false);

        ctx.send(poise::CreateReply::default().embed(embed)).await?;
        Ok(())
    }

    /// Generates a progress bar string for Discord display
    fn generate_progress_bar(balance: f64, allocation: f64) -> String {
        if allocation <= 0.0 {
            return "[No allocation]".to_string();
        }

        let percentage = (balance / allocation * 100.0).clamp(0.0, 100.0);
        // Cast safety: percentage ∈ [0, 100], divided by 10 and rounded gives [0, 10],
        // then clamped again to ensure it's in valid range. Truncation and sign loss are intentional.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let filled = (percentage / 10.0).round().clamp(0.0, 10.0) as usize;
        let empty = 10_usize.saturating_sub(filled);

        let bar = "█".repeat(filled) + &"░".repeat(empty);
        format!("[{bar}] {percentage:.1}%")
    }
}

// Re-export all commands
pub use inner::*;
