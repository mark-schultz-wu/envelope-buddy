use crate::config::AppConfig;
use crate::db::DbPool;
use crate::errors::{Error, Result};
use crate::models::Envelope;
use rusqlite::Error as RusqliteError;
use rusqlite::{OptionalExtension, params};
use std::sync::Arc;
use tracing::{debug, error, info, instrument, trace, warn};

#[instrument(skip(pool, config))]
pub async fn seed_initial_envelopes(pool: &DbPool, config: &Arc<AppConfig>) -> Result<()> {
    info!(
        "Starting to seed initial envelopes. Found {} configurations from TOML.",
        config.envelopes_from_toml.len()
    );
    let mut conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for seeding".to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;

    for cfg_envelope in &config.envelopes_from_toml {
        debug!(
            "Processing config for envelope: '{}', individual: {}",
            cfg_envelope.name, cfg_envelope.is_individual
        );

        if cfg_envelope.is_individual {
            let user_ids_to_process = [&config.user_id_1, &config.user_id_2];
            for current_user_id_str in user_ids_to_process.iter() {
                let current_user_id: &str = current_user_id_str;

                // 1. Check for an ACTIVE envelope first
                let mut stmt_check_active = tx.prepare_cached(
                    "SELECT id FROM envelopes WHERE name = ?1 AND user_id = ?2 AND is_deleted = FALSE",
                )?;
                let active_exists: Option<i64> = stmt_check_active
                    .query_row(params![cfg_envelope.name, current_user_id], |row| {
                        row.get(0)
                    })
                    .optional()?;

                if active_exists.is_some() {
                    warn!(
                        "ACTIVE individual envelope '{}' for user {} already exists. Skipping.",
                        cfg_envelope.name, current_user_id
                    );
                    continue; // Move to the next user_id or config
                }

                // 2. No active one found, check for a SOFT-DELETED envelope
                let mut stmt_check_deleted = tx.prepare_cached(
                    "SELECT id FROM envelopes WHERE name = ?1 AND user_id = ?2 AND is_deleted = TRUE",
                )?;
                let deleted_envelope_id: Option<i64> = stmt_check_deleted
                    .query_row(params![cfg_envelope.name, current_user_id], |row| {
                        row.get(0)
                    })
                    .optional()?;

                if let Some(id_to_reenable) = deleted_envelope_id {
                    info!(
                        "Found soft-deleted individual envelope '{}' for user {}. Re-enabling and updating.",
                        cfg_envelope.name, current_user_id
                    );
                    let mut stmt_update = tx.prepare_cached(
                        "UPDATE envelopes SET category = ?1, allocation = ?2, balance = ?2, rollover = ?3, is_deleted = FALSE
                         WHERE id = ?4", // balance reset to new allocation
                    )?;
                    stmt_update.execute(params![
                        cfg_envelope.category,
                        cfg_envelope.allocation,
                        cfg_envelope.rollover,
                        id_to_reenable,
                    ])?;
                } else {
                    // 3. Neither active nor soft-deleted found, INSERT NEW
                    info!(
                        "Inserting NEW individual envelope '{}' for user {}",
                        cfg_envelope.name, current_user_id
                    );
                    let mut stmt_insert = tx.prepare_cached(
                        "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                         VALUES (?1, ?2, ?3, ?3, TRUE, ?4, ?5, FALSE)", // balance set to allocation
                    )?;
                    stmt_insert.execute(params![
                        cfg_envelope.name,
                        cfg_envelope.category,
                        cfg_envelope.allocation,
                        current_user_id,
                        cfg_envelope.rollover,
                    ])?;
                }
            }
        } else {
            // Shared envelope
            // 1. Check for an ACTIVE shared envelope
            let mut stmt_check_active = tx.prepare_cached(
                "SELECT id FROM envelopes WHERE name = ?1 AND user_id IS NULL AND is_deleted = FALSE",
            )?;
            let active_exists: Option<i64> = stmt_check_active
                .query_row(params![cfg_envelope.name], |row| row.get(0))
                .optional()?;

            if active_exists.is_some() {
                warn!(
                    "ACTIVE shared envelope '{}' already exists. Skipping.",
                    cfg_envelope.name
                );
                continue; // Move to the next config
            }

            // 2. No active one, check for SOFT-DELETED shared envelope
            let mut stmt_check_deleted = tx.prepare_cached(
                "SELECT id FROM envelopes WHERE name = ?1 AND user_id IS NULL AND is_deleted = TRUE",
            )?;
            let deleted_envelope_id: Option<i64> = stmt_check_deleted
                .query_row(params![cfg_envelope.name], |row| row.get(0))
                .optional()?;

            if let Some(id_to_reenable) = deleted_envelope_id {
                info!(
                    "Found soft-deleted shared envelope '{}'. Re-enabling and updating.",
                    cfg_envelope.name
                );
                let mut stmt_update = tx.prepare_cached(
                    "UPDATE envelopes SET category = ?1, allocation = ?2, balance = ?2, rollover = ?3, is_deleted = FALSE
                     WHERE id = ?4", // balance reset to new allocation
                )?;
                stmt_update.execute(params![
                    cfg_envelope.category,
                    cfg_envelope.allocation,
                    cfg_envelope.rollover,
                    id_to_reenable,
                ])?;
            } else {
                // 3. Neither active nor soft-deleted, INSERT NEW shared envelope
                info!("Inserting NEW shared envelope '{}'", cfg_envelope.name);
                let mut stmt_insert = tx.prepare_cached(
                    "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
                     VALUES (?1, ?2, ?3, ?3, FALSE, NULL, ?4, FALSE)", // balance set to allocation
                )?;
                stmt_insert.execute(params![
                    cfg_envelope.name,
                    cfg_envelope.category,
                    cfg_envelope.allocation,
                    cfg_envelope.rollover,
                ])?;
            }
        }
    }

    tx.commit()
        .map_err(|e| Error::Database(format!("Failed to commit transaction for seeding: {}", e)))?;
    info!("Finished seeding initial envelopes with re-enable logic.");
    Ok(())
}

