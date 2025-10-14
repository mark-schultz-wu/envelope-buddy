//! Autocomplete handlers for Discord slash command parameters.
//!
//! This module provides autocomplete functionality for command parameters like
//! envelope names and product names, improving the user experience by suggesting
//! valid options as the user types.

use crate::{
    bot::BotData,
    core::{envelope, product},
    errors::Error,
};

/// Provides autocomplete suggestions for envelope names.
///
/// This function queries the database for active envelopes that match the user's
/// partial input and returns up to 25 matching envelope names. It searches both
/// shared envelopes and user-specific individual envelopes.
///
/// # Arguments
/// * `ctx` - The poise context containing the database connection
/// * `partial` - The partial string the user has typed so far
///
/// # Returns
/// A vector of envelope names that match the partial input
pub async fn autocomplete_envelope_name(
    ctx: poise::Context<'_, BotData, Error>,
    partial: &str,
) -> Vec<String> {
    let db = &ctx.data().database;
    let user_id = ctx.author().id.to_string();

    // Get all active envelopes
    let Ok(envelopes) = envelope::get_all_active_envelopes(db).await else {
        return Vec::new();
    };

    let partial_lower = partial.to_lowercase();

    // Filter envelopes based on:
    // 1. Name matches the partial input (case-insensitive)
    // 2. User can access it (shared OR belongs to user)
    let mut matching: Vec<String> = envelopes
        .into_iter()
        .filter(|env| {
            // Check if name matches
            let name_matches = env.name.to_lowercase().contains(&partial_lower);

            // Check if user can access (shared or their own individual envelope)
            let can_access = !env.is_individual || env.user_id.as_deref() == Some(&user_id);

            name_matches && can_access
        })
        .map(|env| env.name) // Return just the envelope name without suffix
        .take(25) // Discord autocomplete limit
        .collect();

    // Sort alphabetically for consistent UX
    matching.sort();
    matching
}

/// Provides autocomplete suggestions for product names.
///
/// This function queries the database for active products that match the user's
/// partial input and returns up to 25 matching product names with their prices
/// for easy identification.
///
/// # Arguments
/// * `ctx` - The poise context containing the database connection
/// * `partial` - The partial string the user has typed so far
///
/// # Returns
/// A vector of product names (with prices) that match the partial input
pub async fn autocomplete_product_name(
    ctx: poise::Context<'_, BotData, Error>,
    partial: &str,
) -> Vec<String> {
    let db = &ctx.data().database;

    // Get all active products
    let Ok(products) = product::get_all_active_products(db).await else {
        return Vec::new();
    };

    let partial_lower = partial.to_lowercase();

    // Filter products where name matches the partial input
    // Return just the name (not formatted with price) so it matches command parameters exactly
    let mut matching: Vec<String> = products
        .into_iter()
        .filter(|prod| prod.name.to_lowercase().contains(&partial_lower))
        .map(|prod| prod.name)
        .take(25) // Discord autocomplete limit
        .collect();

    // Sort alphabetically for consistent UX
    matching.sort();
    matching
}

/// Provides autocomplete suggestions for category names.
///
/// This function returns common envelope category names to help users
/// organize their envelopes consistently. Categories are predefined common
/// budget categories.
///
/// # Arguments
/// * `_ctx` - The poise context (unused, but required by poise signature)
/// * `partial` - The partial string the user has typed so far
///
/// # Returns
/// A vector of category names that match the partial input
#[must_use]
pub fn autocomplete_category(
    _ctx: poise::Context<'_, BotData, Error>,
    partial: &str,
) -> Vec<String> {
    let categories = [
        "Bills",
        "Entertainment",
        "Food",
        "Healthcare",
        "Housing",
        "Insurance",
        "Personal",
        "Savings",
        "Transportation",
        "Utilities",
        "Other",
    ];

    let partial_lower = partial.to_lowercase();

    categories
        .iter()
        .filter(|cat| cat.to_lowercase().contains(&partial_lower))
        .map(|&cat| cat.to_string())
        .collect()
}
