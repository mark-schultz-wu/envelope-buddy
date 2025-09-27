//! Product business logic - Handles all product-related operations.
//! This module provides functions for creating, retrieving, updating, and managing products
//! within the envelope system. Products are predefined items with fixed prices that can be
//! quickly used in transactions via the /use_product command. All functions are async and
//! return Result types for proper error handling throughout the system.

use crate::{
    entities::*,
    errors::{Error, Result},
};
use sea_orm::*;

/// Retrieves all active (non-deleted) products from the database, ordered alphabetically by name.
/// This function is commonly used to display the complete list of available products
/// to users, such as in autocomplete suggestions or product selection interfaces.
pub async fn get_all_active_products(db: &DatabaseConnection) -> Result<Vec<product::Model>> {
    Product::find()
        .filter(product::Column::IsDeleted.eq(false))
        .order_by_asc(product::Column::Name)
        .all(db)
        .await
        .map_err(Into::into)
}

/// Finds a specific product by its name, returning None if not found or deleted.
/// This function is used for product lookups when users reference products by name
/// in commands, and ensures that deleted products are not accessible.
pub async fn get_product_by_name(
    db: &DatabaseConnection,
    name: &str,
) -> Result<Option<product::Model>> {
    Product::find()
        .filter(product::Column::Name.eq(name))
        .filter(product::Column::IsDeleted.eq(false))
        .one(db)
        .await
        .map_err(Into::into)
}

/// Retrieves a specific product by its unique ID.
/// This function is used for product lookups when the ID is known, such as when
/// processing transactions that reference a product by ID.
pub async fn get_product_by_id(
    db: &DatabaseConnection,
    product_id: i32,
) -> Result<Option<product::Model>> {
    Product::find_by_id(product_id)
        .one(db)
        .await
        .map_err(Into::into)
}

