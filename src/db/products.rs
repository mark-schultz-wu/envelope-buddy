use crate::db::DbPool;
use crate::errors::{Error, Result};
use crate::models::Product;
use rusqlite::{OptionalExtension, params};
use tracing::{debug, info, instrument, trace};

#[instrument(skip(pool, description))]
pub async fn add_product(
    pool: &DbPool,
    name: &str,
    price: f64,
    envelope_id: i64,
    description: Option<&str>,
) -> Result<i64> {
    // Returns the ID of the new product
    if price < 0.0 {
        return Err(Error::Command(
            "Product price cannot be negative.".to_string(),
        ));
    }
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock for adding product".to_string()))?;
    let mut stmt = conn.prepare_cached(
        "INSERT INTO products (name, price, envelope_id, description) VALUES (?1, ?2, ?3, ?4)",
    )?;
    let product_id = stmt.insert(params![name, price, envelope_id, description])?;
    info!(
        "Added new product '{}' (ID: {}) with price {} linked to envelope_id {}",
        name, product_id, price, envelope_id
    );
    Ok(product_id)
}

#[instrument(skip(pool))]
pub async fn get_product_by_name(pool: &DbPool, name: &str) -> Result<Option<Product>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let mut stmt = conn.prepare_cached(
        // Select basic product info. Envelope name can be joined if needed here or separately.
        "SELECT p.id, p.name, p.price, p.envelope_id, p.description, e.name as envelope_name
         FROM products p
         JOIN envelopes e ON p.envelope_id = e.id
         WHERE p.name = ?1",
    )?;
    let product_result = stmt
        .query_row(params![name], |row| {
            Ok(Product {
                id: row.get(0)?,
                name: row.get(1)?,
                price: row.get(2)?,
                envelope_id: row.get(3)?,
                description: row.get(4)?,
                envelope_name: row.get(5)?, // Get the joined envelope name
            })
        })
        .optional()?; // Handles case where no product is found

    debug!(
        "Product lookup by name '{}': {:?}",
        name,
        product_result.as_ref().map(|p| &p.id)
    );
    Ok(product_result)
}

#[instrument(skip(pool))]
pub async fn get_product_by_id(pool: &DbPool, product_id: i64) -> Result<Option<Product>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.name, p.price, p.envelope_id, p.description, e.name as envelope_name
         FROM products p
         JOIN envelopes e ON p.envelope_id = e.id
         WHERE p.id = ?1",
    )?;
    let product_result = stmt
        .query_row(params![product_id], |row| {
            Ok(Product {
                id: row.get(0)?,
                name: row.get(1)?,
                price: row.get(2)?,
                envelope_id: row.get(3)?,
                description: row.get(4)?,
                envelope_name: row.get(5)?,
            })
        })
        .optional()?;
    Ok(product_result)
}

#[instrument(skip(pool))]
pub async fn list_all_products(pool: &DbPool) -> Result<Vec<Product>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let mut stmt = conn.prepare_cached(
        "SELECT p.id, p.name, p.price, p.envelope_id, p.description, e.name as envelope_name
         FROM products p
         JOIN envelopes e ON p.envelope_id = e.id
         ORDER BY p.name ASC",
    )?;
    let product_iter = stmt.query_map([], |row| {
        Ok(Product {
            id: row.get(0)?,
            name: row.get(1)?,
            price: row.get(2)?,
            envelope_id: row.get(3)?,
            description: row.get(4)?,
            envelope_name: row.get(5)?,
        })
    })?;

    let mut products = Vec::new();
    for product_result in product_iter {
        products.push(
            product_result
                .map_err(|e| Error::Database(format!("Failed to map product row: {}", e)))?,
        );
    }
    debug!("Fetched {} products.", products.len());
    Ok(products)
}

