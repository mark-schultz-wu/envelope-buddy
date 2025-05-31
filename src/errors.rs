use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Environment variable error: {0}")]
    EnvVar(#[from] std::env::VarError),

    #[error("Command execution error: {0}")]
    #[allow(dead_code)]
    Command(String),

    #[error("Rusqlite error: {0}")]
    Rusqlite(#[from] rusqlite::Error),

    #[error("Serenity/Poise framework error: {0}")]
    #[allow(clippy::enum_variant_names)]
    FrameworkError(Box<poise::serenity_prelude::Error>),
}

impl From<poise::serenity_prelude::Error> for Error {
    fn from(value: poise::serenity_prelude::Error) -> Self {
        Error::FrameworkError(Box::new(value))
    }
}

// Convenience `Result` type
pub type Result<T> = std::result::Result<T, Error>;
