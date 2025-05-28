use crate::bot::{Context, Error};
use crate::config::AppConfig;
use crate::db::{self, DbPool};
use crate::models::Envelope;
use chrono::{Datelike, Local, NaiveDate};
use poise::serenity_prelude::AutocompleteChoice;
use std::sync::Arc;
use tracing::{error, trace};

// Helper function to get current date info
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

// Helper function to generate report field data for a single envelope
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
    let spent_for_indicator = f64::max(0.0, envelope.allocation - envelope.balance);
    let daily_allocation = if envelope.allocation > 0.0 {
        envelope.allocation / days_in_month
    } else {
        0.0
    };
    let expected_spending_to_date = daily_allocation * current_day_of_month;

    let status_emoji = if envelope.allocation <= 0.0 {
        "⚪"
    } else if spent_for_indicator <= expected_spending_to_date * 0.90 {
        "🟢"
    } else if spent_for_indicator > expected_spending_to_date * 1.10 {
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

pub(crate) async fn envelope_name_autocomplete(
    ctx: Context<'_>,
    partial: &str,
) -> Vec<AutocompleteChoice> {
    trace!(user = %ctx.author().name, partial_input = partial, "Autocomplete request received for envelope_name");

    let data = ctx.data();
    let db_pool = &data.db_pool;
    let author_id_str = ctx.author().id.to_string();
    trace!(author_id = %author_id_str, "Author ID for autocomplete query");

    match db::suggest_accessible_envelope_names(db_pool, &author_id_str, partial).await {
        Ok(names) => {
            trace!(fetched_names = ?names, "Names fetched from DB for autocomplete");
            let choices: Vec<AutocompleteChoice> = names
                .into_iter()
                .map(|name_str| {
                    trace!(name = %name_str, value = %name_str, "Mapping to AutocompleteChoice");
                    AutocompleteChoice::new(name_str.clone(), name_str)
                })
                .collect();
            trace!(returned_choices = ?choices, "Returning choices for autocomplete");
            choices
        }
        Err(e) => {
            error!(
                "Autocomplete: Failed to fetch envelope suggestions: {:?}",
                e
            );
            Vec::new()
        }
    }
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
