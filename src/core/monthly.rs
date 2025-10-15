//! Monthly update business logic
//!
//! Handles monthly envelope updates and maintenance.
//! This module provides functionality for processing monthly updates to envelopes,
//! including resetting balances for non-rollover envelopes and rolling over balances
//! for rollover envelopes. It also tracks the last monthly update timestamp using the
//! `system_state` table to prevent duplicate updates within the same month.

use crate::{
    entities::{Envelope, SystemState, envelope, system_state},
    errors::{Error, Result},
};
use chrono::{Datelike, NaiveDate, Utc};
use sea_orm::{Set, TransactionTrait, prelude::*};

const LAST_MONTHLY_UPDATE_KEY: &str = "last_monthly_update";

/// Represents the result of a monthly update operation for a single envelope.
#[derive(Debug, Clone)]
pub struct EnvelopeUpdateResult {
    /// Name of the envelope that was updated
    pub envelope_name: String,
    /// Balance before the monthly update
    pub old_balance: f64,
    /// Balance after the monthly update
    pub new_balance: f64,
    /// Monthly allocation amount for this envelope
    pub allocation: f64,
    /// Whether rollover is enabled for this envelope
    pub rollover: bool,
}

/// Represents the result of processing monthly updates for all envelopes.
#[derive(Debug, Clone)]
pub struct MonthlyUpdateResult {
    /// Detailed results for each envelope that was updated
    pub updated_envelopes: Vec<EnvelopeUpdateResult>,
    /// Total number of envelopes processed
    pub total_envelopes_processed: usize,
    /// Number of envelopes with rollover enabled
    pub rollover_count: usize,
    /// Number of envelopes that were reset (no rollover)
    pub reset_count: usize,
    /// Date when the update was performed
    pub update_date: NaiveDate,
}

/// Checks if a monthly update is needed by comparing the last update date
/// with the current date. Returns true if we've entered a new month since
/// the last update, or if no previous update exists.
///
/// # Arguments
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(true)` - A monthly update is needed
/// * `Ok(false)` - Already updated this month
pub async fn is_monthly_update_needed(db: &DatabaseConnection) -> Result<bool> {
    let last_update = get_last_monthly_update_date(db).await?;
    let now = Utc::now().date_naive();

    last_update.map_or_else(
        || Ok(true),
        |last_date| Ok(last_date.year() != now.year() || last_date.month() != now.month()),
    )
}

/// Retrieves the date of the last monthly update from the `system_state` table.
///
/// # Arguments
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(Some(date))` - Last update date if it exists
/// * `Ok(None)` - No previous update recorded
pub async fn get_last_monthly_update_date(db: &DatabaseConnection) -> Result<Option<NaiveDate>> {
    let state = SystemState::find()
        .filter(system_state::Column::Key.eq(LAST_MONTHLY_UPDATE_KEY))
        .one(db)
        .await?;

    match state {
        Some(s) => {
            // Parse the stored date string (format: YYYY-MM-DD)
            NaiveDate::parse_from_str(&s.value, "%Y-%m-%d")
                .map(Some)
                .map_err(|e| Error::Config {
                    message: format!("Failed to parse last update date: {e}"),
                })
        }
        None => Ok(None),
    }
}

/// Updates the last monthly update date in the `system_state` table.
///
/// # Arguments
/// * `db` - Database connection
/// * `date` - The date to store as the last update date
async fn set_last_monthly_update_date<C>(db: &C, date: NaiveDate) -> Result<()>
where
    C: ConnectionTrait,
{
    let date_str = date.format("%Y-%m-%d").to_string();
    let now = Utc::now().naive_utc();

    // Check if the key exists
    let existing = SystemState::find()
        .filter(system_state::Column::Key.eq(LAST_MONTHLY_UPDATE_KEY))
        .one(db)
        .await?;

    if let Some(state) = existing {
        // Update existing record
        let mut active_model: system_state::ActiveModel = state.into();
        active_model.value = Set(date_str);
        active_model.updated_at = Set(now);
        active_model.update(db).await?;
    } else {
        // Insert new record
        let new_state = system_state::ActiveModel {
            key: Set(LAST_MONTHLY_UPDATE_KEY.to_string()),
            value: Set(date_str),
            updated_at: Set(now),
            ..Default::default()
        };
        new_state.insert(db).await?;
    }

    Ok(())
}

