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
/// This function queries the database for all distinct categories currently in use
/// by active envelopes, allowing users to maintain consistency while also being
/// flexible to add new categories as needed.
///
/// # Arguments
/// * `ctx` - The poise context containing the database connection
/// * `partial` - The partial string the user has typed so far
///
/// # Returns
/// A vector of category names that match the partial input
pub async fn autocomplete_category(
    ctx: poise::Context<'_, BotData, Error>,
    partial: &str,
) -> Vec<String> {
    let db = &ctx.data().database;

    // Get all categories from existing envelopes
    let Ok(categories) = envelope::get_all_categories(db).await else {
        return Vec::new();
    };

    let partial_lower = partial.to_lowercase();

    // Filter categories that match the partial input
    categories
        .into_iter()
        .filter(|cat| cat.to_lowercase().contains(&partial_lower))
        .take(25) // Discord autocomplete limit
        .collect()
}

/// Provides autocomplete suggestions for user nicknames.
///
/// This function shows all configured nicknames that match the partial input.
/// Only nicknames are accepted - no Discord IDs or mentions.
///
/// # Arguments
/// * `ctx` - The poise context
/// * `partial` - The partial string the user has typed so far
///
/// # Returns
/// A vector of nickname strings for autocomplete suggestions
#[allow(clippy::unused_async)]
pub async fn autocomplete_user(
    ctx: poise::Context<'_, BotData, Error>,
    partial: &str,
) -> Vec<String> {
    use crate::config::users;

    let author_id = ctx.author().id.to_string();
    let author_nickname = users::get_nickname(&author_id);

    // Get all configured nicknames
    let all_nicknames = users::get_all_nicknames();
    let partial_lower = partial.to_lowercase();

    let mut suggestions = Vec::new();

    // If no partial input, suggest author's nickname first, then all others
    if partial.is_empty() {
        if let Some(ref nick) = author_nickname {
            suggestions.push(nick.clone());
        }
        suggestions.extend(all_nicknames.into_iter().filter(|nick| {
            Some(nick) != author_nickname.as_ref() // Don't duplicate author's nickname
        }));
        suggestions.sort();
        suggestions.truncate(25);
        return suggestions;
    }

    // Find matching nicknames
    for nickname in all_nicknames {
        if nickname.to_lowercase().contains(&partial_lower) {
            suggestions.push(nickname);
        }
    }

    suggestions.sort();
    suggestions.truncate(25); // Discord autocomplete limit
    suggestions
}