#[instrument(skip(pool))]
pub async fn update_product_price(pool: &DbPool, product_id: i64, new_price: f64) -> Result<usize> {
    if new_price < 0.0 {
        return Err(Error::Command(
            "Product price cannot be negative.".to_string(),
        ));
    }
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let rows_affected = conn.execute(
        "UPDATE products SET price = ?1 WHERE id = ?2",
        params![new_price, product_id],
    )?;
    info!(
        "Updated price for product_id {}: new_price = {}",
        product_id, new_price
    );
    Ok(rows_affected)
}

#[instrument(skip(pool))]
pub async fn delete_product_by_name(pool: &DbPool, name: &str) -> Result<usize> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let rows_affected = conn.execute("DELETE FROM products WHERE name = ?1", params![name])?;
    info!(
        "Attempted to delete product by name '{}', rows affected: {}",
        name, rows_affected
    );
    Ok(rows_affected)
}

#[instrument(skip(pool))]
pub async fn get_all_product_names(pool: &DbPool) -> Result<Vec<String>> {
    let conn = pool.lock().map_err(|_| {
        Error::Database("Failed to acquire DB lock for fetching all product names".to_string())
    })?;

    let mut stmt = conn.prepare_cached("SELECT name FROM products ORDER BY name ASC")?; // Products already have a UNIQUE constraint on name

    let names_iter = stmt.query_map([], |row| {
        row.get(0) // Get the name string
    })?;

    let mut names = Vec::new();
    for name_result in names_iter {
        names.push(name_result.map_err(|e| {
            Error::Database(format!("Failed to map product name row for cache: {}", e))
        })?);
    }
    debug!("Fetched {} product names for cache.", names.len());
    Ok(names)
}

