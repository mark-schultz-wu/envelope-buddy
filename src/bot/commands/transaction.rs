//! Transaction Discord commands - `spend` and `addfunds`.
//!
//! This module contains commands that interact with the database through our core modules
//! to handle financial transactions and reporting within the envelope system.

// Inner module to suppress missing_docs warnings for poise macro-generated code
mod inner {
    #![allow(missing_docs)]

    use crate::{
        bot::{BotData, handlers::autocomplete},
        config::users,
        core::{envelope, transaction},
        errors::{Error, Result},
    };

    /// Records an expense from an envelope.
    ///
    /// This command deducts the specified amount from the envelope balance and creates
    /// a transaction record for tracking purposes. If no user is specified, it uses the
    /// command author's ID for individual envelopes or looks for shared envelopes.
    #[poise::command(slash_command, prefix_command)]
    pub async fn spend(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope to spend from"]
        #[autocomplete = "autocomplete::autocomplete_envelope_name"]
        envelope_name: String,
        #[description = "Amount to spend"] amount: f64,
        #[description = "Optional user nickname (for individual envelopes)"]
        #[autocomplete = "autocomplete::autocomplete_user"]
        user: Option<String>,
        #[description = "Optional description of the expense"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Transaction";

        // Validate amount parameter
        if amount.is_nan() || amount.is_infinite() {
            ctx.say("❌ Invalid amount: must be a valid number").await?;
            return Ok(());
        }
        if amount <= 0.0 {
            ctx.say("❌ Invalid amount: must be greater than zero")
                .await?;
            return Ok(());
        }

        // Resolve user nickname or default to the command author
        let author_id = ctx.author().id.to_string();
        let target_user_id = if let Some(nickname) = user {
            // Try to resolve the provided nickname
            if let Some(user_id) = users::resolve_nickname(&nickname) {
                user_id
            } else {
                ctx.say(&format!(
                    "❌ Unknown nickname '{}'. Available nicknames: {}",
                    nickname,
                    users::get_all_nicknames().join(", ")
                ))
                .await?;
                return Ok(());
            }
        } else {
            author_id.clone()
        };
        let desc = description.as_deref().unwrap_or(DEFAULT_DESCRIPTION);

        // Get database connection from context
        let db = &ctx.data().database;

        // Find the envelope by name and user
        let envelope =
            envelope::get_envelope_by_name_and_user(db, &envelope_name, &target_user_id).await?;

        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "❌ Envelope '{envelope_name}' not found. Use `/envelopes` to see available envelopes.",
            ))
            .await?;
            return Ok(());
        };

        // Create the transaction (negative amount for spending)
        let transaction_result = transaction::create_transaction(
            db,
            envelope.id,
            -amount, // Negative amount for spending
            desc.to_string(),
            author_id.clone(),
            None, // No Discord message ID
            "spend".to_string(),
        )
        .await?;

        ctx.say(&format!(
            "✅ Spent ${:.2} from envelope '{}' - {} (Transaction ID: {})",
            amount, envelope_name, desc, transaction_result.id
        ))
        .await?;

        Ok(())
    }

    /// Adds funds to an envelope.
    ///
    /// This command increases the envelope balance by the specified amount and creates
    /// a transaction record for tracking purposes. If no user is specified, it uses the
    /// command author's ID for individual envelopes or looks for shared envelopes.
    #[poise::command(slash_command, prefix_command)]
    pub async fn addfunds(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope to add funds to"]
        #[autocomplete = "autocomplete::autocomplete_envelope_name"]
        envelope_name: String,
        #[description = "Amount to add"] amount: f64,
        #[description = "Optional user nickname (for individual envelopes)"]
        #[autocomplete = "autocomplete::autocomplete_user"]
        user: Option<String>,
        #[description = "Optional description of the income"] description: Option<String>,
    ) -> Result<()> {
        const DEFAULT_DESCRIPTION: &str = "Income";

        // Validate amount parameter
        if amount.is_nan() || amount.is_infinite() {
            ctx.say("❌ Invalid amount: must be a valid number").await?;
            return Ok(());
        }
        if amount <= 0.0 {
            ctx.say("❌ Invalid amount: must be greater than zero")
                .await?;
            return Ok(());
        }

        // Resolve user nickname or default to the command author
        let author_id = ctx.author().id.to_string();
        let target_user_id = if let Some(nickname) = user {
            // Try to resolve the provided nickname
            if let Some(user_id) = users::resolve_nickname(&nickname) {
                user_id
            } else {
                ctx.say(&format!(
                    "❌ Unknown nickname '{}'. Available nicknames: {}",
                    nickname,
                    users::get_all_nicknames().join(", ")
                ))
                .await?;
                return Ok(());
            }
        } else {
            author_id.clone()
        };
        let desc = description.as_deref().unwrap_or(DEFAULT_DESCRIPTION);

        // Get database connection from context
        let db = &ctx.data().database;

        // Find the envelope by name and user
        let envelope =
            envelope::get_envelope_by_name_and_user(db, &envelope_name, &target_user_id).await?;

        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "❌ Envelope '{envelope_name}' not found. Use `/envelopes` to see available envelopes.",
            ))
            .await?;
            return Ok(());
        };

        // Create the transaction (positive amount for adding funds)
        let transaction_result = transaction::create_transaction(
            db,
            envelope.id,
            amount, // Positive amount for adding funds
            desc.to_string(),
            author_id.clone(),
            None, // No Discord message ID
            "addfunds".to_string(),
        )
        .await?;

        ctx.say(&format!(
            "✅ Added ${:.2} to envelope '{}' - {} (Transaction ID: {})",
            amount, envelope_name, desc, transaction_result.id
        ))
        .await?;

        Ok(())
    }
}

