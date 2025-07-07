use std::sync::Arc;

use crate::bot::Context;
use crate::commands::utils::envelope_name_autocomplete;
use crate::commands::utils::product_name_autocomplete;
use crate::models::Envelope;
use crate::{Error, Result, db};
use poise::serenity_prelude as serenity;
use tracing::{error, info, instrument, warn};

/// Parent command for managing predefined, fixed-price products.
///
/// Products allow for quick logging of expenses by linking a named item to a
/// specific envelope and unit price. This command groups subcommands for adding,
/// listing, updating, and deleting products.
///
/// Running this command by itself displays help text for its subcommands.
///
/// TODO: Think about unified way to handle help text for all commands
#[poise::command(
    slash_command,
    subcommands("product_add", "product_list", "product_update", "product_delete")
)]
pub async fn product_manage(ctx: Context<'_>) -> Result<()> {
    let help_text = "Product management command. Available subcommands:\n\
        `/product add name:<name> total_price:<price> envelope:<envelope_name> [quantity:<num>] [description:<text>]`\n\
        > Defines a new product. The unit price is calculated from `total_price` and `quantity` (quantity defaults to 1 if omitted).\n\
        `/product list`\n\
        > Displays all defined products, their prices, and linked envelopes.\n\
        `/product update name:<name> total_price:<price> [quantity:<num>]`\n\
        > Updates an existing product's unit price using the given `total_price` and `quantity` (quantity defaults to 1 if omitted).\n\
        `/product consume name:<name> [quantity:<num>]`.";

    ctx.say(help_text).await?;
    Ok(())
}

