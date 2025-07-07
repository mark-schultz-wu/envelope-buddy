use crate::bot::{Context, Error};
use crate::config::AppConfig;
use crate::db::{self, DbPool};
use crate::models::Envelope;
use chrono::{Datelike, Local, NaiveDate};
use poise::serenity_prelude::{self, AutocompleteChoice};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, trace};

/// Returns current date information relevant for monthly budget tracking.
///
/// # Returns
///
/// A tuple containing:
/// * `NaiveDate`: The current local date.
/// * `f64`: The current day of the month as a float.
/// * `f64`: The total number of days in the current month as a float.
/// * `i32`: The current year.
/// * `u32`: The current month (1-12).
// Assuming this function doesn't error out due to date logic being robust.
// If it could, an # Errors section would be needed.
pub(crate) fn get_current_month_date_info() -> (NaiveDate, f64, f64, i32, u32) {
    let now_local_date = Local::now().date_naive();
    let current_day_of_month = now_local_date.day() as f64;
    let year = now_local_date.year();
    let month = now_local_date.month();
    let days_in_month = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .pred_opt()
    .unwrap()
    .day() as f64;
    (
        now_local_date,
        current_day_of_month,
        days_in_month,
        year,
        month,
    )
}

/// Generates the field name and value for a single envelope in a report embed.
///
/// This includes the envelope's name, user indicator (shared or owner's nickname),
/// balance, allocation, actual spending for the month, expected spending pace,
/// and a status emoji based on spending progress.
///
/// # Parameters
///
/// * `envelope`: The `Envelope` struct to generate report data for.
/// * `app_config`: The application configuration, used for user nicknames.
/// * `db_pool`: The database connection pool, used to fetch actual monthly spending.
/// * `current_day_of_month`: The current day of the month.
/// * `days_in_month`: The total number of days in the current month.
/// * `year`: The current year.
/// * `month`: The current month.
///
/// # Returns
///
/// Returns `Ok((String, String))` where the first string is the field name
/// (e.g., "Groceries (Shared)") and the second string is the formatted field value
/// containing all the details.
///
/// # Errors
///
/// Returns `Error::Database` if fetching actual monthly spending fails.
/// May also propagate errors from database interactions within helper functions.
pub(crate) async fn generate_single_envelope_report_field_data(
    envelope: &Envelope,
    app_config: &Arc<AppConfig>, // Pass Arc<AppConfig>
    db_pool: &DbPool,            // Pass DbPool
    current_day_of_month: f64,
    days_in_month: f64,
    year: i32,
    month: u32,
) -> Result<(String, String), Error> {
    // Returns (field_name, field_value)
    let user_indicator = if let Some(uid) = &envelope.user_id {
        if uid == &app_config.user_id_1 {
            format!("({})", app_config.user_nickname_1)
        } else if uid == &app_config.user_id_2 {
            format!("({})", app_config.user_nickname_2)
        } else {
            format!("(ID: ...{})", &uid[uid.len().saturating_sub(4)..])
        }
    } else {
        "(Shared)".to_string()
    };

    let actual_monthly_spending =
        db::get_actual_spending_this_month(db_pool, envelope.id, year, month).await?;
    let daily_allocation = if envelope.allocation > 0.0 {
        envelope.allocation / days_in_month
    } else {
        0.0
    };
    let expected_spending_to_date = daily_allocation * current_day_of_month;

    let status_emoji = if envelope.allocation <= 0.0 {
        "⚪"
    } else if actual_monthly_spending <= expected_spending_to_date * 0.90 {
        "🟢"
    } else if actual_monthly_spending > expected_spending_to_date * 1.10 {
        "🔴"
    } else {
        "🟡"
    };

    let field_name = format!("{} {}", envelope.name, user_indicator);
    let field_value = format!(
        "Balance: ${:.2} / Alloc: ${:.2}\nSpent (Actual): ${:.2}\nExpected Pace: ${:.2}\nStatus: {}",
        envelope.balance,
        envelope.allocation,
        actual_monthly_spending,
        expected_spending_to_date,
        status_emoji
    );
    Ok((field_name, field_value))
}

