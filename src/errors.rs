use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] Box<sea_orm::DbErr>),

    #[error("Discord error: {0}")]
    Discord(#[from] Box<serenity::Error>),

    #[error("Envelope not found: {name}")]
    EnvelopeNotFound { name: String },

    #[error("Product not found: {name}")]
    ProductNotFound { name: String },

    #[error("Insufficient funds: envelope has {current}, need {required}")]
    InsufficientFunds { current: f64, required: f64 },

    #[error("Invalid amount: {amount}")]
    InvalidAmount { amount: f64 },

    #[error("User not found: {user_id}")]
    UserNotFound { user_id: String },

    #[error("Configuration error: {message}")]
    Config { message: String },
}

// Add explicit From implementations for unboxed types
impl From<sea_orm::DbErr> for Error {
    fn from(err: sea_orm::DbErr) -> Self {
        Error::Database(Box::new(err))
    }
}

impl From<serenity::Error> for Error {
    fn from(err: serenity::Error) -> Self {
        Error::Discord(Box::new(err))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
