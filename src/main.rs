mod bot;
mod commands;
mod config;
mod db;
mod errors;
mod models;

use crate::errors::{Error, Result};
use dotenvy::dotenv;
use std::path::Path;
use std::{env, sync::Arc};
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, Registry, fmt, prelude::*};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Load .env file (as early as possible)
    dotenv().ok();

    // Vector to store pre-initialization messages
    let mut setup_messages: Vec<String> = Vec::new();

    // 2. Prepare EnvFilter
    let env_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => {
            setup_messages.push(
                "Log filter successfully determined from RUST_LOG environment variable."
                    .to_string(),
            );
            filter
        }
        Err(e) => {
            setup_messages.push(format!(
                "[WARNING] Failed to parse RUST_LOG environment variable: {}. Falling back to default 'info' log filter.",
                e
            ));
            EnvFilter::new("info") // Default filter
        }
    };

    // 3. Determine Log Directory Path & Ensure it Exists
    let log_directory_path_str = match env::var("LOG_DIRECTORY_PATH") {
        Ok(path) => {
            setup_messages.push(format!(
                "Using log directory path from LOG_DIRECTORY_PATH environment variable: '{}'",
                path
            ));
            path
        }
        Err(e) => {
            let default_path = "./logs".to_string();
            if e == env::VarError::NotPresent {
                setup_messages.push(format!(
                    "[WARNING] LOG_DIRECTORY_PATH not set in .env, using default '{}'.",
                    default_path
                ));
            } else {
                setup_messages.push(format!(
                    "[ERROR] Error reading LOG_DIRECTORY_PATH from .env: {}. Using default '{}'.",
                    e, default_path
                ));
            }
            default_path
        }
    };
    let log_directory = Path::new(&log_directory_path_str);

    if !log_directory.exists() {
        setup_messages.push(format!(
            "Log directory '{}' does not exist, attempting to create it.",
            log_directory.display()
        ));
        if let Err(io_err) = std::fs::create_dir_all(log_directory) {
            // If creating the log directory fails, we have a problem.
            // We can't log to the file, so we might have to resort to eprintln! here or panic.
            // Or, we could try to log only to stdout if file logging fails.
            // For now, let's convert to Error::Config and let it propagate.
            // The tracing init will likely fail if the writer can't be created.
            return Err(Error::Config(format!(
                "[CRITICAL] Failed to create log directory '{}': {}. Cannot initialize file logging.",
                log_directory.display(),
                io_err
            )));
        }
    }

    // 4. Initialize tracing subscriber
    let file_appender = tracing_appender::rolling::daily(log_directory, "envelope_buddy.log");
    let (non_blocking_file_writer, _guard) = tracing_appender::non_blocking(file_appender);

    Registry::default()
        .with(env_filter)
        .with(
            fmt::Layer::new()
                .with_writer(std::io::stdout)
                .with_ansi(true),
        )
        .with(
            fmt::Layer::new()
                .with_writer(non_blocking_file_writer)
                .with_ansi(false),
        )
        .try_init()
        .map_err(|e| Error::Config(format!("Failed to initialize tracing subscriber: {}", e)))?;

    // --- Tracing is now initialized. Log collected setup messages. ---
    for msg in setup_messages {
        if msg.starts_with("[ERROR]") || msg.starts_with("[CRITICAL]") {
            error!("{}", msg);
        } else if msg.starts_with("[WARNING]") {
            warn!("{}", msg);
        } else {
            info!("{}", msg);
        }
    }

    info!(
        "Tracing initialized. Logging to stdout and daily rolling files in '{}'.",
        log_directory.display()
    );
    // The message about which filter is being used is now part of setup_messages.

    // ... (rest of your main function) ...
    let app_config = config::load_app_configuration()?;
    info!("Successfully processed application configuration.");

    let db_pool = db::init_db(&app_config.database_path)
        .await
        .inspect(|_| info!("Database initialized successfully."))
        .inspect_err(|e| error!("Failed to initialize database: {}", e))?;
    let arc_app_config = Arc::new(app_config);

    db::seed_initial_envelopes(&db_pool, &arc_app_config)
        .await
        .inspect(|_| info!("Initial envelopes seeded successfully."))
        .inspect_err(|e| error!("Failed to seed initial envelopes: {}", e))?;

    let token = env::var("DISCORD_BOT_TOKEN")
        .inspect_err(|e| error!("DISCORD_BOT_TOKEN not found: {}", e))
        .map_err(Error::EnvVar)?;

    bot::run_bot(token, Arc::clone(&arc_app_config), db_pool)
        .await
        .map_err(Error::from)?;
    Ok(())
}
