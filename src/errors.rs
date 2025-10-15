//! Error types and result handling for `EnvelopeBuddy`
//!
//! This module provides a unified error type that consolidates all possible errors
//! that can occur throughout the application, from database operations to Discord
//! interactions and business logic validation.

use thiserror::Error;

/// Unified error type for all `EnvelopeBuddy` operations
#[derive(Error, Debug)]
pub enum Error {
    /// Database operation failed (`SeaORM` errors)
    #[error("Database error: {0}")]
    Database(#[from] Box<sea_orm::DbErr>),

    /// Discord API interaction failed (Serenity errors)
    #[error("Discord error: {0}")]
    Discord(#[from] Box<serenity::Error>),

    /// String formatting operation failed
    #[error("String Formatting Error: {0}")]
    Formatting(#[from] std::fmt::Error),

    /// Numeric conversion failed (e.g., i64 to i32)
    #[error("Numeric Conversion Error: {0}")]
    NumericConversion(#[from] std::num::TryFromIntError),

    /// Requested envelope was not found in the database
    #[error("Envelope not found: {name}")]
    EnvelopeNotFound {
        /// Name of the envelope that wasn't found
        name: String,
    },

    /// Requested product was not found in the database
    #[error("Product not found: {name}")]
    ProductNotFound {
        /// Name of the product that wasn't found
        name: String,
    },

    /// Transaction would result in negative balance
    #[error("Insufficient funds: envelope has {current}, need {required}")]
    InsufficientFunds {
        /// Current envelope balance
        current: f64,
        /// Amount required for the transaction
        required: f64,
    },

    /// Transaction amount is invalid (e.g., zero, NaN, infinity)
    #[error("Invalid amount: {amount}")]
    InvalidAmount {
        /// The invalid amount value
        amount: f64,
    },

    /// Referenced user was not found
    #[error("User not found: {user_id}")]
    UserNotFound {
        /// Discord user ID that wasn't found
        user_id: String,
    },

    /// Configuration or system state error
    #[error("Configuration error: {message}")]
    Config {
        /// Description of the configuration error
        message: String,
    },
}

// Add explicit From implementations for unboxed types
impl From<sea_orm::DbErr> for Error {
    fn from(err: sea_orm::DbErr) -> Self {
        Self::Database(Box::new(err))
    }
}

impl From<serenity::Error> for Error {
    fn from(err: serenity::Error) -> Self {
        Self::Discord(Box::new(err))
    }
}

/// Convenience type alias for Result<T, Error>
///
/// This type alias is used throughout the codebase to simplify function signatures
/// that return results with the unified `Error` type.
pub type Result<T> = std::result::Result<T, Error>;