/// Creates a new product with the specified parameters, performing input validation.
/// This function validates that the name is not empty, the price is non-negative,
/// and trims whitespace from the name. It initializes the product with proper
/// timestamps for tracking creation and updates.
pub async fn create_product(
    db: &DatabaseConnection,
    name: String,
    price: f64,
) -> Result<product::Model> {
    // Validate inputs
    if name.trim().is_empty() {
        return Err(Error::Config {
            message: "Product name cannot be empty".to_string(),
        });
    }

    if price < 0.0 {
        return Err(Error::InvalidAmount { amount: price });
    }

    if !price.is_finite() {
        return Err(Error::InvalidAmount { amount: price });
    }

    let now = chrono::Utc::now().naive_utc();

    let product = product::ActiveModel {
        name: Set(name.trim().to_string()),
        price: Set(price),
        is_deleted: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    product.insert(db).await.map_err(Into::into)
}

/// Updates an existing product's name and price, performing input validation.
/// This function validates the new parameters and ensures the product exists
/// before attempting the update. It refreshes the updated timestamp.
pub async fn update_product(
    db: &DatabaseConnection,
    product_id: i32,
    new_name: String,
    new_price: f64,
) -> Result<product::Model> {
    // Validate inputs
    if new_name.trim().is_empty() {
        return Err(Error::Config {
            message: "Product name cannot be empty".to_string(),
        });
    }

    if new_price < 0.0 {
        return Err(Error::InvalidAmount { amount: new_price });
    }

    if !new_price.is_finite() {
        return Err(Error::InvalidAmount { amount: new_price });
    }

    let mut product: product::ActiveModel = Product::find_by_id(product_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::ProductNotFound {
            name: product_id.to_string(),
        })?
        .into();

    if *product.is_deleted.as_ref() {
        return Err(Error::ProductNotFound {
            name: product_id.to_string(),
        });
    }

    product.name = Set(new_name.trim().to_string());
    product.price = Set(new_price);
    product.updated_at = Set(chrono::Utc::now().naive_utc());

    product.update(db).await.map_err(Into::into)
}

/// Soft deletes a product by marking it as deleted, preserving transaction history.
/// This function ensures the product exists and is not already deleted before
/// performing the soft delete operation.
pub async fn delete_product(db: &DatabaseConnection, product_id: i32) -> Result<product::Model> {
    let mut product: product::ActiveModel = Product::find_by_id(product_id)
        .one(db)
        .await?
        .ok_or_else(|| Error::ProductNotFound {
            name: product_id.to_string(),
        })?
        .into();

    if *product.is_deleted.as_ref() {
        return Err(Error::ProductNotFound {
            name: product_id.to_string(),
        });
    }

    product.is_deleted = Set(true);
    product.updated_at = Set(chrono::Utc::now().naive_utc());

    product.update(db).await.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_product_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test empty name validation
        let result = create_product(&db, "".to_string(), 10.0).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test whitespace-only name validation
        let result = create_product(&db, "   ".to_string(), 10.0).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test negative price validation
        let result = create_product(&db, "Test Product".to_string(), -10.0).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: -10.0 }
        ));

        // Test NaN price validation
        let result = create_product(&db, "Test Product".to_string(), f64::NAN).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        // Test infinity price validation
        let result = create_product(&db, "Test Product".to_string(), f64::INFINITY).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_product_integration() -> Result<()> {
        // Use real database to test actual product creation
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        let product = create_product(&db, "Test Product".to_string(), 15.50).await?;

        assert_eq!(product.name, "Test Product");
        assert_eq!(product.price, 15.50);
        assert!(!product.is_deleted);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_product_by_name_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create a product
        let created_product = create_product(&db, "Test Product".to_string(), 25.0).await?;

        // Test finding it by name
        let found_product = get_product_by_name(&db, "Test Product").await?;
        assert!(found_product.is_some());
        assert_eq!(found_product.unwrap().id, created_product.id);

        // Test finding non-existent product
        let not_found = get_product_by_name(&db, "Non-existent").await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_active_products_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create multiple products
        let product0 = create_product(&db, "Product 0".to_string(), 10.0).await?;

        let product1 = create_product(&db, "Product 1".to_string(), 20.0).await?;

        // Test getting all active products
        let products = get_all_active_products(&db).await?;
        assert_eq!(products.len(), 2);

        // Test that they're ordered alphabetically
        assert_eq!(products[0], product0);
        assert_eq!(products[1], product1);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_product_integration() -> Result<()> {
        // Use real database to test actual update logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create a product
        let product = create_product(&db, "Original Name".to_string(), 10.0).await?;

        // Update the product
        let updated_product =
            update_product(&db, product.id, "Updated Name".to_string(), 15.0).await?;

        assert_eq!(updated_product.name, "Updated Name");
        assert_eq!(updated_product.price, 15.0);
        assert_eq!(updated_product.id, product.id);

        // Verify the update persisted
        let retrieved = Product::find_by_id(product.id).one(&db).await?.unwrap();
        assert_eq!(retrieved.name, "Updated Name");
        assert_eq!(retrieved.price, 15.0);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_product_validation() -> Result<()> {
        let db = MockDatabase::new(DatabaseBackend::Sqlite).into_connection();

        // Test empty name validation
        let result = update_product(&db, 1, "".to_string(), 10.0).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Config { message: _ }));

        // Test negative price validation
        let result = update_product(&db, 1, "Test".to_string(), -10.0).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::InvalidAmount { amount: -10.0 }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_product_integration() -> Result<()> {
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        let product = create_product(&db, "Test Product".to_string(), 10.0).await?;

        // Capture the returned deleted product
        let deleted_product = delete_product(&db, product.id).await?;

        // Test the returned product directly
        assert!(deleted_product.is_deleted);
        assert_eq!(deleted_product.id, product.id);

        // Test that it's not returned in active products
        let active_products = get_all_active_products(&db).await?;
        assert_eq!(active_products.len(), 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_product_not_found() -> Result<()> {
        // Use real database to test error handling
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Try to delete non-existent product
        let result = delete_product(&db, 999).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            Error::ProductNotFound { name: _ }
        ));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_product_by_id_integration() -> Result<()> {
        // Use real database to test actual query logic
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create a product
        let product = create_product(&db, "Test Product".to_string(), 30.0).await?;

        // Test finding the product by ID
        let found_product = get_product_by_id(&db, product.id).await?;
        assert!(found_product.is_some());
        let found = found_product.unwrap();
        assert_eq!(found.id, product.id);
        assert_eq!(found.name, "Test Product");
        assert_eq!(found.price, 30.0);

        // Test finding non-existent product
        let not_found = get_product_by_id(&db, 999).await?;
        assert!(not_found.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_active_products_excludes_deleted() -> Result<()> {
        // Use real database to test that deleted products are excluded
        let db = sea_orm::Database::connect("sqlite::memory:").await?;
        crate::config::database::create_tables(&db).await?;

        // Create active product
        let active_product = create_product(&db, "Active Product".to_string(), 10.0).await?;

        // Create and delete another product
        let deleted_product = create_product(&db, "Deleted Product".to_string(), 20.0).await?;
        delete_product(&db, deleted_product.id).await?;

        // Test that only active product is returned
        let products = get_all_active_products(&db).await?;
        assert_eq!(products.len(), 1);
        assert_eq!(products[0], active_product);

        Ok(())
    }
}
