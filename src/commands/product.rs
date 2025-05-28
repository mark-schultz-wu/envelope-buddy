use std::sync::Arc;

use crate::bot::Context;
use crate::commands::utils::{
    envelope_name_autocomplete, generate_single_envelope_report_field_data,
    get_current_month_date_info,
};
use crate::{Error, Result, db};
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::AutocompleteChoice;
use tracing::{error, info, instrument, trace, warn};
// Autocomplete function for existing product names
async fn product_name_autocomplete(ctx: Context<'_>, partial: &str) -> Vec<AutocompleteChoice> {
    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request for product_name");
    let db_pool = &ctx.data().db_pool;

    match db::suggest_product_names(db_pool, partial).await {
        Ok(names) => names
            .into_iter()
            .map(|name_str| AutocompleteChoice::new(name_str.clone(), name_str))
            .collect(),
        Err(e) => {
            error!(
                "Autocomplete: Failed to fetch product name suggestions: {:?}",
                e
            );
            Vec::new()
        }
    }
}

/// Parent command for managing products. Use subcommands like add, list, update, use.
#[poise::command(
    slash_command,
    subcommands("product_add", "product_list", "product_update", "product_use")
)]
pub async fn product(ctx: Context<'_>) -> Result<()> {
    // This body is called if the user types just /product
    // You can provide help text for the subcommands here.
    let help_text = "Product management command. Available subcommands:\n\
                     `/product add <name> <price> <envelope> [description]` - Add a new product.\n\
                     `/product list` - View all defined products.\n\
                     `/product update <name> <total_price> [quantity]` - Update a product's price.\n\
                     `/product use <name> [quantity]` - Record an expense using a product.";
    ctx.say(help_text).await?;
    Ok(())
}