#[instrument(skip(pool))]
pub async fn get_all_active_envelopes(pool: &DbPool) -> Result<Vec<Envelope>> {
    let conn = pool.lock().map_err(|_| {
        Error::Database("Failed to acquire DB lock for getting envelopes".to_string())
    })?;

    let mut stmt = conn.prepare_cached("SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted FROM envelopes WHERE is_deleted = FALSE ORDER BY name, user_id")?;

    let envelope_iter = stmt.query_map([], |row| {
        Ok(Envelope {
            id: row.get(0)?,
            name: row.get(1)?,
            category: row.get(2)?,
            allocation: row.get(3)?,
            balance: row.get(4)?,
            is_individual: row.get(5)?,
            user_id: row.get(6)?,
            rollover: row.get(7)?,
            is_deleted: row.get(8)?,
        })
    })?;

    let mut envelopes = Vec::new();
    for envelope_result in envelope_iter {
        envelopes.push(envelope_result.map_err(|e: RusqliteError| {
            Error::Database(format!("Failed to map envelope row: {}", e))
        })?);
    }

    debug!("Fetched {} active envelopes.", envelopes.len());
    Ok(envelopes)
}

#[instrument(skip(pool))]
pub async fn get_user_or_shared_envelope(
    pool: &DbPool,
    envelope_name: &str,
    user_id: &str, // The Discord User ID of the person trying to spend
) -> Result<Option<Envelope>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;

    // Try to find an individual envelope for this user OR a shared envelope
    // Prioritize individual envelope if names could clash (though our naming rules might prevent this)
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted
         FROM envelopes
         WHERE name = ?1 AND (user_id = ?2 OR user_id IS NULL) AND is_deleted = FALSE
         ORDER BY user_id DESC", // This prioritizes the user's specific envelope if a shared one has the same name
    )?;

    let envelope_result: Option<Envelope> = stmt
        .query_row(params![envelope_name, user_id], |row| {
            Ok(Envelope {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                allocation: row.get(3)?,
                balance: row.get(4)?,
                is_individual: row.get(5)?,
                user_id: row.get(6)?,
                rollover: row.get(7)?,
                is_deleted: row.get(8)?,
            })
        })
        .optional()?; // Handles case where no envelope is found

    if envelope_result.is_some()
        && envelope_result.as_ref().unwrap().is_individual
        && envelope_result.as_ref().unwrap().user_id.as_deref() != Some(user_id)
    {
        // This case should ideally not happen if the query is correct and names are unique per user for individual types,
        // or if a shared envelope has the same name as an individual one (which is disallowed by your naming rules).
        // The query with "ORDER BY user_id DESC" (NULLs last) should pick the user's own first.
        // If it picked a shared one despite user_id matching, something is off or names aren't unique.
        // For now, we trust the query to fetch the correct one if it exists.
    }

    debug!(
        "Envelope lookup for '{}' for user '{}': {:?}",
        envelope_name,
        user_id,
        envelope_result.as_ref().map(|e| &e.name)
    );
    Ok(envelope_result)
}

#[instrument(skip(pool))]
pub async fn get_envelope_by_id(pool: &DbPool, envelope_id: i64) -> Result<Option<Envelope>> {
    let conn = pool.lock().map_err(|_| {
        Error::Database("Failed to acquire DB lock for get_envelope_by_id".to_string())
    })?;
    let mut stmt = conn.prepare_cached(
        "SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted
         FROM envelopes WHERE id = ?1 AND is_deleted = FALSE", // Only fetch active envelopes
    )?;
    let envelope_result = stmt
        .query_row(params![envelope_id], |row| {
            Ok(Envelope {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                allocation: row.get(3)?,
                balance: row.get(4)?,
                is_individual: row.get(5)?,
                user_id: row.get(6)?,
                rollover: row.get(7)?,
                is_deleted: row.get(8)?,
            })
        })
        .optional()?; // Handles case where no envelope with this ID is found or if it's deleted

    Ok(envelope_result)
}

#[instrument(skip(pool))]
pub async fn update_envelope_balance(
    pool: &DbPool,
    envelope_id: i64,
    new_balance: f64,
) -> Result<()> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    conn.execute(
        "UPDATE envelopes SET balance = ?1 WHERE id = ?2",
        params![new_balance, envelope_id],
    )?;
    info!(
        "Updated balance for envelope_id {}: new_balance = {}",
        envelope_id, new_balance
    );
    Ok(())
}

