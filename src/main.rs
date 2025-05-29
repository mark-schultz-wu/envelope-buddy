mod bot;
mod commands;
mod config;
mod db;
mod errors;
mod models;

use crate::errors::{Error, Result};
use dotenvy::dotenv;
use std::env;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing subscriber
    // You can use RUST_LOG env var to control logging, e.g., RUST_LOG="envelope_buddy=debug,serenity=info"
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_line_number(true) // Optional: include line numbers
        .with_file(true) // Optional: include file paths
        .init();

    info!("Starting EnvelopeBuddy...");

    // Load .env file
    dotenv().expect(".env file not found or failed to load");
    info!(".env file loaded successfully.");

    // Load configuration
    let app_config = match config::load_config("config.toml") {
        Ok(cfg) => {
            info!("Configuration loaded from config.toml");
            cfg
        }
        Err(e) => {
            error!("Failed to load config.toml: {}", e);
            return Err(e); // Propagate the error
        }
    };

    // Initialize database
    let db_path = env::var("DATABASE_PATH").expect("DATABASE_PATH must be set in .env");
    let db_pool = match db::init_db(&db_path).await {
        Ok(pool) => {
            info!("Database initialized successfully at {}", db_path);
            pool
        }
        Err(e) => {
            error!("Failed to initialize database: {}", e);
            return Err(e);
        }
    };

    // Initialize and run the Discord bot
    if let Err(why) = bot::run_bot(app_config, db_pool).await {
        error!("Bot client error: {:?}", why);
        return Err(Error::from(why));
    }

    Ok(())
}
