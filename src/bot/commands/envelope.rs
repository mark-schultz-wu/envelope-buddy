//! Envelope Discord commands - report, update, and envelope management.
//!
//! This module contains commands that interact with the database through our core modules
//! to handle envelope operations including reporting, monthly updates, and CRUD operations.

// Inner module to suppress missing_docs warnings for poise macro-generated code
mod inner {
    #![allow(missing_docs)]

    use crate::{
        bot::{BotData, handlers::autocomplete},
        config,
        core::{envelope, monthly, report},
        errors::{Error, Result},
    };
    use chrono::Datelike;
    use sea_orm::ActiveModelTrait;
    use std::fmt::Write;

    /// Shows a comprehensive financial report of all active envelopes.
    ///
    /// This command generates a detailed report showing current balances, allocations,
    /// and spending progress for all envelopes in the system. The report includes
    /// visual progress indicators and recent transaction information.
    #[allow(clippy::too_many_lines)] // Complex reporting logic with status calculations
    #[poise::command(slash_command, prefix_command)]
    pub async fn report(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        use poise::serenity_prelude as serenity;

        let db = &ctx.data().database;

        // Get all active envelopes
        let envelopes = envelope::get_all_active_envelopes(db).await?;

        if envelopes.is_empty() {
            ctx.say("📊 No envelopes found. Create one with `/create_envelope` to get started!")
                .await?;
            return Ok(());
        }

        // Get current date info for embed description
        let now = chrono::Local::now();

        // Calculate days in current month
        // Note: from_ymd_opt only returns None for invalid dates (e.g., Feb 30).
        // Since we're using month=1-12 from DateTime and day=1, these are always valid.
        #[allow(clippy::expect_used)] // First day of any valid month/year is always valid
        let next_month_first = if now.month() == 12 {
            chrono::NaiveDate::from_ymd_opt(now.year() + 1, 1, 1)
        } else {
            chrono::NaiveDate::from_ymd_opt(now.year(), now.month() + 1, 1)
        }
        .expect("First day of next month is always valid");

        #[allow(clippy::expect_used)] // First day of current month from DateTime is always valid
        let current_month_first = chrono::NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .expect("First day of current month is always valid");

        let days_in_month = next_month_first
            .signed_duration_since(current_month_first)
            .num_days();

        let current_day = i64::from(now.day());

        // Build embed fields - one field per envelope
        let mut embed_fields = Vec::new();

        for env in &envelopes {
            let progress = report::calculate_progress(env.balance, env.allocation);
            let progress_bar = report::format_progress_bar(progress, Some(10));

            // Calculate expected pace based on day of month
            #[allow(clippy::cast_precision_loss)]
            // Days in month is small, precision loss negligible
            let expected_percent = (current_day as f64 / days_in_month as f64) * 100.0;
            let spent_amount = env.allocation - env.balance;
            let spent_percent = if env.allocation > 0.0 {
                (spent_amount / env.allocation) * 100.0
            } else {
                0.0
            };

            // Status indicator: green if on track, yellow if slightly over, red if significantly over
            let status_emoji = if spent_percent <= expected_percent {
                "🟢" // On track or under budget
            } else if spent_percent <= expected_percent + 20.0 {
                "🟡" // Slightly over pace
            } else {
                "🔴" // Significantly over pace
            };

            // Build field name: "name (User)" or "name (Shared)"
            let field_name = if env.is_individual {
                if let Some(ref uid) = env.user_id {
                    // First try to get nickname from .env config
                    let user_name = if let Some(nickname) = config::users::get_nickname(uid) {
                        nickname
                    } else {
                        // Fallback to Discord username
                        if let Ok(user_id_val) = uid.parse::<u64>() {
                            let user_id = serenity::UserId::new(user_id_val);
                            if let Ok(user) = user_id.to_user(ctx.serenity_context()).await {
                                user.name
                            } else {
                                format!("User {uid}")
                            }
                        } else {
                            format!("User {uid}")
                        }
                    };
                    format!("{} ({})", env.name, user_name)
                } else {
                    format!("{} (Individual)", env.name)
                }
            } else {
                format!("{} (Shared)", env.name)
            };

            let mut field_value = String::new();
            writeln!(
                &mut field_value,
                "**Balance:** ${:.2} / ${:.2}",
                env.balance, env.allocation
            )?;
            writeln!(
                &mut field_value,
                "**Spent:** ${:.2} ({:.1}%)",
                spent_amount.abs(),
                spent_percent.abs()
            )?;
            #[allow(clippy::cast_precision_loss)]
            // Days in month is small, precision loss negligible
            let expected_spent = env.allocation * (current_day as f64 / days_in_month as f64);
            writeln!(
                &mut field_value,
                "**Expected Pace:** ${expected_spent:.2} ({expected_percent:.1}%)"
            )?;
            write!(
                &mut field_value,
                "**Progress:** {progress_bar} {progress:.1}%"
            )?;
            writeln!(&mut field_value, "\n**Status:** {status_emoji}")?;

            embed_fields.push((field_name, field_value, false)); // false = not inline
        }

        // Create embed
        let report_embed = serenity::CreateEmbed::default()
            .title("📊 Full Envelope Report")
            .description(format!(
                "As of: {} (Day {}/{} of month)",
                now.format("%Y-%m-%d"),
                now.day(),
                days_in_month
            ))
            .color(0x0034_98DB) // Blue color
            .fields(embed_fields)
            .footer(serenity::CreateEmbedFooter::new(format!(
                "EnvelopeBuddy v0.2.0 | {} envelope{}",
                envelopes.len(),
                if envelopes.len() == 1 { "" } else { "s" }
            )));

        ctx.send(poise::CreateReply::default().embed(report_embed))
            .await?;

        Ok(())
    }

