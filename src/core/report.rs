//! Report generation business logic.
//!
//! This module provides functions for generating spending reports, progress calculations,
//! and transaction summaries. All functions are framework-agnostic and return structured
//! data that can be formatted by the bot layer.

use crate::{
    entities::{envelope, transaction},
    errors::Result,
};
use sea_orm::DatabaseConnection;

/// Represents a comprehensive envelope report with spending analysis.
#[derive(Debug, Clone)]
pub struct EnvelopeReport {
    /// The envelope being reported on
    pub envelope: envelope::Model,
    /// Current balance
    pub balance: f64,
    /// Monthly allocation
    pub allocation: f64,
    /// Progress as a percentage (0-100)
    pub progress_percent: f64,
    /// Recent transactions for this envelope
    pub recent_transactions: Vec<transaction::Model>,
    /// Amount spent this period
    pub amount_spent: f64,
    /// Amount remaining
    pub amount_remaining: f64,
}

/// Generates a comprehensive report for a specific envelope.
///
/// This function retrieves the envelope details and recent transactions,
/// calculates spending progress, and returns structured report data.
///
/// # Arguments
/// * `db` - Database connection
/// * `envelope_id` - ID of the envelope to report on
/// * `transaction_limit` - Maximum number of recent transactions to include (default 10)
///
/// # Returns
/// A structured `EnvelopeReport` containing all report data
pub async fn generate_envelope_report(
    db: &DatabaseConnection,
    envelope_id: i64,
    transaction_limit: Option<u64>,
) -> Result<EnvelopeReport> {
    // Get envelope
    let envelope = crate::core::envelope::get_envelope_by_id(db, envelope_id)
        .await?
        .ok_or_else(|| crate::errors::Error::EnvelopeNotFound {
            name: envelope_id.to_string(),
        })?;

    // Get recent transactions (default 10)
    let limit = transaction_limit.unwrap_or(10);
    let all_transactions =
        crate::core::transaction::get_transactions_for_envelope(db, envelope_id).await?;
    let recent_transactions: Vec<transaction::Model> = all_transactions
        .into_iter()
        .take(limit.try_into()?)
        .collect();

    // Calculate spending metrics
    let balance = envelope.balance;
    let allocation = envelope.allocation;
    let progress_percent = calculate_progress(balance, allocation);
    let amount_remaining = balance;
    let amount_spent = allocation - balance;

    Ok(EnvelopeReport {
        envelope,
        balance,
        allocation,
        progress_percent,
        recent_transactions,
        amount_spent,
        amount_remaining,
    })
}

/// Calculates progress percentage based on current balance and allocation.
///
/// Progress represents how much of the allocation has been used:
/// - 100% = full allocation remaining (no spending)
/// - 50% = half allocation remaining
/// - 0% = allocation fully spent
/// - Negative percentages indicate overspending
///
/// # Arguments
/// * `balance` - Current envelope balance
/// * `allocation` - Monthly allocation amount
///
/// # Returns
/// Progress percentage (0-100, can be negative for overspending)
#[must_use]
pub fn calculate_progress(balance: f64, allocation: f64) -> f64 {
    if allocation == 0.0 {
        return 0.0;
    }

    (balance / allocation) * 100.0
}

/// Generates a progress bar string for visual representation.
///
/// Creates a text-based progress bar like: `[████████░░] 80%`
///
/// # Arguments
/// * `progress_percent` - Progress percentage (0-100)
/// * `bar_length` - Length of the progress bar in characters (default 10)
///
/// # Returns
/// Formatted progress bar string
#[must_use]
pub fn format_progress_bar(progress_percent: f64, bar_length: Option<usize>) -> String {
    let length = bar_length.unwrap_or(10);
    let clamped_progress = progress_percent.clamp(0.0, 100.0);

    // Cast safety: clamped_progress ∈ [0, 100], length is small (10-20).
    // Result is mathematically in [0, length], truncation/sign loss intentional for display.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
    let filled = ((clamped_progress / 100.0) * length as f64).round() as usize;
    let empty = length.saturating_sub(filled);

    let filled_str = "█".repeat(filled);
    let empty_str = "░".repeat(empty);

    format!("[{filled_str}{empty_str}] {progress_percent:.1}%")
}

/// Formats a transaction amount with appropriate sign and currency.
///
/// # Arguments
/// * `amount` - Transaction amount (positive for income, negative for expenses)
///
/// # Returns
/// Formatted string like "+$50.00" or "-$25.50"
#[must_use]
pub fn format_transaction_amount(amount: f64) -> String {
    if amount >= 0.0 {
        format!("+${amount:.2}")
    } else {
        format!("-${:.2}", amount.abs())
    }
}