#[instrument(skip(pool))]
pub async fn soft_delete_envelope(
    pool: &DbPool,
    envelope_name: &str,
    user_id: &str, // The Discord User ID of the person trying to delete
) -> Result<bool> {
    // Returns true if an envelope was deleted, false otherwise
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for delete".to_string()))?;

    // Find the envelope: either a shared one by this name, or the user's individual one.
    // Prioritize the user's individual envelope if names could clash.
    let mut stmt_find = conn.prepare_cached(
        "SELECT id, user_id as owner_id, is_individual FROM envelopes
         WHERE name = ?1 AND (user_id = ?2 OR user_id IS NULL) AND is_deleted = FALSE
         ORDER BY user_id DESC LIMIT 1", // User's own DESC (not NULL) comes before NULL
    )?;

    struct FoundEnvelope {
        id: i64,
        owner_id: Option<String>,
        is_individual: bool,
    }

    let envelope_to_delete_result: Option<FoundEnvelope> = stmt_find
        .query_row(params![envelope_name, user_id], |row| {
            Ok(FoundEnvelope {
                id: row.get(0)?,
                owner_id: row.get(1)?,
                is_individual: row.get(2)?,
            })
        })
        .optional()?;

    if let Some(envelope_data) = envelope_to_delete_result {
        // If it's an individual envelope, ensure the deleter is the owner.
        // If it's shared, anyone in the couple (who can issue commands) can delete it.
        if envelope_data.is_individual && envelope_data.owner_id.as_deref() != Some(user_id) {
            warn!(
                "User {} attempted to delete individual envelope '{}' belonging to someone else ({}). Denied.",
                user_id,
                envelope_name,
                envelope_data.owner_id.as_deref().unwrap_or("unknown")
            );
            return Ok(false); // Or return an error indicating permission denied
        }

        let rows_affected = conn.execute(
            "UPDATE envelopes SET is_deleted = TRUE WHERE id = ?1 AND is_deleted = FALSE",
            params![envelope_data.id],
        )?;

        if rows_affected > 0 {
            info!(
                "Soft-deleted envelope '{}' (ID: {}) by user {}",
                envelope_name, envelope_data.id, user_id
            );
            return Ok(true);
        } else {
            // This case (found but not deleted) would be rare if the select query included is_deleted = FALSE
            warn!(
                "Envelope '{}' (ID: {}) was found but not soft-deleted (already deleted or race condition?).",
                envelope_name, envelope_data.id
            );
            return Ok(false);
        }
    } else {
        info!(
            "No active envelope named '{}' found for user {} to delete.",
            envelope_name, user_id
        );
        return Ok(false); // No active envelope found to delete
    }
}

// Argument struct for creating/updating an envelope instance with optional fields
#[derive(Debug)]
pub struct EnvelopeInstanceOptionalArgs<'a> {
    pub name: &'a str,
    pub category: Option<&'a str>,
    pub allocation: Option<f64>,
    pub is_individual: Option<bool>,
    pub user_id: Option<&'a str>, // For specific instance (User1, User2, or None for shared)
    pub rollover: Option<bool>,
}