    /// Runs the monthly update process for all envelopes.
    ///
    /// This command processes monthly updates for all active envelopes:
    /// - Rollover envelopes: adds allocation to existing balance
    /// - Non-rollover envelopes: resets balance to allocation amount
    /// The command prevents duplicate updates within the same month.
    #[poise::command(slash_command, prefix_command)]
    pub async fn update(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        let db = &ctx.data().database;

        // Acknowledge command quickly
        ctx.defer().await?;

        // Process monthly updates
        match monthly::process_monthly_updates(db).await? {
            Some(result) => {
                let summary = monthly::format_monthly_update_summary(&result)?;
                ctx.say(format!(
                    "✅ **Monthly Update Complete!**\n\n```\n{summary}\n```",
                ))
                .await?;
            }
            None => {
                ctx.say("ℹ️ Monthly update already performed this month. No updates needed.")
                    .await?;
            }
        }

        Ok(())
    }

    /// Shows detailed information about a specific envelope.
    ///
    /// This command displays comprehensive information about an envelope including
    /// its balance, allocation, category, rollover setting, and recent transactions.
    #[poise::command(slash_command, prefix_command)]
    pub async fn envelope_info(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope"]
        #[autocomplete = "autocomplete::autocomplete_envelope_name"]
        envelope_name: String,
        #[description = "User ID (for individual envelopes)"] user: Option<String>,
    ) -> Result<()> {
        let db = &ctx.data().database;
        let user_id = user.unwrap_or_else(|| ctx.author().id.to_string());

        // Try to find the envelope - first check user's individual envelope, then shared
        let envelope = if let Some(env) =
            envelope::get_envelope_by_name_and_user(db, &envelope_name, &user_id).await?
        {
            Some(env)
        } else {
            envelope::get_shared_envelope_by_name(db, &envelope_name).await?
        };

        let Some(envelope) = envelope else {
            ctx.say(&format!(
                "❌ Envelope '{envelope_name}' not found. Use `/envelopes` to see all available envelopes.",
            ))
            .await?;
            return Ok(());
        };

        // Generate envelope report
        let envelope_report = report::generate_envelope_report(db, envelope.id, Some(5)).await?;

        // Build response
        let mut response = format!("📋 **Envelope: {}**\n\n", envelope.name);
        writeln!(&mut response, "💰 Balance: ${:.2}", envelope_report.balance)?;
        writeln!(
            &mut response,
            "📅 Monthly Allocation: ${:.2}",
            envelope_report.allocation
        )?;
        writeln!(&mut response, "📊 Category: {}\n", envelope.category)?;
        writeln!(
            &mut response,
            "👥 Type: {}",
            if envelope.is_individual {
                "Individual"
            } else {
                "Shared"
            }
        )?;
        writeln!(
            &mut response,
            "🔄 Rollover: {}",
            if envelope.rollover {
                "Enabled"
            } else {
                "Disabled"
            }
        )?;
        writeln!(&mut response)?;

        let progress_bar = report::format_progress_bar(envelope_report.progress_percent, Some(15));
        writeln!(
            &mut response,
            "**Progress:** {} {:.1}%",
            progress_bar, envelope_report.progress_percent
        )?;
        writeln!(
            &mut response,
            "Spent: ${:.2} | Remaining: ${:.2}",
            envelope_report.amount_spent, envelope_report.amount_remaining
        )?;
        writeln!(&mut response)?;

        if envelope_report.recent_transactions.is_empty() {
            response.push_str("_No recent transactions_\n");
        } else {
            response.push_str("**Recent Transactions:**\n");
            for txn in envelope_report.recent_transactions {
                let summary = report::format_transaction_summary(&txn);
                writeln!(&mut response, "• {summary}")?;
            }
        }

        ctx.say(response).await?;
        Ok(())
    }

