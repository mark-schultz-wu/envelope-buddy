use crate::errors::{Error, Result};
use serde::Deserialize;
use std::{env, fs, path::Path};
use tracing::{debug, error, info, instrument}; // Add instrument

#[derive(Deserialize, Debug, Clone)]
pub struct EnvelopeConfig {
    pub name: String,
    pub category: String, // Consider making this an enum: "necessary" or "quality_of_life"
    pub allocation: f64,
    pub is_individual: bool,
    #[serde(default)] // For config.toml, user_id might be absent for shared envelopes
    pub user_id: Option<String>, // Only relevant if is_individual is true
    pub rollover: bool,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub envelopes_from_toml: Vec<EnvelopeConfig>,
    pub user_id_1: String,
    pub user_id_2: String,
    pub user_nickname_1: String,
    pub user_nickname_2: String,
    pub database_path: String,
}

// Helper struct to represent the root structure of your config.toml
#[derive(Deserialize, Debug)]
struct TomlConfigFile {
    // This field name 'envelopes' MUST match the [[envelopes]] key in your TOML file.
    envelopes: Vec<EnvelopeConfig>,
}

// This function loads the Vec<EnvelopeConfig>
pub fn load_config_file_data<P: AsRef<Path>>(path: P) -> Result<Vec<EnvelopeConfig>> {
    let path_ref = path.as_ref();
    debug!(
        "Attempting to load envelope configurations from: {:?}",
        path_ref
    );
    let contents = fs::read_to_string(path_ref)
        .map_err(|e| Error::Config(format!("Failed to read config file {:?}: {}", path_ref, e)))?;

    // Parse into the TomlConfigFile struct first
    let parsed_toml_root: TomlConfigFile = toml::from_str(&contents).map_err(|e| {
        Error::Config(format!(
            "Failed to parse TOML from config file {:?}: {}",
            path_ref, e
        ))
    })?;

    // Then return the Vec<EnvelopeConfig> from that struct
    Ok(parsed_toml_root.envelopes)
}

#[instrument] // Instrument the whole loading process
pub fn load_app_configuration() -> Result<AppConfig> {
    info!("Loading application configuration...");

    // 1. Load .env variables (dotenv() should be called in main ideally, but env::var can be used here)
    let user_id_1 = env::var("COUPLE_USER_ID_1")
        .inspect_err(|e| error!("COUPLE_USER_ID_1 not found in environment: {}", e))
        .map_err(Error::EnvVar)?;
    info!("COUPLE_USER_ID_1 loaded.");

    let user_id_2 = env::var("COUPLE_USER_ID_2")
        .inspect_err(|e| error!("COUPLE_USER_ID_2 not found in environment: {}", e))
        .map_err(Error::EnvVar)?;
    info!("COUPLE_USER_ID_2 loaded.");

    let user_nickname_1 = env::var("USER_NICKNAME_1").unwrap_or_else(|_| {
        info!("USER_NICKNAME_1 not found, using default 'User1'.");
        "User1".to_string()
    });

    let user_nickname_2 = env::var("USER_NICKNAME_2").unwrap_or_else(|_| {
        info!("USER_NICKNAME_2 not found, using default 'User2'.");
        "User2".to_string()
    });

    let database_path = env::var("DATABASE_PATH")
        .inspect_err(|e| error!("DATABASE_PATH not found in environment: {}", e))
        .map_err(Error::EnvVar)?;
    info!("DATABASE_PATH loaded.");

    // 2. Load from config.toml
    let envelopes_from_toml = load_config_file_data("config.toml")
        .inspect_err(|e| error!("Error detail from load_config_file_data: {}", e))?; // Log specific error from file loading
    info!(
        "{} envelope configurations parsed from TOML.",
        envelopes_from_toml.len()
    );

    let app_config = AppConfig {
        envelopes_from_toml,
        user_id_1,
        user_id_2,
        user_nickname_1,
        user_nickname_2,
        database_path,
    };

    info!("Application configuration loaded successfully.");
    Ok(app_config)
}
