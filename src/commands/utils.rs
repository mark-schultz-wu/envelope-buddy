use crate::bot::{Context, Error};
use crate::config::AppConfig;
use crate::db::{
    self, DbPool, envelopes::suggest_accessible_envelope_names, products::suggest_product_names,
};
use crate::models::Envelope;
use chrono::{Datelike, Local, NaiveDate};
use poise::serenity_prelude::{self, AutocompleteChoice};
use std::sync::Arc;
use tracing::{error, trace};

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
/// This function queries the database directly for active envelope names.
/// It performs a case-insensitive search and returns up to 25 matching choices.
///
/// # Parameters
///
/// * `ctx`: The command context, used to access the database.
/// * `partial`: The partial string input by the user for the envelope name.
///
/// # Returns
///
/// A `Vec<AutocompleteChoice>` containing up to 25 matching envelope names.
pub(crate) async fn envelope_name_autocomplete(
    ctx: Context<'_>,
    partial: &str,
) -> Vec<AutocompleteChoice> {
    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request for envelope_name");

    let data = ctx.data();
    let author_id_str = ctx.author().id.to_string();

    // Query database directly using suggest_accessible_envelope_names
    // This replaces the previous cache-based approach for always-fresh data
    let matching_names =
        match suggest_accessible_envelope_names(&data.db_pool, &author_id_str, partial).await {
            Ok(names) => names,
            Err(e) => {
                error!("Failed to fetch envelope names for autocomplete: {}", e);
                return Vec::new();
            }
        };

    // Convert to choices (already sorted by the DB function)
    matching_names
        .into_iter()
        .take(25)
        .map(|name| AutocompleteChoice::new(name.clone(), name))
        .collect()
}

/// Provides autocomplete suggestions for product names based on partial user input.
///
/// This function queries the database directly for product names.
/// It performs a case-insensitive search and returns up to 25 matching choices.
///
/// # Parameters
///
/// * `ctx`: The command context, used to access the database.
/// * `partial`: The partial string input by the user for the product name.
///
/// # Returns
///
/// A `Vec<AutocompleteChoice>` containing product names that match the partial input.
pub(crate) async fn product_name_autocomplete(
    ctx: Context<'_>,
    partial: &str,
) -> Vec<AutocompleteChoice> {
    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request for product_name");

    let data = ctx.data();

    // Query database directly using suggest_product_names
    // This replaces the previous cache-based approach for always-fresh data
    let matching_names = match suggest_product_names(&data.db_pool, partial).await {
        Ok(names) => names,
        Err(e) => {
            error!("Failed to fetch product names for autocomplete: {}", e);
            return Vec::new();
        }
    };

    // Convert to choices (already sorted by the DB function)
    matching_names
        .into_iter()
        .take(25)
        .map(|name| AutocompleteChoice::new(name.clone(), name))
        .collect()
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
    use crate::db::envelopes::suggest_accessible_envelope_names;
    use crate::db::test_utils::{
        DirectInsertArgs, direct_insert_envelope, init_test_tracing, setup_test_db,
    };
    use crate::db::{add_product, suggest_product_names};
    use crate::errors::Result;

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
    #[tokio::test]
    async fn test_autocomplete_integration() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;

        // Setup test envelope
        let env_id;
        {
            let conn = pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Groceries",
                category: "necessary",
                allocation: 500.0,
                balance: 500.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }

        // Setup test products
        add_product(&pool, "Milk", 3.99, env_id, None).await?;
        add_product(&pool, "Bread", 2.50, env_id, None).await?;
        add_product(&pool, "Apples", 4.00, env_id, None).await?;

        // Test product autocomplete
        let suggestions = suggest_product_names(&pool, "Mi").await?;
        assert_eq!(suggestions, vec!["Milk"]);

        let suggestions2 = suggest_product_names(&pool, "Br").await?;
        assert_eq!(suggestions2, vec!["Bread"]);

        // Test envelope autocomplete
        let env_suggestions = suggest_accessible_envelope_names(&pool, "user123", "Gro").await?;
        assert_eq!(env_suggestions, vec!["Groceries"]);

        // Test case insensitive
        let suggestions3 = suggest_product_names(&pool, "milk").await?;
        assert_eq!(suggestions3, vec!["Milk"]);

        Ok(())
    }

    #[tokio::test]
    async fn test_autocomplete_error_handling() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;

        // Test invalid envelope access (non-existent user)
        let result =
            suggest_accessible_envelope_names(&pool, "nonexistent_user", "anything").await?;
        assert!(
            result.is_empty(),
            "Should return empty for non-existent user"
        );

        // Test empty partial input
        let empty_result = suggest_product_names(&pool, "").await?;
        assert!(
            empty_result.is_empty(),
            "Should handle empty input gracefully"
        );

        // Test non-existent product search
        let no_match = suggest_product_names(&pool, "NonExistentProduct").await?;
        assert!(no_match.is_empty(), "Should return empty for no matches");

        Ok(())
    }
}