    /// Lists all active envelopes in the system.
    ///
    /// This command shows a summary of all available envelopes with their
    /// balances and allocations, helping users see what envelopes exist.
    #[poise::command(slash_command, prefix_command)]
    pub async fn envelopes(ctx: poise::Context<'_, BotData, Error>) -> Result<()> {
        let db = &ctx.data().database;

        let all_envelopes = envelope::get_all_active_envelopes(db).await?;

        if all_envelopes.is_empty() {
            ctx.say("📂 No envelopes found. Create one with `/create_envelope` to get started!")
                .await?;
            return Ok(());
        }

        let mut response = String::from("📂 **All Envelopes**\n\n");

        for env in all_envelopes {
            let type_indicator = if env.is_individual { "👤" } else { "👥" };
            writeln!(
                &mut response,
                "{} **{}** - ${:.2} / ${:.2} ({})",
                type_indicator, env.name, env.balance, env.allocation, env.category
            )?;
        }

        ctx.say(response).await?;
        Ok(())
    }

    /// Creates a new envelope for budget tracking.
    ///
    /// This command creates a new envelope with the specified name, category, and allocation.
    /// Envelopes can be either shared (accessible by all users) or individual (user-specific).
    #[poise::command(slash_command, prefix_command)]
    pub async fn create_envelope(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope"] name: String,
        #[description = "Budget category (e.g., 'groceries', 'entertainment')"] category: String,
        #[description = "Monthly allocation amount"] allocation: f64,
        #[description = "Is this an individual envelope? (default: false)"] is_individual: Option<
            bool,
        >,
        #[description = "Enable rollover? (default: false)"] rollover: Option<bool>,
    ) -> Result<()> {
        let db = &ctx.data().database;

        if allocation.is_nan() || allocation.is_infinite() {
            ctx.say("❌ Invalid allocation: must be a valid number")
                .await?;
            return Ok(());
        }
        if allocation < 0.0 {
            ctx.say("❌ Allocation must be a non-negative number.")
                .await?;
            return Ok(());
        }

        let user_id = if is_individual.unwrap_or(false) {
            Some(ctx.author().id.to_string())
        } else {
            None
        };

        // Check if envelope already exists
        let existing = if let Some(uid) = &user_id {
            envelope::get_envelope_by_name_and_user(db, &name, uid).await?
        } else {
            envelope::get_shared_envelope_by_name(db, &name).await?
        };

        if existing.is_some() {
            ctx.say(&format!(
                "❌ Envelope '{name}' already exists. Use a different name or delete the existing envelope first.",
            ))
            .await?;
            return Ok(());
        }

        // Create the envelope
        let new_envelope = envelope::create_envelope(
            db,
            name.clone(),
            user_id,
            category.clone(),
            allocation,
            is_individual.unwrap_or(false),
            rollover.unwrap_or(false),
        )
        .await?;

        let type_str = if new_envelope.is_individual {
            "individual"
        } else {
            "shared"
        };
        let rollover_str = if new_envelope.rollover {
            "with rollover enabled"
        } else {
            "without rollover"
        };

        ctx.say(&format!(
            "✅ Created {type_str} envelope **{name}** in category '{category}' with ${allocation:.2} monthly allocation {rollover_str}!"
        ))
        .await?;

        Ok(())
    }