#[instrument(skip(tx, args))]
fn manage_envelope_instance_in_transaction(
    tx: &rusqlite::Transaction,
    args: &EnvelopeInstanceOptionalArgs<'_>, // user_id in args determines if we are acting on a shared or specific user's individual envelope
) -> Result<String> {
    // Determine the target is_individual status based on args.user_id (more reliable for lookup)
    let target_is_individual_type = args.user_id.is_some();
    debug!(
        ?args,
        "manage_envelope_instance_in_transaction called. Target individual type: {}",
        target_is_individual_type
    );

    // 1. Check for an ACTIVE envelope first
    let mut stmt_check_active = tx.prepare_cached(
        "SELECT id FROM envelopes WHERE name = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND is_individual = ?3 AND is_deleted = FALSE",
    )?;
    let active_exists_id: Option<i64> = stmt_check_active // Renamed to avoid conflict
        .query_row(
            params![args.name, args.user_id, target_is_individual_type],
            |row| row.get(0),
        )
        .optional()?;
    debug!(
        "Active check for name='{}', user_id={:?}, is_individual={}: Found ID {:?}",
        args.name, args.user_id, target_is_individual_type, active_exists_id
    );

    if active_exists_id.is_some() {
        let msg = format!(
            "ACTIVE envelope '{}' for user {:?} (individual: {}) already exists.",
            args.name,
            args.user_id.unwrap_or("Shared"),
            target_is_individual_type
        );
        warn!("{}", msg);
        return Ok(format!("{} (Skipped).", msg));
    }

    // 2. No active one, check for a SOFT-DELETED envelope
    let mut stmt_get_deleted = tx.prepare_cached(
        "SELECT id, category, allocation, is_individual, rollover FROM envelopes
         WHERE name = ?1 AND IFNULL(user_id, '') = IFNULL(?2, '') AND is_individual = ?3 AND is_deleted = TRUE",
    )?;
    let deleted_envelope_data: Option<(i64, String, f64, bool, bool)> = stmt_get_deleted
        .query_row(
            params![args.name, args.user_id, target_is_individual_type],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    debug!(
        "Soft-deleted check for name='{}', user_id={:?}, is_individual={}: Found data {:?}",
        args.name,
        args.user_id,
        target_is_individual_type,
        deleted_envelope_data.as_ref().map(|(id, _, _, _, _)| id)
    );

    if let Some((
        id_to_reenable,
        old_category,
        old_allocation,
        existing_is_individual_db,
        old_rollover,
    )) = deleted_envelope_data
    {
        // CRITICAL: The is_individual status is taken from the DB record.
        // The args.is_individual (if Some) should match target_is_individual_type, which comes from args.user_id presence.
        // This ensures we found the correct type of envelope (shared vs individual for a specific user).
        if args.is_individual.is_some() && args.is_individual.unwrap() != existing_is_individual_db
        {
            // This case should be rare if the SELECT query correctly used target_is_individual_type.
            // It indicates an attempt to change the fundamental shared/individual nature on re-enable, which we are disallowing.
            let msg = format!(
                "Error: Found soft-deleted envelope '{}' for user {:?} with is_individual={}, but tried to re-enable with is_individual={}. This change is not allowed.",
                args.name,
                args.user_id.unwrap_or("Shared"),
                existing_is_individual_db,
                args.is_individual.unwrap()
            );
            error!("{}", msg);
            return Err(Error::Command(msg));
        }

        let category = args.category.unwrap_or(&old_category);
        let allocation = args.allocation.unwrap_or(old_allocation);
        let rollover = args.rollover.unwrap_or(old_rollover);
        let new_balance = allocation; // Reset balance to new/effective allocation

        info!(
            "Re-enabling soft-deleted envelope '{}' for {:?} (individual: {}) with updated attributes.",
            args.name,
            args.user_id.unwrap_or("Shared"),
            existing_is_individual_db
        );
        let mut stmt_update = tx.prepare_cached(
            "UPDATE envelopes SET category = ?1, allocation = ?2, balance = ?3, rollover = ?4, is_deleted = FALSE
             WHERE id = ?5", // is_individual is NOT updated from args here, it's fixed from DB.
        )?;
        stmt_update.execute(params![
            category,
            allocation,
            new_balance,
            rollover,
            id_to_reenable,
        ])?;
        debug!("Re-enabling envelope ID: {}", id_to_reenable);
        return Ok(format!(
            "Re-enabled and updated envelope '{}' for {:?} (individual: {}).",
            args.name,
            args.user_id.unwrap_or("Shared"),
            existing_is_individual_db
        ));
    } else {
        // 3. Neither active nor soft-deleted found, INSERT NEW
        // For a new envelope, is_individual status comes from args (defaulting if None)
        debug!("No active or soft-deleted found. Proceeding to insert new.");
        let is_individual_for_new = args.is_individual.unwrap_or(target_is_individual_type);
        if is_individual_for_new != target_is_individual_type {
            return Err(Error::Command(format!(
                "Inconsistency for new envelope '{}': command's is_individual flag ({:?}) mismatches expectation based on user_id presence ({}).",
                args.name, args.is_individual, target_is_individual_type
            )));
        }

        let category = args.category.unwrap_or("uncategorized");
        let allocation = args.allocation.unwrap_or(0.0);
        let rollover = args.rollover.unwrap_or(false);
        let new_balance = allocation;

        info!(
            "Inserting NEW envelope '{}' for {:?} (individual: {}).",
            args.name,
            args.user_id.unwrap_or("Shared"),
            is_individual_for_new
        );
        let mut stmt_insert = tx.prepare_cached(
            "INSERT INTO envelopes (name, category, allocation, balance, is_individual, user_id, rollover, is_deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, FALSE)",
        )?;
        stmt_insert.execute(params![
            args.name,
            category,
            allocation,
            new_balance,
            is_individual_for_new,
            args.user_id, // This will be None for shared, Some(id) for individual
            rollover,
        ])?;

        // Path for "Created new":
        let user_desc = args.user_id.unwrap_or("Shared"); // This makes sense.
        return Ok(format!(
            "Created new envelope '{}' for {} (individual: {}).",
            args.name,
            user_desc, // Use this
            args.is_individual.unwrap_or(target_is_individual_type)
        ));
    }
}

// New struct to group arguments for creating/flexible-updating an envelope
#[derive(Debug)] // Add Debug for easier logging if needed
pub struct CreateUpdateEnvelopeArgs<'a> {
    pub name: &'a str,
    pub category_opt: Option<&'a str>,
    pub allocation_opt: Option<f64>,
    pub is_individual_cmd_opt: Option<bool>, // The user's intent for individual status
    pub rollover_opt: Option<bool>,
}

#[instrument(skip(pool, envelope_data, config_user_id_1, config_user_id_2))]
pub async fn create_or_reenable_envelope_flexible(
    pool: &DbPool,
    envelope_data: &CreateUpdateEnvelopeArgs<'_>,
    config_user_id_1: &str,
    config_user_id_2: &str,
) -> Result<Vec<String>> {
    let mut conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| Error::Database(format!("Failed to start transaction: {}", e)))?;
    let mut results = Vec::new();

    let intended_is_individual_type = envelope_data.is_individual_cmd_opt.unwrap_or(false);

    let category = envelope_data.category_opt;
    let allocation = envelope_data.allocation_opt;
    let rollover = envelope_data.rollover_opt;
    let name = envelope_data.name;

    if intended_is_individual_type {
        // Base arguments for individual type. is_individual in the struct itself will be Some(true).
        // user_id will be filled per user.
        let base_args_for_individual = EnvelopeInstanceOptionalArgs {
            name,
            category,
            allocation,
            is_individual: Some(true), // This instance IS individual
            user_id: None,             // Placeholder, set below
            rollover,
        };
        results.push(manage_envelope_instance_in_transaction(
            &tx,
            &EnvelopeInstanceOptionalArgs {
                user_id: Some(config_user_id_1),
                ..base_args_for_individual
            },
        )?);
        results.push(manage_envelope_instance_in_transaction(
            &tx,
            &EnvelopeInstanceOptionalArgs {
                user_id: Some(config_user_id_2),
                ..base_args_for_individual
            },
        )?);
    } else {
        // Shared envelope
        let shared_args = EnvelopeInstanceOptionalArgs {
            name,
            category,
            allocation,
            is_individual: Some(false), // This instance IS shared
            user_id: None,              // No user_id for shared
            rollover,
        };
        results.push(manage_envelope_instance_in_transaction(&tx, &shared_args)?);
    }

    tx.commit()
        .map_err(|e| Error::Database(format!("Failed to commit transaction: {}", e)))?;
    Ok(results)
}