// Re-export all commands
pub use inner::*;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::float_cmp)]
    use crate::{
        core::{envelope, transaction},
        errors::Result,
        test_utils::*,
    };

    /// Tests the bug where `spend()` without a user parameter can spend from
    /// the wrong user's individual envelope.
    ///
    /// Scenario:
    /// - User "alice" has an individual envelope named "Groceries"
    /// - User "bob" has an individual envelope named "Groceries"
    /// - When user "alice" runs `/spend Groceries 50` (without user parameter)
    /// - EXPECTED: Should spend from alice's "Groceries" envelope
    /// - BUG: Might spend from bob's "Groceries" envelope instead
    #[tokio::test]
    async fn test_spend_bug_wrong_users_envelope() -> Result<()> {
        let db = setup_test_db().await?;

        // Create two users with individual envelopes with the same name
        let alice_id = "alice_user_id";
        let bob_id = "bob_user_id";

        // Create Alice's individual "Groceries" envelope
        let alice_envelope = create_custom_envelope(
            &db,
            "Groceries",
            Some(alice_id.to_string()),
            "food",
            100.0, // allocation
            true,  // is_individual
            false, // rollover
        )
        .await?;

        // Give it a starting balance of $100
        envelope::update_envelope_balance_atomic(&db, alice_envelope.id, 100.0).await?;

        // Create Bob's individual "Groceries" envelope
        let bob_envelope = create_custom_envelope(
            &db,
            "Groceries",
            Some(bob_id.to_string()),
            "food",
            200.0, // allocation
            true,  // is_individual
            false, // rollover
        )
        .await?;

        // Give it a starting balance of $200
        envelope::update_envelope_balance_atomic(&db, bob_envelope.id, 200.0).await?;

        // Now demonstrate that the bug is fixed - trying to get a "shared" Groceries
        // envelope should return None since both are individual envelopes
        let found_shared = envelope::get_shared_envelope_by_name(&db, "Groceries").await?;
        assert!(
            found_shared.is_none(),
            "Individual envelopes should not be returned by get_shared_envelope_by_name"
        );

        // The correct behavior: should use get_envelope_by_name_and_user
        let alice_correct = envelope::get_envelope_by_name_and_user(&db, "Groceries", alice_id)
            .await?
            .unwrap();
        assert_eq!(alice_correct.user_id, Some(alice_id.to_string()));
        assert_eq!(alice_correct.balance, 100.0);

        let bob_correct = envelope::get_envelope_by_name_and_user(&db, "Groceries", bob_id)
            .await?
            .unwrap();
        assert_eq!(bob_correct.user_id, Some(bob_id.to_string()));
        assert_eq!(bob_correct.balance, 200.0);

        Ok(())
    }

    /// Tests that individual envelopes should only be accessible by their owner.
    #[tokio::test]
    async fn test_individual_envelope_isolation() -> Result<()> {
        let db = setup_test_db().await?;

        let user1_id = "user1";
        let user2_id = "user2";

        // Create user1's individual envelope
        let env1 = create_custom_envelope(
            &db,
            "Personal Spending",
            Some(user1_id.to_string()),
            "personal",
            100.0,
            true, // is_individual
            false,
        )
        .await?;
        envelope::update_envelope_balance_atomic(&db, env1.id, 50.0).await?;

        // User2 should NOT be able to access user1's envelope
        let result =
            envelope::get_envelope_by_name_and_user(&db, "Personal Spending", user2_id).await?;
        assert!(
            result.is_none(),
            "User2 should not be able to access User1's individual envelope"
        );

        // User1 should be able to access their own envelope
        let result =
            envelope::get_envelope_by_name_and_user(&db, "Personal Spending", user1_id).await?;
        assert!(
            result.is_some(),
            "User1 should be able to access their own envelope"
        );
        assert_eq!(result.unwrap().balance, 50.0);

        Ok(())
    }

    /// Tests that shared envelopes should be accessible by anyone.
    #[tokio::test]
    async fn test_shared_envelope_access() -> Result<()> {
        let db = setup_test_db().await?;

        // Create a shared envelope (no user_id, is_individual = false)
        let shared_env = create_custom_envelope(
            &db,
            "Household Bills",
            None, // no user_id for shared
            "bills",
            500.0,
            false, // is_individual = false (shared)
            false,
        )
        .await?;
        envelope::update_envelope_balance_atomic(&db, shared_env.id, 500.0).await?;

        // Anyone should be able to look it up by name (as a shared envelope)
        let found = envelope::get_shared_envelope_by_name(&db, "Household Bills").await?;
        assert!(found.is_some());
        assert_eq!(found.unwrap().balance, 500.0);

        Ok(())
    }

    /// Tests the expected behavior for the spend command.
    #[tokio::test]
    async fn test_spend_command_expected_behavior() -> Result<()> {
        let db = setup_test_db().await?;

        let alice_id = "alice";
        let bob_id = "bob";

        // Setup: Both users have individual "Groceries" envelopes
        let alice_groceries = create_custom_envelope(
            &db,
            "Groceries",
            Some(alice_id.to_string()),
            "food",
            100.0,
            true,
            false,
        )
        .await?;
        envelope::update_envelope_balance_atomic(&db, alice_groceries.id, 100.0).await?;

        let bob_groceries = create_custom_envelope(
            &db,
            "Groceries",
            Some(bob_id.to_string()),
            "food",
            100.0,
            true,
            false,
        )
        .await?;
        envelope::update_envelope_balance_atomic(&db, bob_groceries.id, 100.0).await?;

        // When Alice spends from "Groceries", it should use HER envelope
        let alice_envelope = envelope::get_envelope_by_name_and_user(&db, "Groceries", alice_id)
            .await?
            .unwrap();
        assert_eq!(alice_envelope.id, alice_groceries.id);

        // Create a transaction for Alice
        transaction::create_transaction(
            &db,
            alice_envelope.id,
            -50.0,
            "Groceries shopping".to_string(),
            alice_id.to_string(),
            None,
            "spend".to_string(),
        )
        .await?;

        // Verify Alice's balance decreased
        let alice_after = envelope::get_envelope_by_id(&db, alice_groceries.id)
            .await?
            .unwrap();
        assert_eq!(alice_after.balance, 50.0);

        // Verify Bob's balance is unchanged
        let bob_after = envelope::get_envelope_by_id(&db, bob_groceries.id)
            .await?
            .unwrap();
        assert_eq!(bob_after.balance, 100.0);

        Ok(())
    }
}