/// Processes monthly updates for all active envelopes. This function:
///
/// 1. Checks if an update is needed (prevents duplicate updates in same month)
/// 2. For each active envelope:
///    - If rollover is enabled: adds allocation to existing balance
///    - If rollover is disabled: resets balance to allocation amount
/// 3. Records the update date in `system_state`
///
/// # Arguments
/// * `db` - Database connection
///
/// # Returns
/// * `Ok(Some(result))` - Update was performed with detailed results
/// * `Ok(None)` - No update needed (already updated this month)
pub async fn process_monthly_updates(
    db: &DatabaseConnection,
) -> Result<Option<MonthlyUpdateResult>> {
    // Check if update is needed
    if !is_monthly_update_needed(db).await? {
        return Ok(None);
    }

    // Start a database transaction to ensure atomicity
    // All envelope updates must succeed or all must fail
    let txn = db.begin().await?;

    let now = Utc::now().date_naive();
    let mut results = Vec::new();
    let mut rollover_count = 0;
    let mut reset_count = 0;

    // Get all active envelopes
    let envelopes = Envelope::find()
        .filter(envelope::Column::IsDeleted.eq(false))
        .all(&txn)
        .await?;

    // Process each envelope
    for env in envelopes {
        let old_balance = env.balance;
        let new_balance = if env.rollover {
            // Rollover: add allocation to existing balance
            env.balance + env.allocation
        } else {
            // No rollover: reset to allocation
            env.allocation
        };

        // Update the envelope balance
        let mut active_model: envelope::ActiveModel = env.clone().into();
        active_model.balance = Set(new_balance);
        active_model.update(&txn).await?;

        // Track statistics
        if env.rollover {
            rollover_count += 1;
        } else {
            reset_count += 1;
        }

        // Store result
        results.push(EnvelopeUpdateResult {
            envelope_name: env.name,
            old_balance,
            new_balance,
            allocation: env.allocation,
            rollover: env.rollover,
        });
    }

    // Record the update date
    set_last_monthly_update_date(&txn, now).await?;

    // Commit the transaction - all updates succeed or all fail
    txn.commit().await?;

    Ok(Some(MonthlyUpdateResult {
        total_envelopes_processed: results.len(),
        rollover_count,
        reset_count,
        updated_envelopes: results,
        update_date: now,
    }))
}