    /// Deletes an envelope from the system.
    ///
    /// This command performs a soft delete on the envelope, marking it as deleted
    /// while preserving historical transaction data. The envelope will no longer
    /// appear in reports or be available for new transactions.
    #[poise::command(slash_command, prefix_command)]
    pub async fn delete_envelope(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope to delete"] name: String,
        #[description = "User ID (for individual envelopes)"] user: Option<String>,
    ) -> Result<()> {
        let db = &ctx.data().database;
        let user_id = user.unwrap_or_else(|| ctx.author().id.to_string());

        // Try to find the envelope - first check user's individual envelope, then shared
        let envelope = if let Some(env) =
            envelope::get_envelope_by_name_and_user(db, &name, &user_id).await?
        {
            Some(env)
        } else {
            envelope::get_shared_envelope_by_name(db, &name).await?
        };

        let Some(envelope) = envelope else {
            ctx.say(&format!("❌ Envelope '{name}' not found.")).await?;
            return Ok(());
        };

        // Perform soft delete
        let mut active_model: crate::entities::envelope::ActiveModel = envelope.clone().into();
        active_model.is_deleted = sea_orm::ActiveValue::Set(true);
        active_model.update(db).await?;

        ctx.say(&format!(
            "✅ Deleted envelope **{name}**. Historical transaction data has been preserved.",
        ))
        .await?;

        Ok(())
    }

    /// Updates an envelope's allocation or settings.
    ///
    /// This command allows modifying an existing envelope's monthly allocation,
    /// rollover setting, or category without creating a new envelope.
    #[poise::command(slash_command, prefix_command)]
    pub async fn update_envelope(
        ctx: poise::Context<'_, BotData, Error>,
        #[description = "Name of the envelope to update"] name: String,
        #[description = "New monthly allocation (optional)"] allocation: Option<f64>,
        #[description = "Enable/disable rollover (optional)"] rollover: Option<bool>,
        #[description = "New category (optional)"] category: Option<String>,
        #[description = "User ID (for individual envelopes)"] user: Option<String>,
    ) -> Result<()> {
        let db = &ctx.data().database;
        let user_id = user.unwrap_or_else(|| ctx.author().id.to_string());

        if allocation.is_none() && rollover.is_none() && category.is_none() {
            ctx.say(
                "❌ Please specify at least one field to update (allocation, rollover, or category).",
            )
            .await?;
            return Ok(());
        }

        if let Some(alloc) = allocation {
            if alloc.is_nan() || alloc.is_infinite() {
                ctx.say("❌ Invalid allocation: must be a valid number")
                    .await?;
                return Ok(());
            }
            if alloc < 0.0 {
                ctx.say("❌ Allocation must be a non-negative number.")
                    .await?;
                return Ok(());
            }
        }

        // Try to find the envelope - first check user's individual envelope, then shared
        let envelope = if let Some(env) =
            envelope::get_envelope_by_name_and_user(db, &name, &user_id).await?
        {
            Some(env)
        } else {
            envelope::get_shared_envelope_by_name(db, &name).await?
        };

        let Some(envelope) = envelope else {
            ctx.say(&format!("❌ Envelope '{name}' not found.")).await?;
            return Ok(());
        };

        // Update the envelope
        let mut active_model: crate::entities::envelope::ActiveModel = envelope.into();

        let mut changes = Vec::new();

        if let Some(alloc) = allocation {
            active_model.allocation = sea_orm::ActiveValue::Set(alloc);
            changes.push(format!("allocation to ${alloc:.2}"));
        }
        if let Some(roll) = rollover {
            active_model.rollover = sea_orm::ActiveValue::Set(roll);
            changes.push(format!(
                "rollover to {}",
                if roll { "enabled" } else { "disabled" }
            ));
        }
        if let Some(ref cat) = category {
            active_model.category = sea_orm::ActiveValue::Set(cat.clone());
            changes.push(format!("category to '{cat}'"));
        }

        let _updated = active_model.update(db).await?;

        ctx.say(&format!(
            "✅ Updated envelope **{}**: {}",
            name,
            changes.join(", ")
        ))
        .await?;

        Ok(())
    }
}

// Re-export all commands
pub use inner::*;