/// Adds a new fixed-price product linked to an envelope.
#[poise::command(slash_command, rename = "add")]
#[instrument(skip(ctx))]
pub async fn product_add(
    ctx: Context<'_>,
    #[description = "Unique name for the product (e.g., 'Coffee Shop Latte')"] name: String,
    #[description = "Price of the product (e.g., 4.75)"] price: f64,
    #[description = "Envelope this product's cost should come from"]
    #[autocomplete = "envelope_name_autocomplete"]
    envelope_name: String,
    #[description = "Optional description for the product"] description: Option<String>,
) -> Result<()> {
    let author_id_str = ctx.author().id.to_string(); // For context, not directly used for adding product
    info!(
        "Product_add command from {}: name='{}', price={}, envelope_name='{}', desc={:?}",
        ctx.author().name,
        name,
        price,
        envelope_name,
        description
    );

    if name.trim().is_empty() {
        ctx.say("Product name cannot be empty.").await?;
        return Ok(());
    }
    if price < 0.0 {
        // The db::add_product function also checks this, but early exit is good.
        ctx.say("Product price cannot be negative.").await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;

    // 1. Find the envelope ID for the given envelope_name
    //    This must be an envelope the user can conceptually associate with (shared or their own)
    let envelope_to_link =
        match db::get_user_or_shared_envelope(db_pool, &envelope_name, &author_id_str).await? {
            Some(env) => env,
            None => {
                ctx.say(format!(
                    "Could not find an envelope named '{}' that you can use to link this product.",
                    envelope_name
                ))
                .await?;
                return Ok(());
            }
        };
    // Here, we might want to consider if *any* user can add a product linked to any shared envelope,
    // or if only specific users can link to their individual envelopes.
    // For now, get_user_or_shared_envelope ensures it's accessible to the user.

    // 2. Add the product to the database
    match db::add_product(
        db_pool,
        &name,
        price,
        envelope_to_link.id,
        description.as_deref(),
    )
    .await
    {
        Ok(product_id) => {
            info!(
                "Product '{}' (ID: {}) added successfully, linked to envelope '{}' (ID: {}).",
                name, product_id, envelope_to_link.name, envelope_to_link.id
            );
            ctx.say(format!(
                "✅ Product '{}' added with price ${:.2}, linked to envelope '{}'.",
                name, price, envelope_to_link.name
            ))
            .await?;
        }
        Err(Error::Database(db_err_msg))
            if db_err_msg.contains("UNIQUE constraint failed: products.name") =>
        {
            warn!("Attempt to add product with duplicate name: {}", name);
            ctx.say(format!(
                "⚠️ A product named '{}' already exists. Product names must be unique.",
                name
            ))
            .await?;
        }
        Err(e) => {
            error!("Failed to add product '{}': {:?}", name, e);
            ctx.say(format!(
                "❌ Failed to add product '{}'. Please try again later.",
                name
            ))
            .await?;
            return Err(e); // Propagate other errors
        }
    }

    Ok(())
}

/// Lists all available products.
#[poise::command(slash_command, rename = "list")]
#[instrument(skip(ctx))]
pub async fn product_list(ctx: Context<'_>) -> Result<()> {
    info!(
        "Products_list command received from user: {}",
        ctx.author().name
    );
    let db_pool = &ctx.data().db_pool;

    let products = db::list_all_products(db_pool).await?;

    if products.is_empty() {
        ctx.say("No products have been defined yet. Use `/product add` to create some!")
            .await?;
        return Ok(());
    }

    let mut embed_fields = Vec::new();
    for product in products {
        let field_name = format!("{} (${:.2})", product.name, product.price);
        let field_value = format!(
            "Links to: {}\n{}", // Envelope name is already joined in list_all_products
            product
                .envelope_name
                .as_deref()
                .unwrap_or("Unknown Envelope"),
            product.description.as_deref().unwrap_or("No description.")
        );
        embed_fields.push((field_name, field_value, false)); // false for not inline
    }

    let list_embed = serenity::CreateEmbed::default()
        .title("**Product List**")
        .color(0x5865F2) // Discord purple
        .fields(embed_fields);

    ctx.send(poise::CreateReply::default().embed(list_embed))
        .await?;
    Ok(())
}

/// Uses a predefined product to record an expense.
#[poise::command(slash_command, rename = "use")]
#[instrument(skip(ctx))]
pub async fn product_use(
    // Function name can be different from command name with `rename`
    ctx: Context<'_>,
    #[description = "Name of the product to use"]
    #[autocomplete = "product_name_autocomplete"]
    name: String,
    #[description = "Quantity of the product (defaults to 1)"] quantity_opt: Option<i64>, // Use i64 for quantity, can be f64 if fractional quantities make sense
) -> Result<()> {
    let author_id_str = ctx.author().id.to_string();
    let quantity = quantity_opt.unwrap_or(1); // Default quantity to 1
    info!(
        "UseProduct command from {}: product_name='{}', quantity={}",
        ctx.author().name,
        name,
        quantity
    );

    if quantity <= 0 {
        ctx.say("Quantity must be a positive number.").await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;
    let app_config = Arc::clone(&data.app_config);

    // 1. Fetch the product
    let product = match db::get_product_by_name(db_pool, &name).await? {
        Some(p) => p,
        None => {
            ctx.say(format!("Product '{}' not found.", name)).await?;
            return Ok(());
        }
    };

    // 2. Fetch the linked envelope
    let linked_envelope = match db::get_envelope_by_id(db_pool, product.envelope_id).await? {
        Some(env) => {
            if env.is_deleted {
                // Should not happen if get_envelope_by_id filters active
                ctx.say(format!(
                    "The envelope '{}' linked to product '{}' is deleted.",
                    env.name, product.name
                ))
                .await?;
                return Ok(());
            }
            env
        }
        None => {
            error!(
                "Product '{}' (ID: {}) links to a non-existent envelope_id: {}. This indicates a data integrity issue.",
                product.name, product.id, product.envelope_id
            );
            ctx.say(format!("Error: The envelope linked to product '{}' could not be found. Please notify an admin.", product.name)).await?;
            return Ok(());
        }
    };

    // 3. Verify user access to the envelope
    if linked_envelope.is_individual && linked_envelope.user_id.as_deref() != Some(&author_id_str) {
        warn!(
            "User {} tried to use product '{}' linked to {}'s envelope '{}'. Denied.",
            author_id_str,
            product.name,
            linked_envelope.user_id.as_deref().unwrap_or("unknown"),
            linked_envelope.name
        );
        ctx.say(format!(
            "You cannot use product '{}' as its linked envelope ('{}') does not belong to you.",
            product.name, linked_envelope.name
        ))
        .await?;
        return Ok(());
    }

    // 4. Calculate cost and perform spend
    let total_cost = product.price * quantity as f64;
    let new_balance = linked_envelope.balance - total_cost;

    // (Optional: Overdraft check for the envelope)
    // if new_balance < 0.0 { ... }

    db::update_envelope_balance(db_pool, linked_envelope.id, new_balance).await?;

    let transaction_description = format!(
        "Product: {} (x{}){}",
        product.name,
        quantity,
        product
            .description
            .as_ref()
            .map_or("".to_string(), |d| format!(" - {}", d))
    );
    let discord_interaction_id = ctx.id().to_string();

    db::create_transaction(
        db_pool,
        linked_envelope.id,
        total_cost, // The amount spent
        &transaction_description,
        &author_id_str,
        Some(&discord_interaction_id),
        "spend", // Transaction type
    )
    .await?;

    // 5. Reply with confirmation and mini-report
    let updated_envelope_for_report = db::get_envelope_by_id(db_pool, linked_envelope.id)
        .await?
        .ok_or_else(|| {
            Error::Command(format!(
                "Failed to fetch updated envelope {} for mini-report after using product.",
                linked_envelope.name
            ))
        })?;

    let (_now_local_date, current_day_of_month, days_in_month, year, month) =
        get_current_month_date_info();
    let (field_name, field_value) = generate_single_envelope_report_field_data(
        &updated_envelope_for_report,
        &app_config,
        db_pool,
        current_day_of_month,
        days_in_month,
        year,
        month,
    )
    .await?;

    let mini_report_embed = serenity::CreateEmbed::default()
        .title(format!("Update for: {}", field_name))
        .description(field_value)
        .color(0xF1C40F); // Yellow for spend

    let confirmation_text = format!(
        "Used product '{}' (x{}) from envelope '{}'. Total cost: ${:.2}.\nNew balance for '{}': ${:.2}.",
        product.name, quantity, linked_envelope.name, total_cost, linked_envelope.name, new_balance
    );

    ctx.send(
        poise::CreateReply::default()
            .content(confirmation_text) // Keeping this for clarity of action
            .embed(mini_report_embed)
            .ephemeral(false),
    )
    .await?;

    Ok(())
}

/// Updates a product's price, optionally by calculating from a total and quantity.
#[poise::command(slash_command, rename = "update")]
#[instrument(skip(ctx))]
pub async fn product_update(
    ctx: Context<'_>,
    #[description = "Name of the product to update"]
    #[autocomplete = "product_name_autocomplete"]
    name: String,
    #[description = "Total price paid (e.g., 10.00). This becomes the unit price if quantity is 1 or omitted."]
    total_price: f64,
    #[description = "Quantity purchased (e.g., 3). Defaults to 1 if omitted."] quantity_opt: Option<
        f64,
    >,
) -> Result<()> {
    let quantity = quantity_opt.unwrap_or(1.0); // Default quantity to 1.0 if not provided

    info!(
        "Product_update command from {}: product_name='{}', total_price={}, effective_quantity={}",
        ctx.author().name,
        name,
        total_price,
        quantity
    );

    if quantity <= 0.0 {
        ctx.say("Quantity, if provided, must be a positive number.")
            .await?;
        return Ok(());
    }
    if total_price < 0.0 {
        ctx.say("Total price cannot be negative.").await?;
        return Ok(());
    }

    let unit_price = total_price / quantity;
    // Optional: Round the unit price if you want to store consistently (e.g., 2 decimal places)
    // let unit_price_rounded = (unit_price * 100.0).round() / 100.0;
    // Let's use unit_price directly for now, database stores f64.

    let db_pool = &ctx.data().db_pool;

    // 1. Fetch the product to get its ID
    let product = match db::get_product_by_name(db_pool, &name).await? {
        Some(p) => p,
        None => {
            ctx.say(format!("Product '{}' not found.", name)).await?;
            return Ok(());
        }
    };

    // 2. Update the product's price using its ID and the calculated unit price
    match db::update_product_price(db_pool, product.id, unit_price).await {
        Ok(rows_affected) if rows_affected > 0 => {
            let price_calculation_note = if quantity_opt.is_some() && quantity != 1.0 {
                format!(
                    " (calculated from {} items for ${:.2})",
                    quantity, total_price
                )
            } else {
                String::new() // No note if quantity was 1 or defaulted to 1
            };

            info!(
                "Price updated for product '{}' (ID: {}) to ${:.2}{}",
                name, product.id, unit_price, price_calculation_note
            );
            ctx.say(format!(
                "✅ Price for product '{}' updated to **${:.2} per item**{}.",
                name, unit_price, price_calculation_note
            ))
            .await?;
        }
        Ok(_) => {
            // Should not happen if product was found, but handle 0 rows affected
            warn!(
                "Product '{}' was found but price update affected 0 rows.",
                name
            );
            ctx.say(format!(
                "Could not update price for '{}'. Product might have been deleted just now.",
                name
            ))
            .await?;
        }
        Err(Error::Command(msg)) if msg.contains("Product price cannot be negative") => {
            // This error is now caught before DB call if unit_price became negative due to total_price < 0
            // But db::update_product_price also has this check, which is good.
            ctx.say(msg).await?;
        }
        Err(e) => {
            error!("Failed to update price for product '{}': {:?}", name, e);
            ctx.say(format!("❌ Failed to update price for product '{}'.", name))
                .await?;
            return Err(e);
        }
    }
    Ok(())
}
