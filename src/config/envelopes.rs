//! Envelope configuration loading from config.toml
//!
//! This module provides functionality to load initial envelope configurations
//! from a TOML configuration file. The envelopes defined in config.toml are
//! used to seed the database on first run or when envelopes are missing.

use crate::errors::{Error, Result};
use serde::Deserialize;
use std::path::Path;

/// Configuration structure representing the entire config.toml file
#[derive(Debug, Deserialize)]
pub struct Config {
    /// List of envelope configurations to seed
    pub envelopes: Vec<EnvelopeConfig>,
}

/// Configuration for a single envelope
#[derive(Debug, Deserialize, Clone)]
pub struct EnvelopeConfig {
    /// Name of the envelope
    pub name: String,
    /// Category for organization (e.g., "necessary", `quality_of_life`)
    pub category: String,
    /// Monthly allocation amount
    pub allocation: f64,
    /// Whether this envelope is individual (per-user) or shared
    pub is_individual: bool,
    /// Whether unused balance rolls over to next month
    pub rollover: bool,
}

/// Loads envelope configuration from a TOML file
///
/// # Arguments
/// * `path` - Path to the config.toml file
///
/// # Returns
/// * `Ok(Config)` - Successfully parsed configuration
/// * `Err(Error)` - Failed to read or parse the configuration file
///
/// # Errors
/// Returns an error if:
/// - The file cannot be read
/// - The TOML syntax is invalid
/// - Required fields are missing
pub fn load_config<P: AsRef<Path>>(path: P) -> Result<Config> {
    let contents = std::fs::read_to_string(path.as_ref()).map_err(|e| Error::Config {
        message: format!("Failed to read config file: {e}"),
    })?;

    toml::from_str(&contents).map_err(|e| Error::Config {
        message: format!("Failed to parse config.toml: {e}"),
    })
}

/// Loads envelope configuration from the default location (./config.toml)
///
/// # Returns
/// * `Ok(Config)` - Successfully parsed configuration
/// * `Err(Error)` - Failed to read or parse the configuration file
pub fn load_default_config() -> Result<Config> {
    load_config("config.toml")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn test_parse_envelope_config() {
        let toml_str = r#"
            [[envelopes]]
            name = "groceries"
            category = "necessary"
            allocation = 500.0
            is_individual = false
            rollover = false

            [[envelopes]]
            name = "game"
            category = "quality_of_life"
            allocation = 80.0
            is_individual = true
            rollover = true
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.envelopes.len(), 2);
        assert_eq!(config.envelopes[0].name, "groceries");
        assert_eq!(config.envelopes[0].allocation, 500.0);
        assert!(!config.envelopes[0].is_individual);
        assert!(!config.envelopes[0].rollover);

        assert_eq!(config.envelopes[1].name, "game");
        assert!(config.envelopes[1].is_individual);
        assert!(config.envelopes[1].rollover);
    }
}
