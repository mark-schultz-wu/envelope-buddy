#![allow(clippy::result_large_err)]

mod bot;
mod cache;
mod commands;
mod config;
mod db;
mod errors;
mod models;

use crate::errors::{Error, Result};
use dotenvy::dotenv;
use std::{env, sync::Arc};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize tracing (as early as possible)
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init(); // Simplified init

    // 2. Load .env file (as early as possible)
    dotenv().ok(); // Make it non-fatal, env vars can be set externally
    info!("Attempted to load .env file.");

    // 3. Load the main application configuration
    let app_config = config::load_app_configuration()
        // .inspect_err(|e| error!("Critical error loading application configuration: {}", e)) // load_app_configuration now does internal logging
        ?;
    info!("Successfully processed application configuration."); // Or this log can be inside load_app_configuration

    // 4. Initialize database (database_path now comes from app_config)
    let db_pool = db::init_db(&app_config.database_path)
        .await
        .inspect(|_| info!("Database initialized successfully."))
        .inspect_err(|e| error!("Failed to initialize database: {}", e))?;

    // 5. Seed initial envelopes (if necessary, now takes full app_config)
    let arc_app_config = Arc::new(app_config); // Arc it for sharing
    db::seed_initial_envelopes(&db_pool, &arc_app_config)
        .await
        .inspect(|_| info!("Initial envelopes seeded successfully."))
        .inspect_err(|e| error!("Failed to seed initial envelopes: {}", e))?;

    // 6. Run the bot
    // DISCORD_BOT_TOKEN is loaded here, directly before use, not stored in AppConfig
    let token = env::var("DISCORD_BOT_TOKEN")
        .inspect_err(|e| error!("DISCORD_BOT_TOKEN not found: {}", e))
        .map_err(Error::EnvVar)?; // Convert to your app's error type

    bot::run_bot(token, Arc::clone(&arc_app_config), db_pool)
        .await // Pass token separately
        .map_err(Error::from)?;

    Ok(())
}