#[instrument(skip(pool))]
pub async fn suggest_product_names(pool: &DbPool, partial_name: &str) -> Result<Vec<String>> {
    let conn = pool
        .lock()
        .map_err(|_| Error::Database("Failed to acquire DB lock".to_string()))?;
    let lower_partial_name = partial_name.to_lowercase();
    let search_pattern = format!("{}%", lower_partial_name);

    let mut stmt = conn.prepare_cached(
        "SELECT name FROM products
         WHERE LOWER(name) LIKE ?1
         ORDER BY name ASC
         LIMIT 25",
    )?;
    let names_iter = stmt.query_map(params![search_pattern], |row| row.get(0))?;
    let mut names = Vec::new();
    for name_result in names_iter {
        names.push(
            name_result
                .map_err(|e| Error::Database(format!("Failed to map product name: {}", e)))?,
        );
    }
    trace!(
        "Suggested product names for partial '{}': {:?}",
        partial_name, names
    );
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_utils::{
        DirectInsertArgs, direct_insert_envelope, init_test_tracing, setup_test_db,
    };
    use crate::errors::Result;

    #[tokio::test]
    async fn test_add_and_get_product() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;
        let env_id;
        {
            let conn = pool.lock().unwrap();
            // Assuming direct_insert_envelope is accessible and correctly defined
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Food",
                category: "nec",
                allocation: 100.0,
                balance: 100.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }

        let product_name = "Milk";
        let product_price = 2.99;
        let product_desc = Some("Gallon of Milk");

        let product_id =
            add_product(&pool, product_name, product_price, env_id, product_desc).await?;
        assert!(product_id > 0);

        let fetched_product_opt = get_product_by_name(&pool, product_name).await?;
        assert!(fetched_product_opt.is_some());
        let fetched_product = fetched_product_opt.unwrap();

        assert_eq!(fetched_product.name, product_name);
        assert_eq!(fetched_product.price, product_price);
        assert_eq!(fetched_product.envelope_id, env_id);
        assert_eq!(fetched_product.description.as_deref(), product_desc);
        assert_eq!(fetched_product.envelope_name.as_deref(), Some("Food")); // Check joined name

        // Test unique constraint
        let add_duplicate_result = add_product(&pool, product_name, 3.99, env_id, None).await;
        assert!(
            add_duplicate_result.is_err(),
            "Adding product with duplicate name should fail"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_product_by_id_found_and_not_found() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_get_product_by_id_found_and_not_found");
        let pool = setup_test_db().await?;
        let env_id;
        {
            let conn = pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Electronics",
                category: "qol",
                allocation: 300.0,
                balance: 300.0,
                is_individual: false,
                user_id: None,
                rollover: true,
                is_deleted: false,
            })?;
        }

        let product_id =
            add_product(&pool, "Headphones", 79.99, env_id, Some("Noise cancelling")).await?;

        // Test found
        let fetched_product_opt = get_product_by_id(&pool, product_id).await?;
        assert!(
            fetched_product_opt.is_some(),
            "Product should be found by ID"
        );
        let fetched_product = fetched_product_opt.unwrap();
        assert_eq!(fetched_product.id, product_id);
        assert_eq!(fetched_product.name, "Headphones");
        assert_eq!(
            fetched_product.envelope_name.as_deref(),
            Some("Electronics")
        );

        // Test not found
        let not_found_product_opt = get_product_by_id(&pool, product_id + 999).await?;
        assert!(
            not_found_product_opt.is_none(),
            "Product should not be found for a non-existent ID"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_list_all_products_empty_and_with_data() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_list_all_products_empty_and_with_data");
        let pool = setup_test_db().await?;

        // Test with no products
        let products_empty = list_all_products(&pool).await?;
        assert!(
            products_empty.is_empty(),
            "Product list should be empty initially"
        );

        // Add some products
        let env_id1;
        let env_id2;
        {
            let conn = pool.lock().unwrap();
            env_id1 = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Groceries",
                category: "n",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
            env_id2 = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Entertainment",
                category: "q",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }
        add_product(&pool, "Bread", 3.50, env_id1, None).await?;
        add_product(&pool, "Movie Ticket", 15.00, env_id2, Some("Cinema visit")).await?;
        add_product(&pool, "Apples", 4.00, env_id1, Some("Bag of apples")).await?;

        let products_with_data = list_all_products(&pool).await?;
        assert_eq!(products_with_data.len(), 3, "Should list 3 products");

        // Check if names are present (order is by name ASC)
        assert_eq!(products_with_data[0].name, "Apples");
        assert_eq!(products_with_data[1].name, "Bread");
        assert_eq!(products_with_data[2].name, "Movie Ticket");

        assert_eq!(
            products_with_data[0].envelope_name.as_deref(),
            Some("Groceries")
        );
        assert_eq!(
            products_with_data[2].envelope_name.as_deref(),
            Some("Entertainment")
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_update_product_price_valid_and_invalid() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_update_product_price_valid_and_invalid");
        let pool = setup_test_db().await?;
        let env_id;
        {
            let conn = pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Books",
                category: "edu",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }
        let product_id = add_product(&pool, "Novel", 12.99, env_id, None).await?;

        // Valid update
        let new_price = 14.50;
        let rows_affected = update_product_price(&pool, product_id, new_price).await?;
        assert_eq!(rows_affected, 1, "Should affect 1 row for valid update");

        let updated_product = get_product_by_id(&pool, product_id).await?.unwrap();
        assert_eq!(updated_product.price, new_price, "Price should be updated");

        // Invalid update (negative price)
        let update_negative_result = update_product_price(&pool, product_id, -5.0).await;
        assert!(
            update_negative_result.is_err(),
            "Updating with negative price should fail"
        );
        if let Err(Error::Command(msg)) = update_negative_result {
            assert!(msg.contains("Product price cannot be negative"));
        } else {
            panic!("Expected Command error for negative price");
        }

        // Check price hasn't changed due to failed update
        let product_after_failed_update = get_product_by_id(&pool, product_id).await?.unwrap();
        assert_eq!(
            product_after_failed_update.price, new_price,
            "Price should remain unchanged after failed negative update"
        );

        // Update non-existent product
        let rows_affected_non_existent =
            update_product_price(&pool, product_id + 999, 20.0).await?;
        assert_eq!(
            rows_affected_non_existent, 0,
            "Updating non-existent product should affect 0 rows"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_product_by_name_exists_and_not_exists() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_delete_product_by_name_exists_and_not_exists");
        let pool = setup_test_db().await?;
        let env_id;
        {
            let conn = pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Office",
                category: "work",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }
        let product_name = "Stapler";
        add_product(&pool, product_name, 8.75, env_id, None).await?;

        // Delete existing product
        let rows_affected_existing = delete_product_by_name(&pool, product_name).await?;
        assert_eq!(
            rows_affected_existing, 1,
            "Should delete 1 existing product"
        );

        let fetched_after_delete = get_product_by_name(&pool, product_name).await?;
        assert!(
            fetched_after_delete.is_none(),
            "Product should be gone after deletion"
        );

        // Try deleting non-existent product
        let rows_affected_non_existent =
            delete_product_by_name(&pool, "NonExistentProduct").await?;
        assert_eq!(
            rows_affected_non_existent, 0,
            "Deleting non-existent product should affect 0 rows"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_suggest_product_names_functionality() -> Result<()> {
        init_test_tracing();
        tracing::info!("Running test_suggest_product_names_functionality");
        let pool = setup_test_db().await?;
        let env_id;
        {
            let conn = pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Snacks",
                category: "food",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }

        add_product(&pool, "Apple Pie", 5.00, env_id, None).await?;
        add_product(&pool, "Banana Bread", 4.50, env_id, None).await?;
        add_product(&pool, "apple juice", 2.00, env_id, None).await?; // Lowercase to test case-insensitivity
        add_product(&pool, "Orange", 1.00, env_id, None).await?;

        // Test full match
        let suggestions1 = suggest_product_names(&pool, "Apple Pie").await?;
        assert_eq!(suggestions1.len(), 1);
        assert_eq!(suggestions1[0], "Apple Pie");

        // Test partial match (case-insensitive)
        let suggestions2 = suggest_product_names(&pool, "app").await?;
        assert_eq!(
            suggestions2.len(),
            2,
            "Should find 'Apple Pie' and 'apple juice'"
        );
        assert!(suggestions2.contains(&"Apple Pie".to_string()));
        assert!(suggestions2.contains(&"apple juice".to_string()));

        // Test partial match leading to multiple results
        let suggestions3 = suggest_product_names(&pool, "Ba").await?;
        assert_eq!(suggestions3.len(), 1);
        assert_eq!(suggestions3[0], "Banana Bread");

        // Test no match
        let suggestions4 = suggest_product_names(&pool, "xyz").await?;
        assert!(suggestions4.is_empty());

        // Test empty partial -> should return all (up to limit 25)
        let suggestions_empty_partial = suggest_product_names(&pool, "").await?;
        assert_eq!(suggestions_empty_partial.len(), 4); // All 4 products

        Ok(())
    }
    #[tokio::test]
    async fn test_get_all_product_names_for_cache_empty() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;
        let names = get_all_product_names(&pool).await?;
        assert!(names.is_empty(), "Should return empty vec for no products");
        Ok(())
    }

    #[tokio::test]
    async fn test_get_all_product_names_for_cache_with_data() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;
        let env_id;
        {
            let conn = pool.lock().unwrap();
            // Assuming direct_insert_envelope is accessible and correctly defined.
            // You might need to adjust this if it's in a different module.
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "DefaultEnv",
                category: "cat",
                allocation: 10.0,
                balance: 10.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }

        add_product(&pool, "Milk", 2.99, env_id, None).await?;
        add_product(&pool, "Bread", 3.50, env_id, None).await?;
        add_product(&pool, "Apples", 4.00, env_id, None).await?;

        let names = get_all_product_names(&pool).await?;
        assert_eq!(names.len(), 3, "Should list 3 product names");
        // Check order (ORDER BY name ASC in the query)
        assert_eq!(
            names,
            vec![
                "Apples".to_string(),
                "Bread".to_string(),
                "Milk".to_string()
            ]
        );
        Ok(())
    }
}