/// Formats a monthly update result into a human-readable summary string.
/// This is useful for logging or displaying the results of a monthly update.
///
/// # Arguments
/// * `result` - The monthly update result to format
///
/// # Returns
/// * A formatted string summarizing the update
#[must_use]
pub fn format_monthly_update_summary(result: &MonthlyUpdateResult) -> String {
    use std::fmt::Write;

    let mut summary = format!(
        "Monthly Update - {} - Processed {} envelopes\n",
        result.update_date.format("%B %Y"),
        result.total_envelopes_processed
    );

    // write! is infallible when writing to String, so unwrap is safe
    write!(
        summary,
        "  Rollover: {} envelopes | Reset: {} envelopes\n\n",
        result.rollover_count, result.reset_count
    )
    .unwrap();

    for envelope_result in &result.updated_envelopes {
        let change_type = if envelope_result.rollover {
            "Rollover"
        } else {
            "Reset"
        };

        writeln!(
            summary,
            "  {} - {} | ${:.2} → ${:.2} (Allocation: ${:.2})",
            envelope_result.envelope_name,
            change_type,
            envelope_result.old_balance,
            envelope_result.new_balance,
            envelope_result.allocation
        )
        .unwrap();
    }

    summary
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::test_utils::*;

    #[tokio::test]
    async fn test_is_monthly_update_needed_no_previous_update() -> Result<()> {
        let db = setup_test_db().await?;

        // Should need update when no previous update exists
        let needed = is_monthly_update_needed(&db).await?;
        assert!(needed);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_last_monthly_update_date_none() -> Result<()> {
        let db = setup_test_db().await?;

        let last_date = get_last_monthly_update_date(&db).await?;
        assert!(last_date.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_set_and_get_last_monthly_update_date() -> Result<()> {
        let db = setup_test_db().await?;

        let test_date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        set_last_monthly_update_date(&db, test_date).await?;

        let retrieved_date = get_last_monthly_update_date(&db).await?;
        assert_eq!(retrieved_date, Some(test_date));

        Ok(())
    }

    #[tokio::test]
    async fn test_set_last_monthly_update_date_updates_existing() -> Result<()> {
        let db = setup_test_db().await?;

        // Set initial date
        let first_date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        set_last_monthly_update_date(&db, first_date).await?;

        // Update to new date
        let second_date = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        set_last_monthly_update_date(&db, second_date).await?;

        // Should have the updated date
        let retrieved_date = get_last_monthly_update_date(&db).await?;
        assert_eq!(retrieved_date, Some(second_date));

        // Verify only one record exists
        let count = SystemState::find()
            .filter(system_state::Column::Key.eq(LAST_MONTHLY_UPDATE_KEY))
            .count(&db)
            .await?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_rollover_envelope() -> Result<()> {
        let db = setup_test_db().await?;

        // Create rollover envelope with existing balance
        let envelope = create_custom_envelope(
            &db,
            "Rollover Test",
            None,
            "savings",
            100.0,
            false,
            true, // rollover enabled
        )
        .await?;

        // Set initial balance to 50.0
        crate::core::envelope::update_envelope_balance_atomic(&db, envelope.id, 50.0).await?;

        // Process monthly update
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        let update_result = result.unwrap();
        assert_eq!(update_result.total_envelopes_processed, 1);
        assert_eq!(update_result.rollover_count, 1);
        assert_eq!(update_result.reset_count, 0);

        // Check the envelope update result
        let env_result = &update_result.updated_envelopes[0];
        assert_eq!(env_result.envelope_name, "Rollover Test");
        assert_eq!(env_result.old_balance, 50.0);
        assert_eq!(env_result.new_balance, 150.0); // 50.0 + 100.0 allocation
        assert_eq!(env_result.allocation, 100.0);
        assert!(env_result.rollover);

        // Verify envelope balance was updated
        let updated_envelope = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(updated_envelope.balance, 150.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_reset_envelope() -> Result<()> {
        let db = setup_test_db().await?;

        // Create non-rollover envelope with existing balance
        let envelope = create_custom_envelope(
            &db,
            "Reset Test",
            None,
            "entertainment",
            200.0,
            false,
            false, // rollover disabled
        )
        .await?;

        // Set initial balance (different from allocation)
        crate::core::envelope::update_envelope_balance_atomic(&db, envelope.id, 75.0).await?;

        // Process monthly update
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        let update_result = result.unwrap();
        assert_eq!(update_result.total_envelopes_processed, 1);
        assert_eq!(update_result.rollover_count, 0);
        assert_eq!(update_result.reset_count, 1);

        // Check the envelope update result
        let env_result = &update_result.updated_envelopes[0];
        assert_eq!(env_result.envelope_name, "Reset Test");
        assert_eq!(env_result.old_balance, 75.0);
        assert_eq!(env_result.new_balance, 200.0); // Reset to allocation
        assert_eq!(env_result.allocation, 200.0);
        assert!(!env_result.rollover);

        // Verify envelope balance was updated
        let updated_envelope = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(updated_envelope.balance, 200.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_multiple_envelopes() -> Result<()> {
        let db = setup_test_db().await?;

        // Create multiple envelopes with different rollover settings
        let env1 =
            create_custom_envelope(&db, "Rollover 1", None, "savings", 100.0, false, true).await?;
        let env2 =
            create_custom_envelope(&db, "Reset 1", None, "food", 200.0, false, false).await?;
        let env3 =
            create_custom_envelope(&db, "Rollover 2", None, "savings", 50.0, false, true).await?;

        // Set initial balances
        crate::core::envelope::update_envelope_balance_atomic(&db, env1.id, 30.0).await?;
        crate::core::envelope::update_envelope_balance_atomic(&db, env2.id, 150.0).await?;
        crate::core::envelope::update_envelope_balance_atomic(&db, env3.id, 20.0).await?;

        // Process monthly update
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        let update_result = result.unwrap();
        assert_eq!(update_result.total_envelopes_processed, 3);
        assert_eq!(update_result.rollover_count, 2);
        assert_eq!(update_result.reset_count, 1);

        // Verify all envelopes were updated correctly
        let updated_env1 = Envelope::find_by_id(env1.id).one(&db).await?.unwrap();
        assert_eq!(updated_env1.balance, 130.0); // 30.0 + 100.0

        let updated_env2 = Envelope::find_by_id(env2.id).one(&db).await?.unwrap();
        assert_eq!(updated_env2.balance, 200.0); // Reset to allocation

        let updated_env3 = Envelope::find_by_id(env3.id).one(&db).await?.unwrap();
        assert_eq!(updated_env3.balance, 70.0); // 20.0 + 50.0

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_skips_deleted_envelopes() -> Result<()> {
        let db = setup_test_db().await?;

        // Create envelopes
        let _env1 = create_test_envelope(&db, "Active").await?;
        let env2 = create_test_envelope(&db, "Deleted").await?;

        // Soft delete the second envelope by updating its is_deleted flag
        let mut active_model: envelope::ActiveModel = env2.into();
        active_model.is_deleted = Set(true);
        active_model.update(&db).await?;

        // Process monthly update
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        let update_result = result.unwrap();
        assert_eq!(update_result.total_envelopes_processed, 1);

        // Verify only the active envelope was processed
        assert_eq!(update_result.updated_envelopes[0].envelope_name, "Active");

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_prevents_duplicate() -> Result<()> {
        let db = setup_test_db().await?;

        create_test_envelope(&db, "Test").await?;

        // First update should succeed
        let result1 = process_monthly_updates(&db).await?;
        assert!(result1.is_some());

        // Second update in same month should return None
        let result2 = process_monthly_updates(&db).await?;
        assert!(result2.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_records_date() -> Result<()> {
        let db = setup_test_db().await?;

        create_test_envelope(&db, "Test").await?;

        let before_update = Utc::now().date_naive();

        // Process update
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        // Verify the date was recorded
        let recorded_date = get_last_monthly_update_date(&db).await?;
        assert!(recorded_date.is_some());

        let date = recorded_date.unwrap();
        assert_eq!(date, before_update);

        Ok(())
    }

    #[tokio::test]
    async fn test_format_monthly_update_summary() -> Result<()> {
        let result = MonthlyUpdateResult {
            total_envelopes_processed: 3,
            rollover_count: 2,
            reset_count: 1,
            update_date: NaiveDate::from_ymd_opt(2024, 3, 1).unwrap(),
            updated_envelopes: vec![
                EnvelopeUpdateResult {
                    envelope_name: "Savings".to_string(),
                    old_balance: 100.0,
                    new_balance: 200.0,
                    allocation: 100.0,
                    rollover: true,
                },
                EnvelopeUpdateResult {
                    envelope_name: "Food".to_string(),
                    old_balance: 50.0,
                    new_balance: 150.0,
                    allocation: 150.0,
                    rollover: false,
                },
            ],
        };

        let summary = format_monthly_update_summary(&result);

        // Verify summary contains key information
        assert!(summary.contains("March 2024"));
        assert!(summary.contains("Processed 3 envelopes"));
        assert!(summary.contains("Rollover: 2 envelopes"));
        assert!(summary.contains("Reset: 1 envelopes"));
        assert!(summary.contains("Savings"));
        assert!(summary.contains("Food"));
        assert!(summary.contains("$100.00 → $200.00"));
        assert!(summary.contains("$50.00 → $150.00"));

        Ok(())
    }

    #[tokio::test]
    async fn test_rollover_with_negative_balance() -> Result<()> {
        let db = setup_test_db().await?;

        // Create rollover envelope
        let envelope = create_custom_envelope(
            &db,
            "Negative Balance Test",
            None,
            "entertainment",
            100.0,
            false,
            true, // rollover enabled
        )
        .await?;

        // Set negative balance (overspent)
        crate::core::envelope::update_envelope_balance_atomic(&db, envelope.id, -25.0).await?;

        // Process monthly update
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        let update_result = result.unwrap();
        let env_result = &update_result.updated_envelopes[0];

        // Should roll over negative balance: -25.0 + 100.0 = 75.0
        assert_eq!(env_result.old_balance, -25.0);
        assert_eq!(env_result.new_balance, 75.0);

        // Verify in database
        let updated_envelope = Envelope::find_by_id(envelope.id).one(&db).await?.unwrap();
        assert_eq!(updated_envelope.balance, 75.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_process_monthly_updates_empty_database() -> Result<()> {
        let db = setup_test_db().await?;

        // Process with no envelopes
        let result = process_monthly_updates(&db).await?;
        assert!(result.is_some());

        let update_result = result.unwrap();
        assert_eq!(update_result.total_envelopes_processed, 0);
        assert_eq!(update_result.rollover_count, 0);
        assert_eq!(update_result.reset_count, 0);
        assert_eq!(update_result.updated_envelopes.len(), 0);

        // Date should still be recorded
        let recorded_date = get_last_monthly_update_date(&db).await?;
        assert!(recorded_date.is_some());

        Ok(())
    }
}