/// Generates a summary line for a transaction.
///
/// # Arguments
/// * `transaction` - The transaction to summarize
///
/// # Returns
/// Formatted summary string
#[must_use]
pub fn format_transaction_summary(transaction: &transaction::Model) -> String {
    let amount_str = format_transaction_amount(transaction.amount);
    let desc = &transaction.description;
    let tx_type = &transaction.transaction_type;

    format!("{amount_str} | {tx_type} | {desc}")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::test_utils::*;

    #[test]
    fn test_calculate_progress_full_allocation() {
        // Full allocation remaining = 100%
        let progress = calculate_progress(100.0, 100.0);
        assert_eq!(progress, 100.0);
    }

    #[test]
    fn test_calculate_progress_half_spent() {
        // Half spent = 50% remaining
        let progress = calculate_progress(50.0, 100.0);
        assert_eq!(progress, 50.0);
    }

    #[test]
    fn test_calculate_progress_fully_spent() {
        // All spent = 0% remaining
        let progress = calculate_progress(0.0, 100.0);
        assert_eq!(progress, 0.0);
    }

    #[test]
    fn test_calculate_progress_overspent() {
        // Overspent = negative percentage
        let progress = calculate_progress(-25.0, 100.0);
        assert_eq!(progress, -25.0);
    }

    #[test]
    fn test_calculate_progress_zero_allocation() {
        // Zero allocation edge case
        let progress = calculate_progress(50.0, 0.0);
        assert_eq!(progress, 0.0);
    }

    #[test]
    fn test_format_progress_bar_full() {
        let bar = format_progress_bar(100.0, Some(10));
        assert_eq!(bar, "[██████████] 100.0%");
    }

    #[test]
    fn test_format_progress_bar_half() {
        let bar = format_progress_bar(50.0, Some(10));
        assert_eq!(bar, "[█████░░░░░] 50.0%");
    }

    #[test]
    fn test_format_progress_bar_zero() {
        let bar = format_progress_bar(0.0, Some(10));
        assert_eq!(bar, "[░░░░░░░░░░] 0.0%");
    }

    #[test]
    fn test_format_progress_bar_overspent() {
        // Overspending is clamped to 0% in the bar
        let bar = format_progress_bar(-25.0, Some(10));
        assert_eq!(bar, "[░░░░░░░░░░] -25.0%");
    }

    #[test]
    fn test_format_transaction_amount_positive() {
        assert_eq!(format_transaction_amount(50.0), "+$50.00");
        assert_eq!(format_transaction_amount(123.45), "+$123.45");
    }

    #[test]
    fn test_format_transaction_amount_negative() {
        assert_eq!(format_transaction_amount(-50.0), "-$50.00");
        assert_eq!(format_transaction_amount(-123.45), "-$123.45");
    }

    #[test]
    fn test_format_transaction_amount_zero() {
        assert_eq!(format_transaction_amount(0.0), "+$0.00");
    }

    #[tokio::test]
    async fn test_generate_envelope_report_integration() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create some transactions (they automatically update balance)
        create_test_transaction(&db, envelope.id, 100.0).await?; // Added funds (+100)
        create_test_transaction(&db, envelope.id, -25.0).await?; // Spent (-25)
        // Final balance should be: 0 + 100 - 25 = 75

        // Generate report
        let report = generate_envelope_report(&db, envelope.id, Some(5)).await?;

        assert_eq!(report.envelope.id, envelope.id);
        assert_eq!(report.balance, 75.0);
        assert_eq!(report.allocation, 100.0);
        assert_eq!(report.progress_percent, 75.0);
        assert_eq!(report.amount_remaining, 75.0);
        assert_eq!(report.amount_spent, 25.0);
        assert_eq!(report.recent_transactions.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_generate_envelope_report_no_transactions() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        let report = generate_envelope_report(&db, envelope.id, None).await?;

        assert_eq!(report.envelope.id, envelope.id);
        assert_eq!(report.recent_transactions.len(), 0);
        assert_eq!(report.balance, 0.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_generate_envelope_report_transaction_limit() -> Result<()> {
        let (db, envelope) = setup_with_envelope().await?;

        // Create 15 transactions
        for i in 0..15 {
            create_custom_transaction(
                &db,
                envelope.id,
                10.0,
                &format!("Transaction {i}"),
                "user1",
                None,
                "addfunds",
            )
            .await?;
        }

        // Request only 5 recent
        let report = generate_envelope_report(&db, envelope.id, Some(5)).await?;

        assert_eq!(report.recent_transactions.len(), 5);

        Ok(())
    }
}
