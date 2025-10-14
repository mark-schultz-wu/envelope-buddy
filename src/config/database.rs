//! Database configuration module for `EnvelopeBuddy` V2.
//!
//! This module handles `SQLite` database connection and table creation using `SeaORM`.
//! It provides functions for establishing database connections and creating all necessary tables
//! based on the entity definitions. The module uses `SeaORM`'s `Schema::create_table_from_entity`
//! method to automatically generate SQL statements from the entity models, ensuring that the
//! database schema matches the Rust struct definitions without requiring manual SQL.

use crate::entities::{Envelope, Product, SystemState, Transaction};
use crate::errors::Result;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Schema};

/// Gets the database URL from environment variable or returns default `SQLite` path.
///
/// This function looks for `DATABASE_URL` in the environment and falls back to
/// a default local `SQLite` file if not found.
pub fn get_database_url() -> Result<String> {
    Ok(std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/envelope_buddy.sqlite".to_string()))
}

/// Establishes a connection to the `SQLite` database using the `DATABASE_URL` environment variable.
///
/// Falls back to a default local `SQLite` file if no environment variable is set.
/// This function handles connection errors and provides a clean interface for database access
/// throughout the application.
pub async fn create_connection() -> Result<DatabaseConnection> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/envelope_buddy.sqlite".to_string());

    Database::connect(&database_url).await.map_err(Into::into)
}

/// Creates all necessary database tables using `SeaORM`'s schema generation from entity definitions.
///
/// This function uses the `DeriveEntityModel` macros to automatically generate proper SQL
/// statements for table creation, ensuring the database schema matches the Rust struct definitions.
/// It creates tables for envelopes, products, transactions, and system state.
pub async fn create_tables(db: &DatabaseConnection) -> Result<()> {
    // Use SeaORM's proper table creation using Schema::create_table_from_entity
    let builder = db.get_database_backend();
    let schema = Schema::new(builder);

    // Create tables using SeaORM's schema generation
    let envelope_table = schema.create_table_from_entity(Envelope);
    let product_table = schema.create_table_from_entity(Product);
    let transaction_table = schema.create_table_from_entity(Transaction);
    let system_state_table = schema.create_table_from_entity(SystemState);

    db.execute(builder.build(&envelope_table)).await?;
    db.execute(builder.build(&product_table)).await?;
    db.execute(builder.build(&transaction_table)).await?;
    db.execute(builder.build(&system_state_table)).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{
        envelope::Model as EnvelopeModel, product::Model as ProductModel,
        system_state::Model as SystemStateModel, transaction::Model as TransactionModel,
    };
    use sea_orm::{EntityTrait, QuerySelect};

    /// Tests the database connection by executing a simple query
    async fn test_connection(db: &DatabaseConnection) -> Result<()> {
        // Test the connection with a simple query
        let _: Vec<EnvelopeModel> = Envelope::find().limit(1).all(db).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_create_connection() -> Result<()> {
        // Use in-memory database for testing to avoid schema conflicts with existing database
        let db = Database::connect("sqlite::memory:").await?;
        create_tables(&db).await?;

        // Test that we can execute a query to verify the connection is working
        let _: Vec<EnvelopeModel> = Envelope::find().limit(1).all(&db).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_create_tables() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        create_tables(&db).await?;

        // Test that tables exist by querying them
        let _: Vec<EnvelopeModel> = Envelope::find().limit(1).all(&db).await?;
        let _: Vec<ProductModel> = Product::find().limit(1).all(&db).await?;
        let _: Vec<TransactionModel> = crate::entities::Transaction::find()
            .limit(1)
            .all(&db)
            .await?;
        let _: Vec<SystemStateModel> = SystemState::find().limit(1).all(&db).await?;

        Ok(())
    }

    #[tokio::test]
    async fn test_connection_test() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        create_tables(&db).await?;
        test_connection(&db).await?;
        Ok(())
    }
}