/// Provides autocomplete choices for envelope names accessible to the invoking user.
///
/// It searches through a cache of active envelope names (both shared and the user's
/// own individual envelopes) that partially match the user's input.
/// The search is case-insensitive and matches if the envelope name *contains*
/// the partial input. Results are sorted alphabetically.
///
/// # Parameters
///
/// * `ctx`: The command context, used to access shared data (caches, user info).
/// * `partial`: The partial string input by the user for the envelope name.
///
/// # Returns
///
/// A `Vec<AutocompleteChoice>` containing up to 25 matching envelope names.
// This function primarily interacts with a cache and Poise context; direct errors
// it might return (other than from framework/cache access) are less common.
// If cache access could fail in a way that needs documenting as an error, add it.
pub(crate) async fn envelope_name_autocomplete(
    ctx: Context<'_>,
    partial: &str,
) -> Vec<AutocompleteChoice> {
    let function_start_time = Instant::now();

    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request for envelope_name");
    let data = ctx.data();

    let cache_read_start_time = Instant::now();
    let all_envelopes_cache_guard = data.envelope_names_cache.read().await;
    let cache_read_duration = cache_read_start_time.elapsed();

    let processing_start_time = Instant::now();
    let author_id_str = ctx.author().id.to_string();
    let partial_lower = partial.to_lowercase();

    let mut matching_names_sorted: BTreeSet<String> = BTreeSet::new();

    for cached_env in all_envelopes_cache_guard.iter() {
        if cached_env.name.to_lowercase().contains(&partial_lower)
            && (cached_env.user_id.is_none()
                || cached_env.user_id.as_deref() == Some(&author_id_str))
        {
            matching_names_sorted.insert(cached_env.name.clone());
        }
    }

    let choices: Vec<AutocompleteChoice> = matching_names_sorted
        .into_iter()
        .take(25)
        .map(|name_str| AutocompleteChoice::new(name_str.clone(), name_str))
        .collect();
    let processing_duration = processing_start_time.elapsed();
    let total_function_duration = function_start_time.elapsed();

    debug!(
        user = %ctx.author().name,
        partial_input = partial,
        num_choices = choices.len(),
        total_duration_ms = total_function_duration.as_millis(),
        cache_read_duration_ms = cache_read_duration.as_millis(),
        processing_duration_ms = processing_duration.as_millis(),
        "TIMING: Envelope autocomplete"
    );
    choices
}

/// Provides autocomplete suggestions for product names based on partial user input.
///
/// This function queries an in-memory cache of product names.
/// It performs a case-insensitive "contains" search and returns up to 25 matching choices.
///
/// # Parameters
///
/// * `ctx`: The command context, used to access shared data like the product names cache.
/// * `partial`: The partial string input by the user for the product name.
///
/// # Returns
///
/// A `Vec<AutocompleteChoice>` containing product names that match the partial input.
pub(crate) async fn product_name_autocomplete(
    ctx: Context<'_>,
    partial: &str,
) -> Vec<AutocompleteChoice> {
    let function_start_time = Instant::now();

    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request for product_name");
    let data = ctx.data();

    let cache_read_start_time = Instant::now();
    let product_names_cache_guard = data.product_names_cache.read().await;
    let cache_read_duration = cache_read_start_time.elapsed();

    let processing_start_time = Instant::now();
    let partial_lower = partial.to_lowercase();

    let choices: Vec<AutocompleteChoice> = product_names_cache_guard
        .iter()
        .filter(|name| name.to_lowercase().contains(&partial_lower))
        .take(25)
        .map(|name_str| AutocompleteChoice::new(name_str.clone(), name_str.clone()))
        .collect();
    let processing_duration = processing_start_time.elapsed();
    let total_function_duration = function_start_time.elapsed();

    debug!(
        user = %ctx.author().name,
        partial_input = partial,
        num_choices = choices.len(),
        total_duration_ms = total_function_duration.as_millis(),
        cache_read_duration_ms = cache_read_duration.as_millis(),
        processing_duration_ms = processing_duration.as_millis(),
        "TIMING: Product autocomplete"
    );

    choices
}