/// Adds a new fixed-price product to the system.
///
/// The unit price of the product is automatically calculated from the `total_price`
/// and `quantity` provided (quantity defaults to 1 if not specified).
/// The product name must be unique. It is linked to a specified envelope
/// from which expenses will be deducted when the product is used.
///
/// # Parameters
///
/// * `name`: Unique name for the product (e.g., '12-Pack Soda').
/// * `total_price`: Total price paid for the given quantity (e.g., 6.99). Cannot be negative.
/// * `envelope_name`: Name of the envelope this product's cost should be debited from. Autocompletes.
/// * `quantity_opt`: Optional quantity for the `total_price` (e.g., 12). Defaults to 1.0 if omitted. Must be positive.
/// * `description`: Optional textual description for the product.
///
/// # Errors
///
/// Returns `Error::Command` if the product name is empty, total price is negative,
/// or quantity is not positive.
/// Returns `Error::Database` or `Error::Command` if the specified `envelope_name`
/// cannot be found or is inaccessible to the user.
/// Returns `Error::Database` if a product with the same name already exists (due to UNIQUE constraint)
/// or if any other database error occurs during insertion.
/// Can also propagate errors from cache refresh operations.
#[poise::command(slash_command, rename = "add")]
#[instrument(skip(ctx))]
pub async fn product_add(
    ctx: Context<'_>,
    #[description = "Unique name for the product (e.g., '12-Pack Soda')"] name: String,
    #[description = "Total price paid for the given quantity (e.g., 6.99)"] total_price: f64,
    #[description = "Envelope this product's cost should come from"]
    #[autocomplete = "envelope_name_autocomplete"]
    envelope_name: String,
    #[description = "Quantity for the total price (e.g., 12). Defaults to 1."] quantity_opt: Option<
        f64,
    >, // Optional quantity, f64 for flexibility
    #[description = "Optional description for the product"] description: Option<String>,
) -> Result<()> {
    let author_id_str = ctx.author().id.to_string();
    let quantity = quantity_opt.unwrap_or(1.0); // Default quantity to 1.0 if not provided

    info!(
        "Product_add command from {}: name='{}', total_price={}, quantity={}, envelope_name='{}', desc={:?}",
        ctx.author().name,
        name,
        total_price,
        quantity,
        envelope_name,
        description
    );

    if name.trim().is_empty() {
        ctx.say("Product name cannot be empty.").await?;
        return Ok(());
    }
    if total_price < 0.0 {
        ctx.say("Total price cannot be negative.").await?;
        return Ok(());
    }
    if quantity <= 0.0 {
        ctx.say("Quantity must be a positive number.").await?;
        return Ok(());
    }

    let unit_price = total_price / quantity;
    // Optional: Round the unit price if desired, e.g., to 2 decimal places
    // let unit_price_rounded = (unit_price * 100.0).round() / 100.0;
    // For now, we'll store the precise f64 value. db::add_product already checks if unit_price < 0.

    let data = ctx.data();
    let db_pool = &data.db_pool;

    // 1. Find the envelope ID for the given envelope_name
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

    // 2. Add the product to the database with the calculated unit_price
    match db::add_product(
        db_pool,
        &name,
        unit_price,
        envelope_to_link.id,
        description.as_deref(),
    )
    .await
    {
        Ok(product_id) => {
            let price_calculation_note = if quantity_opt.is_some() && quantity != 1.0 {
                format!(
                    " (calculated from ${:.2} for {} items)",
                    total_price, quantity
                )
            } else {
                String::new()
            };

            info!(
                "Product '{}' (ID: {}) added successfully, unit price ${:.2}{}, linked to envelope '{}' (ID: {}).",
                name,
                product_id,
                unit_price,
                price_calculation_note,
                envelope_to_link.name,
                envelope_to_link.id
            );
            if let Err(e) =
                crate::cache::refresh_product_names_cache(db_pool, &data.product_names_cache).await
            {
                error!(
                    "Failed to refresh product names cache after adding product '{}': {}",
                    name, e
                );
            }
            ctx.say(format!(
                "✅ Product '{}' added with unit price **${:.2}**{} and linked to envelope '{}'.",
                name, unit_price, price_calculation_note, envelope_to_link.name
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

/// Lists all defined products, their unit prices, linked envelopes, and descriptions.
///
/// The information is presented in a Discord embed. If no products are defined,
/// a message indicating this will be shown.
///
/// # Errors
///
/// Returns `Error::Database` if fetching products from the database fails.
/// Returns `Error::FrameworkError` if sending the Discord reply fails.
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

/// Records an expense by using a predefined product.
///
/// This command deducts the total cost (unit price * quantity) of the specified
/// product from the appropriate envelope. If the product is linked to an individual
/// envelope type, the cost is deducted from the command issuer's instance of that
/// envelope. If linked to a shared envelope, the shared envelope is debited.
/// A mini-report showing the updated envelope status is sent as a reply.
///
/// # Parameters
///
/// * `name`: Name of the product to use. Autocompletes from existing products.
/// * `quantity_opt`: Optional quantity of the product to use. Defaults to 1 if omitted. Must be positive.
///
/// # Errors
///
/// Returns `Error::Command` if the quantity is not positive, if the product is not found,
/// if the linked base envelope is missing/inactive, or if the user does not have the
/// required individual envelope instance.
/// Returns `Error::Database` for issues during database operations (fetching product/envelope,
/// updating balance, creating transaction).
/// Returns `Error::FrameworkError` if sending the Discord reply fails.
#[poise::command(slash_command, rename = "use_product")]
#[instrument(skip(ctx))]
pub async fn use_product(
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
        "'/product {}' command from {} ({}): product_name='{}', quantity={}",
        ctx.command().name, // Gets the actual invoked subcommand name e.g. "consume"
        ctx.author().name,
        author_id_str,
        name,
        quantity
    );

    if quantity <= 0 {
        ctx.say("Quantity must be a positive number.").await?;
        return Ok(());
    }

    let data = ctx.data();
    let db_pool = &data.db_pool;
    let app_config = Arc::clone(&data.app_config); // For nicknames in mini-report

    // 1. Fetch the product
    let product = match db::get_product_by_name(db_pool, &name).await? {
        Some(p) => p,
        None => {
            ctx.say(format!("Product '{}' not found.", name)).await?;
            return Ok(());
        }
    };

    // 2. Fetch the "template" envelope linked in the product definition
    let template_envelope = match db::get_envelope_by_id(db_pool, product.envelope_id).await? {
        Some(env) => {
            // get_envelope_by_id already checks for is_deleted = FALSE
            env
        }
        None => {
            error!(
                "Data Integrity Issue: Product '{}' (ID: {}) links to a non-existent or deleted envelope_id: {}.",
                product.name, product.id, product.envelope_id
            );
            ctx.say(format!(
                "Error: The base envelope linked to product '{}' is missing or inactive. Please notify an admin.",
                product.name
            )).await?;
            return Ok(());
        }
    };

    // 3. Determine the actual target envelope for spending
    #[allow(clippy::needless_late_init)]
    let actual_target_envelope: Envelope;

    if template_envelope.is_individual {
        // If the product was defined against an individual envelope instance,
        // find the command issuer's specific instance of an envelope with the SAME NAME.
        info!(
            "Product '{}' is linked to an individual envelope type ('{}'). Attempting to find instance for user {}.",
            product.name, template_envelope.name, author_id_str
        );

        actual_target_envelope = match db::get_user_or_shared_envelope(
            db_pool,
            &template_envelope.name,
            &author_id_str,
        )
        .await?
        {
            Some(user_specific_env) => {
                // Ensure it's truly the user's own, active, individual envelope.
                if !user_specific_env.is_individual
                    || user_specific_env.user_id.as_deref() != Some(&author_id_str)
                {
                    warn!(
                        "User {} does not have a matching individual envelope named '{}' for product '{}', or it's not individual.",
                        author_id_str, template_envelope.name, product.name
                    );
                    ctx.say(format!(
                        "To use product '{}', you need an active individual envelope named '{}'. Yours could not be found or is not setup correctly.",
                        product.name, template_envelope.name
                    )).await?;
                    return Ok(());
                }
                if user_specific_env.is_deleted {
                    // Should be caught by get_user_or_shared_envelope if it filters deleted
                    ctx.say(format!(
                        "Your individual envelope '{}' is currently deleted and cannot be used.",
                        template_envelope.name
                    ))
                    .await?;
                    return Ok(());
                }
                user_specific_env
            }
            None => {
                ctx.say(format!(
                    "You don't seem to have an individual envelope named '{}' to use with product '{}'.",
                    template_envelope.name, product.name
                )).await?;
                return Ok(());
            }
        };
    } else {
        // If the product was linked to a shared envelope, use that directly.
        info!(
            "Product '{}' is linked to shared envelope '{}'.",
            product.name, template_envelope.name
        );
        actual_target_envelope = template_envelope;
    }

    // 4. Calculate cost and perform spend using actual_target_envelope
    let total_cost = product.price * quantity as f64;
    let new_balance = actual_target_envelope.balance - total_cost;

    // Optional: Overdraft check for the actual_target_envelope
    if new_balance < 0.0 {
        // Example: Allow overdraft but warn, or prevent it:
        warn!(
            "Envelope '{}' will be overdrafted. Current: ${:.2}, Spending: ${:.2}, New: ${:.2}",
            actual_target_envelope.name, actual_target_envelope.balance, total_cost, new_balance
        );
        // To prevent overdraft:
        // ctx.say(format!("Spending ${:.2} from '{}' would overdraft it (current balance ${:.2}). Transaction cancelled.",
        //                 total_cost, actual_target_envelope.name, actual_target_envelope.balance)).await?;
        // return Ok(());
    }

    db::update_envelope_balance(db_pool, actual_target_envelope.id, new_balance).await?;

    let transaction_description = format!(
        "Product: {} (x{}){}",
        product.name,
        quantity,
        product
            .description
            .as_ref()
            .map_or_else(String::new, |d| format!(" - {}", d))
    );
    let discord_interaction_id = ctx.id().to_string();

    db::create_transaction(
        db_pool,
        actual_target_envelope.id,
        total_cost, // The amount spent (positive)
        &transaction_description,
        &author_id_str,
        Some(&discord_interaction_id),
        "spend", // Transaction type
    )
    .await?;

    // 5. Reply with confirmation and mini-report
    // Fetch the *very latest* state of the envelope for the report, as balance has changed.
    let final_envelope_state_for_report = match db::get_envelope_by_id(
        db_pool,
        actual_target_envelope.id,
    )
    .await?
    {
        Some(env) => env,
        None => {
            error!(
                "Critical error: Failed to re-fetch envelope {} for mini-report after using product.",
                actual_target_envelope.name
            );
            // Send a simpler confirmation if the full report can't be generated
            ctx.say(format!(
                "Used product '{}' (x{}). Total cost: ${:.2}. (Mini-report generation failed).",
                product.name, quantity, total_cost
            ))
            .await?;
            return Err(Error::Command(format!(
                "Failed to re-fetch envelope {} for mini-report.",
                actual_target_envelope.name
            )));
        }
    };

    // Call the new utility function
    crate::commands::utils::send_mini_report_embed(
        ctx,
        &final_envelope_state_for_report,
        &app_config,
        db_pool,
    )
    .await?;

    Ok(())
}

/// Updates the unit price of an existing product.
///
/// The new unit price is calculated from the provided `total_price` and an
/// optional `quantity` (which defaults to 1 if omitted).
///
/// # Parameters
///
/// * `name`: Name of the product to update. Autocompletes from existing products.
/// * `total_price`: New total price paid (e.g., 10.00). This becomes the unit price if quantity is 1 or omitted. Cannot be negative.
/// * `quantity_opt`: Optional new quantity purchased (e.g., 3). Defaults to 1.0 if omitted. Must be positive.
///
/// # Errors
///
/// Returns `Error::Command` if the quantity is not positive or total price is negative.
/// Returns `Error::Database` if the product is not found or if the database update fails.
/// Can also propagate errors from cache refresh operations.
/// Returns `Error::FrameworkError` if sending the Discord reply fails.
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
            // Don't strictly have to refresh the cache here as the name stays the same
            if let Err(e) =
                crate::cache::refresh_product_names_cache(db_pool, &ctx.data().product_names_cache)
                    .await
            {
                error!(
                    "Failed to refresh product names cache after updating product '{}': {}",
                    name, e
                );
            }

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

/// Deletes a product from the system by its name.
///
/// # Parameters
///
/// * `name`: Name of the product to delete. Autocompletes from existing products.
///
/// # Errors
///
/// Returns `Error::Database` if the database deletion fails.
/// Can also propagate errors from cache refresh operations.
/// Returns `Error::FrameworkError` if sending the Discord reply fails (e.g., if the product
/// was not found and a "not found" message is sent).
#[poise::command(slash_command, rename = "delete")]
#[instrument(skip(ctx))]
pub async fn product_delete(
    ctx: Context<'_>,
    #[description = "Name of the product to delete"]
    #[autocomplete = "product_name_autocomplete"] // Use autocomplete to find the product
    name: String,
) -> Result<()> {
    info!(
        "Product_delete command from {}: product_name='{}'",
        ctx.author().name,
        name
    );
    let data = ctx.data();
    let db_pool = &data.db_pool;

    match db::delete_product_by_name(db_pool, &name).await {
        Ok(rows_affected) if rows_affected > 0 => {
            info!("Successfully deleted product '{}'", name);

            if let Err(e) =
                crate::cache::refresh_product_names_cache(db_pool, &data.product_names_cache).await
            {
                error!(
                    "Failed to refresh product names cache after deleting product '{}': {}",
                    name, e
                );
            }

            ctx.say(format!("✅ Product '{}' has been deleted.", name))
                .await?;
        }
        Ok(_) => {
            // 0 rows affected means product wasn't found
            ctx.say(format!("Product '{}' not found. Nothing to delete.", name))
                .await?;
        }
        Err(e) => {
            error!("Failed to delete product '{}': {:?}", name, e);
            ctx.say(format!("❌ Failed to delete product '{}'.", name))
                .await?;
            return Err(e);
        }
    }
    Ok(())
}
