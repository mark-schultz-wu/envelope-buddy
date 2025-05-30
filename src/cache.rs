use crate::db::{DbPool, get_all_product_names, get_all_unique_active_envelope_names};
use crate::errors::Result;
use crate::models::CachedEnvelopeInfo;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, trace};

// Stub for refreshing envelope names cache
pub async fn refresh_envelope_names_cache(
    db_pool: &DbPool,
    cache: &Arc<RwLock<Vec<CachedEnvelopeInfo>>>,
) -> Result<()> {
    info!("Refreshing envelope names cache...");
    let infos = get_all_unique_active_envelope_names(db_pool).await?; // Call new DB func
    let mut cache_writer = cache.write().await;
    *cache_writer = infos;
    info!(
        "All envelopes cache refreshed with {} items.",
        cache_writer.len()
    );
    trace!("All envelopes cache now contains: {:?}", cache_writer);
    Ok(())
}

// Stub for refreshing product names cache
pub async fn refresh_product_names_cache(
    db_pool: &DbPool,
    cache: &Arc<RwLock<Vec<String>>>,
) -> Result<()> {
    info!("Refreshing product names cache...");
    // Call the DB function you implemented and tested
    let names = match get_all_product_names(db_pool).await {
        Ok(n) => n,
        Err(e) => {
            error!("DB error fetching all product names for cache: {}", e);
            return Err(e); // Propagate DB error
        }
    };

    let mut cache_writer = cache.write().await; // Acquire write lock
    *cache_writer = names; // Replace old cache content

    info!(
        "Product names cache refreshed with {} items.",
        cache_writer.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbPool;
    use crate::db::test_utils::{DirectInsertArgs, direct_insert_envelope};
    use crate::db::{products, schema};
    use crate::errors::Result;
    use crate::models::CachedEnvelopeInfo;
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};
    use tokio::sync::RwLock as TokioRwLock;
    use tracing_subscriber::EnvFilter;

    // --- Test Helper Functions ---
    fn init_test_tracing() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("trace")),
            )
            .with_test_writer()
            .try_init();
    }

    async fn setup_test_db() -> Result<DbPool> {
        let conn = Connection::open_in_memory()?;
        schema::create_tables(&conn)?;
        Ok(Arc::new(Mutex::new(conn))) // std::sync::Mutex as per DbPool definition
    }

    #[tokio::test]
    async fn test_refresh_all_envelopes_cache_populates_correctly() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;
        let cache: Arc<TokioRwLock<Vec<CachedEnvelopeInfo>>> =
            Arc::new(TokioRwLock::new(Vec::new()));

        let user1 = "user1_cache_test";
        let user2 = "user2_cache_test";

        // Insert some test data directly
        {
            let conn = pool.lock().unwrap();
            // Active shared
            direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Groceries",
                category: "N",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
            // Active individual for user1
            direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Hobby",
                category: "Q",
                allocation: 1.0,
                balance: 1.0,
                is_individual: true,
                user_id: Some(user1),
                rollover: true,
                is_deleted: false,
            })?;
            // Active individual for user2
            direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Hobby",
                category: "Q",
                allocation: 1.0,
                balance: 1.0,
                is_individual: true,
                user_id: Some(user2),
                rollover: true,
                is_deleted: false,
            })?;
            // Soft-deleted (should not appear in cache)
            direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "Old Stuff",
                category: "S",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: true,
            })?;
        }

        refresh_envelope_names_cache(&pool, &cache).await?;

        let cache_guard = cache.read().await;
        assert_eq!(
            cache_guard.len(),
            3,
            "Cache should contain 3 active envelope infos"
        );
        assert!(
            cache_guard
                .iter()
                .any(|ei| ei.name == "Groceries" && ei.user_id.is_none())
        );
        assert!(
            cache_guard
                .iter()
                .any(|ei| ei.name == "Hobby" && ei.user_id.as_deref() == Some(user1))
        );
        assert!(
            cache_guard
                .iter()
                .any(|ei| ei.name == "Hobby" && ei.user_id.as_deref() == Some(user2))
        );
        assert!(!cache_guard.iter().any(|ei| ei.name == "Old Stuff"));
        Ok(())
    }

    #[tokio::test]
    async fn test_refresh_product_names_cache_populates_correctly() -> Result<()> {
        init_test_tracing();
        let pool = setup_test_db().await?;
        let cache: Arc<TokioRwLock<Vec<String>>> = Arc::new(TokioRwLock::new(Vec::new()));
        let env_id;
        {
            let conn = pool.lock().unwrap();
            env_id = direct_insert_envelope(&DirectInsertArgs {
                conn: &conn,
                name: "DefaultEnv",
                category: "cat",
                allocation: 1.0,
                balance: 1.0,
                is_individual: false,
                user_id: None,
                rollover: false,
                is_deleted: false,
            })?;
        }

        // Insert some products
        products::add_product(&pool, "Milk", 2.99, env_id, None).await?;
        products::add_product(&pool, "Bread", 3.50, env_id, None).await?;

        refresh_product_names_cache(&pool, &cache).await?;
        let cache_guard = cache.read().await;
        assert_eq!(cache_guard.len(), 2);
        assert!(cache_guard.contains(&"Milk".to_string()));
        assert!(cache_guard.contains(&"Bread".to_string()));
        Ok(())
    }
}
