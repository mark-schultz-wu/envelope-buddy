//! Bot layer - Discord-specific interface and command handlers
//!
//! This module provides the Discord interface for the EnvelopeBuddy application,
//! including all slash commands, autocomplete handlers, and bot context management.

/// Discord command implementations (envelope, transaction, product, general)
pub mod commands;
/// Discord interaction handlers (autocomplete, etc.)
pub mod handlers;

use sea_orm::DatabaseConnection;

/// Shared data available to all bot commands.
/// This structure holds the database connection and any other global state
/// that commands need to access.
pub struct BotData {
    /// Database connection for all database operations
    pub database: DatabaseConnection,
}

impl BotData {
    /// Creates a new `BotData` instance with the given database connection.
    /// This is typically called during bot initialization to set up the
    /// shared context for all commands.
    #[must_use]
    pub const fn new(database: DatabaseConnection) -> Self {
        Self { database }
    }
}

pub use commands::*;
pub use handlers::*;
