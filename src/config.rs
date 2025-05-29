use crate::errors::{Error, Result};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize, Debug)]
pub struct AppConfig {
    #[allow(dead_code)]
    pub envelopes: Vec<EnvelopeConfig>,
    // Add other general bot configurations here if needed
    // pub discord_channel_id: Option<String>, // Example
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct EnvelopeConfig {
    pub name: String,
    pub category: String, // Consider making this an enum: "necessary" or "quality_of_life"
    pub allocation: f64,
    pub is_individual: bool,
    #[serde(default)] // For config.toml, user_id might be absent for shared envelopes
    pub user_id: Option<String>, // Only relevant if is_individual is true
    pub rollover: bool,
}

pub fn load_config<P: AsRef<Path>>(path: P) -> Result<AppConfig> {
    let path_ref = path.as_ref();
    tracing::debug!("Attempting to load configuration from: {:?}", path_ref);
    let contents = fs::read_to_string(path_ref)
        .map_err(|e| Error::Config(format!("Failed to read config file {:?}: {}", path_ref, e)))?;
    let app_config: AppConfig = toml::from_str(&contents).map_err(|e| {
        Error::Config(format!(
            "Failed to parse TOML from config file {:?}: {}",
            path_ref, e
        ))
    })?;
    Ok(app_config)
}