/// Generates and sends a standardized mini-report embed for a single envelope.
///
/// This function is typically called after an operation (like spend, addfunds, product use)
/// that modifies an envelope. It fetches current date information, generates the
/// report content using `generate_single_envelope_report_field_data`, and sends
/// it as a publicly visible embed in the channel where the command was invoked.
///
/// # Parameters
///
/// * `ctx`: The command `Context`, used for sending the reply and accessing shared data.
/// * `updated_envelope`: A reference to the `Envelope` struct containing the latest
///   data of the envelope to be reported.
/// * `app_config`: An `Arc` reference to the `AppConfig`, required by
///   `generate_single_envelope_report_field_data` for user nicknames.
/// * `db_pool`: A reference to the `DbPool`, required by
///   `generate_single_envelope_report_field_data` for fetching actual monthly spending.
///
/// # Returns
///
/// Returns `Ok(())` if the mini-report embed was successfully generated and sent.
///
/// # Errors
///
/// This function can return an error if:
/// - `generate_single_envelope_report_field_data` fails (e.g., `Error::Database`
///   if fetching monthly spending encounters an issue).
/// - Sending the reply via `ctx.send()` fails (e.g., `Error::FrameworkError`
///   due to Discord API issues or permission problems).
pub(crate) async fn send_mini_report_embed(
    ctx: Context<'_>,
    updated_envelope: &Envelope,
    app_config: &Arc<AppConfig>,
    db_pool: &DbPool,
) -> Result<(), Error> {
    let (_now_local_date, current_day_of_month, days_in_month, year, month) =
        get_current_month_date_info();

    let (field_name, field_value) = generate_single_envelope_report_field_data(
        updated_envelope,
        app_config,
        db_pool,
        current_day_of_month,
        days_in_month,
        year,
        month,
    )
    .await?;

    let mini_report_embed = serenity_prelude::CreateEmbed::default()
        .title(format!("Update for: {}", field_name))
        .description(field_value)
        // TODO: Consider making the color dynamic based on the operation type
        // For example, green for additions, yellow for spends/updates.
        .color(0xF1C40F); // Yellow for spend/update (as an example)

    ctx.send(
        poise::CreateReply::default()
            .embed(mini_report_embed)
            .ephemeral(false), // Typically mini-reports are visible
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, NaiveDate};

    #[test]
    fn test_get_current_month_date_info_basic() {
        let (now_local_date, current_day_f64, days_in_month_f64, year, month) =
            get_current_month_date_info();

        assert_eq!(year, now_local_date.year());
        assert_eq!(month, now_local_date.month());
        assert_eq!(current_day_f64, now_local_date.day() as f64);

        // Test days_in_month for a known month (e.g., if current month is March 2024 -> 31 days)
        // This part is tricky because it depends on the actual current date when tests run.
        // A more robust test would mock Local::now() or test with fixed dates.
        // For now, let's just check it's a reasonable number.
        assert!((28.0..=31.0).contains(&days_in_month_f64));

        // Example for a fixed date (how you might test days_in_month more robustly)
        let test_date = NaiveDate::from_ymd_opt(2024, 2, 15).unwrap(); // Feb 2024 (leap) -> 29 days
        let days_in_feb_2024 = if test_date.month() == 12 {
            NaiveDate::from_ymd_opt(test_date.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(test_date.year(), test_date.month() + 1, 1)
        }
        .unwrap()
        .pred_opt()
        .unwrap()
        .day();
        assert_eq!(days_in_feb_2024, 29);

        let test_date_apr = NaiveDate::from_ymd_opt(2023, 4, 10).unwrap(); // April -> 30 days
        let days_in_apr_2023 = if test_date_apr.month() == 12 {
            NaiveDate::from_ymd_opt(test_date_apr.year() + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(test_date_apr.year(), test_date_apr.month() + 1, 1)
        }
        .unwrap()
        .pred_opt()
        .unwrap()
        .day();
        assert_eq!(days_in_apr_2023, 30);
    }
}