#[instrument(skip(pool))]
pub async fn suggest_accessible_envelope_names(
    pool: &DbPool,
    user_id: &str,
    partial_name: &str,
) -> Result<Vec<String>> {
    trace!(
        user_id,
        partial_name, "Attempting to suggest envelope names"
    );

    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for suggestions".to_string()))?;

    let lower_partial_name = partial_name.to_lowercase();
    let search_pattern = format!("{}%", lower_partial_name);
    trace!(
        lower_partial_name,
        search_pattern, "Constructed search pattern"
    );

    let mut stmt = conn.prepare_cached(
        "SELECT DISTINCT name FROM envelopes
         WHERE LOWER(name) LIKE ?1 AND is_deleted = FALSE
         AND (user_id = ?2 OR user_id IS NULL)
         ORDER BY name ASC
         LIMIT 25",
    )?;
    trace!("Prepared SQL statement for suggestions");

    let names_iter = stmt.query_map(params![search_pattern, user_id], |row| {
        let name_val: String = row.get(0)?;
        trace!(name_val, "Fetched name from DB row");
        Ok(name_val)
    })?;

    let mut names = Vec::new();
    for name_result in names_iter {
        match name_result {
            Ok(name) => {
                trace!(name, "Adding to suggestions list");
                names.push(name);
            }
            Err(e) => {
                tracing::warn!(error = %e, "Error mapping a row for envelope name suggestion");
                // Optionally decide if one error should stop all suggestions
            }
        }
    }
    debug!(
        "Suggested {} envelope names for user '{}' with partial '{}': {:?}",
        names.len(),
        user_id,
        partial_name,
        names
    );
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::EnvelopeConfig, db::test_utils::DirectInsertArgs};
    // Import items from the parent module (envelopes.rs)
    use std::sync::Arc;

    use crate::db::test_utils::{
        direct_insert_envelope, get_envelope_by_id_for_test, init_test_tracing, setup_test_db,
    };

    #[tokio::test]
    async fn test_seed_envelopes_and_get_all_active() -> Result<()> {
        let db_pool = setup_test_db().await?;

        let mock_envelope_configs = vec![
            EnvelopeConfig {
                name: "Groceries".to_string(),
                category: "necessary".to_string(),
                allocation: 500.0,
                is_individual: false,
                user_id: None,
                rollover: false,
            },
            EnvelopeConfig {
                name: "Hobby".to_string(),
                category: "qol".to_string(),
                allocation: 75.0,
                is_individual: true,
                user_id: None, // In EnvelopeConfig, user_id is for the config file
                rollover: true,
            },
        ];

        let app_config_for_test = Arc::new(AppConfig {
            envelopes_from_toml: mock_envelope_configs,
            user_id_1: "test_user_1_id".to_string(),
            user_id_2: "test_user_2_id".to_string(),
            user_nickname_1: "TestUser1".to_string(),
            user_nickname_2: "TestUser2".to_string(),
            database_path: String::new(), // Not used by seed logic directly
        });

        // Test seeding
        seed_initial_envelopes(&db_pool, &app_config_for_test).await?;

        // Test getting envelopes
        let envelopes = get_all_active_envelopes(&db_pool).await?;

        // Assertions
        // Should be 1 shared ("Groceries") + 2 individual ("Hobby" for User1, "Hobby" for User2)
        assert_eq!(
            envelopes.len(),
            3,
            "Expected 3 active envelopes after seeding."
        );

        // Check shared envelope
        let groceries = envelopes
            .iter()
            .find(|e| e.name == "Groceries")
            .expect("Groceries envelope not found");
        assert_eq!(groceries.allocation, 500.0);
        assert_eq!(groceries.balance, 500.0); // Initial balance should equal allocation
        assert!(!groceries.is_individual);
        assert!(groceries.user_id.is_none());

        // Check individual envelopes
        let hobby_user1 = envelopes
            .iter()
            .find(|e| e.name == "Hobby" && e.user_id == Some("test_user_1_id".to_string()))
            .expect("Hobby for User1 not found");
        assert_eq!(hobby_user1.allocation, 75.0);
        assert_eq!(hobby_user1.balance, 75.0);
        assert!(hobby_user1.is_individual);
        assert!(hobby_user1.rollover);

        let hobby_user2 = envelopes
            .iter()
            .find(|e| e.name == "Hobby" && e.user_id == Some("test_user_2_id".to_string()))
            .expect("Hobby for User2 not found");
        assert_eq!(hobby_user2.allocation, 75.0);
        assert_eq!(hobby_user2.balance, 75.0);
        assert!(hobby_user2.is_individual);
        assert!(hobby_user2.rollover);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_balance() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let app_config_for_test = Arc::new(AppConfig {
            /* ... minimal setup ... */
            envelopes_from_toml: vec![EnvelopeConfig {
                name: "Test".to_string(),
                category: "cat".to_string(),
                allocation: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
            }],
            user_id_1: "u1".to_string(),
            user_id_2: "u2".to_string(),
            user_nickname_1: "N1".to_string(),
            user_nickname_2: "N2".to_string(),
            database_path: String::new(),
        });
        seed_initial_envelopes(&db_pool, &app_config_for_test).await?;

        let envelopes_before = get_all_active_envelopes(&db_pool).await?;
        let test_env_id = envelopes_before
            .iter()
            .find(|e| e.name == "Test")
            .unwrap()
            .id;

        update_envelope_balance(&db_pool, test_env_id, 42.50).await?;

        let envelopes_after = get_all_active_envelopes(&db_pool).await?;
        let test_env_after = envelopes_after
            .iter()
            .find(|e| e.id == test_env_id)
            .unwrap();
        assert_eq!(test_env_after.balance, 42.50);

        Ok(())
    }

    #[tokio::test]
    async fn test_soft_delete_shared_envelope() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let env_id;
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: "SharedToDelete",
                category: "cat",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            };
            env_id = direct_insert_envelope(&args)?;
        }

        let deleted = soft_delete_envelope(&db_pool, "SharedToDelete", "any_user_id").await?;
        assert!(deleted, "Envelope should have been marked as deleted");

        let active_envelopes = get_all_active_envelopes(&db_pool).await?;
        assert!(
            !active_envelopes.iter().any(|e| e.id == env_id),
            "Deleted envelope should not be in active list"
        );

        // Verify is_deleted flag
        {
            let conn = db_pool.lock().unwrap();
            let env_after_delete =
                get_envelope_by_id_for_test(&conn, env_id)?.expect("Envelope should still exist");
            assert!(
                env_after_delete.is_deleted,
                "is_deleted flag should be true"
            );
        }

        // Try deleting again
        let deleted_again = soft_delete_envelope(&db_pool, "SharedToDelete", "any_user_id").await?;
        assert!(
            !deleted_again,
            "Attempting to delete an already soft-deleted envelope should return false"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_soft_delete_individual_envelope_owner() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let user1 = "user_deleter_id";
        let env_id;
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: "IndieToDelete",
                category: "cat",
                allocation: 100.0,
                balance: 100.0,
                is_individual: true,
                user_id: Some(user1),
                rollover: false,
                is_deleted: false,
            };
            env_id = direct_insert_envelope(&args)?;
        }

        let deleted = soft_delete_envelope(&db_pool, "IndieToDelete", user1).await?;
        assert!(
            deleted,
            "Envelope should have been marked as deleted by owner"
        );

        let active_envelopes = get_all_active_envelopes(&db_pool).await?;
        assert!(
            !active_envelopes
                .iter()
                .any(|e| e.id == env_id && e.user_id == Some(user1.to_string())),
            "Deleted envelope should not be in active list for user"
        );

        let conn = db_pool.lock().unwrap();
        let env_after_delete =
            get_envelope_by_id_for_test(&conn, env_id)?.expect("Envelope should still exist");
        assert!(env_after_delete.is_deleted);
        Ok(())
    }

    #[tokio::test]
    async fn test_soft_delete_individual_envelope_not_owner() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let owner_user = "owner_id";
        let other_user = "other_user_id";
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: "IndieProtected",
                category: "cat",
                allocation: 100.0,
                balance: 100.0,
                is_individual: true,
                user_id: Some(owner_user),
                rollover: false,
                is_deleted: false,
            };
            direct_insert_envelope(&args)?;
        }

        // Other user tries to delete
        let deleted = soft_delete_envelope(&db_pool, "IndieProtected", other_user).await?;
        // The current soft_delete_envelope logic finds by name AND (user_id = deleter OR user_id IS NULL)
        // So, if "IndieProtected" is only for "owner_id", other_user won't find it to delete.
        assert!(
            !deleted,
            "Should not delete an individual envelope not owned by the deleter if it's the only one with that name"
        );

        let active_envelopes = get_all_active_envelopes(&db_pool).await?;
        assert!(
            active_envelopes
                .iter()
                .any(|e| e.name == "IndieProtected" && !e.is_deleted)
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_create_new_shared_envelope_flexible() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let args = CreateUpdateEnvelopeArgs {
            name: "NewShared",
            category_opt: Some("cat_shared"),
            allocation_opt: Some(200.0),
            is_individual_cmd_opt: Some(false),
            rollover_opt: Some(true),
        };

        let results = create_or_reenable_envelope_flexible(&db_pool, &args, "u1", "u2").await?;
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("Created new envelope 'NewShared' for Shared"));

        let envelopes = get_all_active_envelopes(&db_pool).await?;
        let new_env = envelopes
            .iter()
            .find(|e| e.name == "NewShared")
            .expect("NewShared not found");
        assert!(!new_env.is_individual);
        assert_eq!(new_env.category, "cat_shared");
        assert_eq!(new_env.allocation, 200.0);
        assert_eq!(new_env.balance, 200.0); // Balance reset to allocation
        assert!(new_env.rollover);
        Ok(())
    }

    #[tokio::test]
    async fn test_create_new_individual_envelope_flexible() -> Result<()> {
        init_test_tracing();
        let db_pool = setup_test_db().await?;
        let user1 = "user_flex_1";
        let user2 = "user_flex_2";
        let args = CreateUpdateEnvelopeArgs {
            name: "NewIndieFlex",
            category_opt: Some("cat_indie"),
            allocation_opt: Some(150.0),
            is_individual_cmd_opt: Some(true),
            rollover_opt: Some(false),
        };

        let results = create_or_reenable_envelope_flexible(&db_pool, &args, user1, user2).await?;
        assert_eq!(results.len(), 2);
        // In test_create_new_individual_envelope_flexible
        let expected_message_part = format!(
            "Created new envelope '{}' for {} (individual: true).",
            "NewIndieFlex",
            user1 // user1 is "user_flex_1"
        );
        assert!(
            results[0].contains(&expected_message_part),
            "Actual message: '{}', Expected to contain: '{}'",
            results[0],
            expected_message_part
        );
        let expected_message_part_user2 = format!(
            "Created new envelope '{}' for {} (individual: true).",
            "NewIndieFlex",
            user2 // user2 is "user_flex_2"
        );
        assert!(
            results[1].contains(&expected_message_part_user2),
            "Actual message: '{}', Expected to contain: '{}'",
            results[1],
            expected_message_part_user2
        );

        let envelopes = get_all_active_envelopes(&db_pool).await?;
        assert_eq!(envelopes.len(), 2);

        let env1 = envelopes
            .iter()
            .find(|e| e.user_id == Some(user1.to_string()))
            .unwrap();
        assert_eq!(env1.name, "NewIndieFlex");
        assert_eq!(env1.allocation, 150.0);
        assert_eq!(env1.balance, 150.0);
        assert!(env1.is_individual);

        let env2 = envelopes
            .iter()
            .find(|e| e.user_id == Some(user2.to_string()))
            .unwrap();
        assert_eq!(env2.name, "NewIndieFlex");
        assert_eq!(env2.allocation, 150.0);
        assert_eq!(env2.balance, 150.0);
        assert!(env2.is_individual);
        Ok(())
    }

    #[tokio::test]
    async fn test_reenable_soft_deleted_shared_envelope_no_updates() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let env_name = "ReenableShared";
        let original_cat = "original_cat_s";
        let original_alloc = 123.0;
        let original_rollover = true;

        // 1. Create and soft-delete
        let env_id;
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: env_name,
                category: original_cat,
                allocation: original_alloc,
                balance: 50.0,
                is_individual: false,
                user_id: None,
                rollover: original_rollover,
                is_deleted: true,
            };
            env_id = direct_insert_envelope(&args)?;
        }

        // 2. Attempt to "create" (re-enable) with only name and type
        let args = CreateUpdateEnvelopeArgs {
            name: env_name,
            category_opt: None,                 // No update
            allocation_opt: None,               // No update
            is_individual_cmd_opt: Some(false), // Specify it's a shared type we're targeting
            rollover_opt: None,                 // No update
        };
        let results = create_or_reenable_envelope_flexible(&db_pool, &args, "u1", "u2").await?;
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("Re-enabled and updated envelope"));

        // 3. Verify
        let conn = db_pool.lock().unwrap();
        let reenabled_env = get_envelope_by_id_for_test(&conn, env_id)?
            .expect("Envelope not found after re-enable");
        assert!(!reenabled_env.is_deleted, "Envelope should be active");
        assert_eq!(
            reenabled_env.category, original_cat,
            "Category should not change"
        );
        assert_eq!(
            reenabled_env.allocation, original_alloc,
            "Allocation should not change"
        );
        assert_eq!(
            reenabled_env.balance, original_alloc,
            "Balance should reset to old allocation"
        );
        assert_eq!(
            reenabled_env.rollover, original_rollover,
            "Rollover should not change"
        );
        Ok(())
    }

    #[tokio::test]
    async fn test_reenable_soft_deleted_individual_with_updates() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let user1 = "user_reenable_1";
        let user2 = "user_reenable_2"; // Not used in this specific test case's setup/check, but create_or_reenable will process for them
        let env_name = "ReenableIndieUpdate";

        // 1. Create and soft-delete for user1
        let env_id_user1;
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: env_name,
                category: "old_cat_i",
                allocation: 100.0,
                balance: 20.0,
                is_individual: true,
                user_id: Some(user1),
                rollover: false,
                is_deleted: true,
            };
            env_id_user1 = direct_insert_envelope(&args)?;
            // Also insert for user2 so the re-enable logic finds something for user2 as well
            let args2: DirectInsertArgs = DirectInsertArgs {
                balance: 30.0,
                user_id: Some(user2),
                ..args
            };
            direct_insert_envelope(&args2)?;
        }

        // 2. Attempt to "create" (re-enable) with name, type, and new allocation/category
        let new_cat = "new_cat_i_updated";
        let new_alloc = 250.0;
        let new_rollover = true;
        let args = CreateUpdateEnvelopeArgs {
            name: env_name,
            category_opt: Some(new_cat),
            allocation_opt: Some(new_alloc),
            is_individual_cmd_opt: Some(true),
            rollover_opt: Some(new_rollover),
        };
        let results = create_or_reenable_envelope_flexible(&db_pool, &args, user1, user2).await?;
        assert_eq!(results.len(), 2); // Should process for both users
        assert!(
            results
                .iter()
                .all(|r| r.contains("Re-enabled and updated envelope"))
        );

        // 3. Verify for user1
        {
            let conn = db_pool.lock().unwrap();
            let reenabled_env_user1 = get_envelope_by_id_for_test(&conn, env_id_user1)?
                .expect("Envelope for user1 not found");
            assert!(!reenabled_env_user1.is_deleted);
            assert_eq!(reenabled_env_user1.user_id.as_deref(), Some(user1));
            assert_eq!(reenabled_env_user1.category, new_cat);
            assert_eq!(reenabled_env_user1.allocation, new_alloc);
            assert_eq!(reenabled_env_user1.balance, new_alloc); // Balance reset to new allocation
            assert_eq!(reenabled_env_user1.rollover, new_rollover);
        }

        // You would also verify for user2 similarly
        let env_user2 = get_all_active_envelopes(&db_pool)
            .await?
            .into_iter()
            .find(|e| e.name == env_name && e.user_id.as_deref() == Some(user2))
            .expect("Envelope for user2 not found after re-enable");
        assert!(!env_user2.is_deleted);
        assert_eq!(env_user2.category, new_cat);
        assert_eq!(env_user2.allocation, new_alloc);
        assert_eq!(env_user2.balance, new_alloc);
        assert_eq!(env_user2.rollover, new_rollover);

        Ok(())
    }

    #[tokio::test]
    async fn test_create_envelope_when_active_exists_skips() -> Result<()> {
        init_test_tracing();
        let db_pool = setup_test_db().await?;
        let env_name = "ActiveExists";
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: env_name,
                category: "cat",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            };
            direct_insert_envelope(&args)?;
        }

        let args = CreateUpdateEnvelopeArgs {
            name: env_name, // Same name as active one
            category_opt: Some("new_cat"),
            allocation_opt: Some(200.0),
            is_individual_cmd_opt: Some(false),
            rollover_opt: Some(false),
        };
        let results = create_or_reenable_envelope_flexible(&db_pool, &args, "u1", "u2").await?;
        assert_eq!(results.len(), 1);
        assert!(results[0].starts_with("ACTIVE envelope 'ActiveExists' for user \"Shared\" (individual: false) already exists."), "Message was: {}", results[0]);

        // Verify no new envelope was created and original is unchanged (except if update logic was different)
        let envelopes = get_all_active_envelopes(&db_pool).await?;
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].allocation, 100.0); // Should still be old allocation
        Ok(())
    }

    // Test for get_user_or_shared_envelope
    #[tokio::test]
    async fn test_get_user_or_shared_envelope_logic() -> Result<()> {
        let db_pool = setup_test_db().await?;
        let user1 = "test_user_fetch_1";
        let user2 = "test_user_fetch_2"; // unused in this specific test path, but good for context

        // Setup:
        // 1. Shared envelope "Common"
        // 2. User1's individual envelope "Personal"
        // 3. User1's individual envelope "Common" (to test priority)
        {
            let conn = db_pool.lock().unwrap();
            let args: DirectInsertArgs = DirectInsertArgs {
                conn: &conn,
                name: "Common",
                category: "shared",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            };
            direct_insert_envelope(&args)?;
            let args2 = DirectInsertArgs {
                name: "Personal",
                category: "indie_u1",
                allocation: 50.0,
                balance: 50.0,
                is_individual: true,
                user_id: Some(user1),
                ..args
            };
            direct_insert_envelope(&args2)?;
            let args3 = DirectInsertArgs {
                name: "Common",
                category: "indie_u1_common",
                allocation: 75.0,
                balance: 75.0,
                ..args2
            };
            direct_insert_envelope(&args3)?;
        }

        // Test Case 1: User1 asks for "Common" -> should get their individual "Common"
        let env1 = get_user_or_shared_envelope(&db_pool, "Common", user1)
            .await?
            .expect("Envelope not found");
        assert!(env1.is_individual);
        assert_eq!(env1.user_id.as_deref(), Some(user1));
        assert_eq!(env1.allocation, 75.0); // User1's "Common" allocation

        // Test Case 2: User1 asks for "Personal" -> should get their "Personal"
        let env2 = get_user_or_shared_envelope(&db_pool, "Personal", user1)
            .await?
            .expect("Envelope not found");
        assert!(env2.is_individual);
        assert_eq!(env2.user_id.as_deref(), Some(user1));
        assert_eq!(env2.allocation, 50.0);

        // Test Case 3: User2 asks for "Common" -> should get the shared "Common"
        // (because User2 doesn't have an individual "Common" in this test setup)
        let env3 = get_user_or_shared_envelope(&db_pool, "Common", user2)
            .await?
            .expect("Envelope not found");
        assert!(!env3.is_individual); // Should be the shared one
        assert!(env3.user_id.is_none());
        assert_eq!(env3.allocation, 100.0); // Shared "Common" allocation

        // Test Case 4: Ask for non-existent envelope
        let env4 = get_user_or_shared_envelope(&db_pool, "DoesNotExist", user1).await?;
        assert!(env4.is_none());

        Ok(())
    }
}
