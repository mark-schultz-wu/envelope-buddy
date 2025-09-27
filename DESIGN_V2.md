# EnvelopeBuddy V2 Design Document

## 1. Executive Summary

EnvelopeBuddy V2 is a comprehensive architectural redesign that maintains 100% of the current functionality while dramatically simplifying the codebase. The primary goals are:

1. **Preserve All Functionality** - Every Discord command, autocomplete, description, embed, and user experience feature remains identical
2. **Simplify Architecture** - Replace complex raw SQL with SeaORM, reorganize code for clarity
3. **Improve Maintainability** - Separate business logic from Discord framework code
4. **Enable Easier Testing** - Make core logic framework-agnostic and testable
5. **Reduce Complexity** - Fewer files, clearer structure, single error type

## 2. Current State Analysis

### 2.1. Current Architecture Problems
- **Database Layer Complexity**: 800+ lines of raw SQL across 7 files
- **Tight Coupling**: Business logic embedded in Discord command handlers
- **Error Handling Inconsistency**: Multiple error types and conversion patterns
- **Testing Difficulty**: Core logic cannot be tested independently of Discord
- **Code Duplication**: Similar patterns repeated across command modules
- **Maintenance Burden**: Complex SQL queries difficult to modify and debug

### 2.2. Current Functionality (100% Preserved)
- **Discord Commands**: 15+ slash commands with full descriptions and autocomplete
- **Envelope System**: Shared/individual envelopes, monthly allocations, rollover logic
- **Transaction Management**: Spend/addfunds with detailed tracking and mini-reports
- **Product System**: Fixed-price items with quick consumption logging
- **Reporting**: Rich embeds with progress indicators and spending analysis
- **Monthly Updates**: Automated rollover/reset with transaction pruning
- **Configuration**: TOML-based envelope seeding and environment configuration

## 3. V2 Architecture Overview

### 3.1. New Directory Structure

```
src/
├── main.rs # Entry point (minimal changes)
├── config.rs # Configuration (preserved)
├── bot/ # Discord interface layer
│ ├── mod.rs # Module exports
│ ├── context.rs # Bot context and data structures
│ ├── commands/ # All Discord command handlers
│ │ ├── mod.rs
│ │ ├── general.rs # ping, help
│ │ ├── transaction.rs # spend, addfunds
│ │ ├── envelope.rs # report, update, manage envelope
│ │ ├── product.rs # use_product, manage product
│ │ └── autocomplete.rs # All autocomplete functions
│ └── utils.rs # Discord-specific utilities (embeds, formatting)
├── core/ # Business logic (framework-agnostic)
│ ├── mod.rs # Core module exports
│ ├── envelope.rs # Envelope business logic
│ ├── transaction.rs # Transaction business logic
│ ├── product.rs # Product business logic
│ ├── report.rs # Report generation logic
│ └── monthly.rs # Monthly update logic
├── entities/ # SeaORM entity definitions
│ ├── mod.rs # Entity exports
│ ├── envelope.rs # Envelope entity
│ ├── transaction.rs # Transaction entity
│ ├── product.rs # Product entity
│ └── system_state.rs # System state entity
├── database/ # Database layer
│ ├── mod.rs # Database module exports
│ ├── connection.rs # Database connection and setup
│ └── migrations/ # Database migration files
│ ├── m20241201_000001_create_tables.rs
│ └── mod.rs
└── errors.rs # Unified error handling
```


### 3.2. Layer Responsibilities

#### 3.2.1. Bot Layer (`bot/`)
- **Purpose**: Handle Discord-specific concerns only
- **Responsibilities**:
  - Parse Discord command parameters
  - Call appropriate core business logic functions
  - Format responses as Discord embeds/messages
  - Handle Discord-specific errors
  - Provide autocomplete data
- **Dependencies**: Core layer, SeaORM entities
- **No Business Logic**: All envelope/transaction logic delegated to core

#### 3.2.2. Core Layer (`core/`)
- **Purpose**: Framework-agnostic business logic
- **Responsibilities**:
  - Envelope creation, deletion, balance updates
  - Transaction processing and validation
  - Product management and consumption
  - Report generation and calculations
  - Monthly update processing
- **Dependencies**: SeaORM entities only
- **No Discord Dependencies**: Pure Rust functions, easily testable

#### 3.2.3. Entity Layer (`entities/`)
- **Purpose**: SeaORM entity definitions and database schema
- **Responsibilities**:
  - Define database schema through SeaORM entities
  - Provide type-safe database operations
  - Handle database migrations
- **Dependencies**: SeaORM, chrono, serde
- **Generated Code**: SeaORM CLI generates boilerplate

#### 3.2.4. Database Layer (`database/`)
- **Purpose**: Database connection and migration management
- **Responsibilities**:
  - Initialize database connections
  - Run migrations
  - Provide connection pooling
- **Dependencies**: SeaORM, SQLite

## 4. SeaORM Migration Details

### 4.1. Why SeaORM?
- **Type Safety**: Compile-time query validation
- **Reduced Boilerplate**: No manual SQL query construction
- **Better Error Handling**: Standardized database error types
- **Migration Management**: Built-in schema versioning
- **Active Development**: Well-maintained with good documentation
- **SQLite Support**: First-class SQLite support with async operations

### 4.2. Entity Definitions

#### 4.2.1. Envelope Entity
```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "envelopes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub category: String,
    pub allocation: f64,
    pub balance: f64,
    pub is_individual: bool,
    pub user_id: Option<String>,
    pub rollover: bool,
    pub is_deleted: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::transaction::Entity")]
    Transactions,
    #[sea_orm(has_many = "super::product::Entity")]
    Products,
}

impl Related<super::transaction::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Transactions.def()
    }
}

impl Related<super::product::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Products.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

#### 4.2.2. Transaction Entity
```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub envelope_id: i64,
    pub amount: f64,
    pub description: String,
    pub timestamp: DateTimeUtc,
    pub user_id: String,
    pub message_id: Option<String>,
    pub transaction_type: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::envelope::Entity",
        from = "Column::EnvelopeId",
        to = "super::envelope::Column::Id"
    )]
    Envelope,
}

impl Related<super::envelope::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Envelope.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
```

### 4.3. Database Operations Comparison

#### 4.3.1. Current Raw SQL Approach
```rust
// Current complex query (50+ lines in db/envelopes.rs)
pub async fn get_user_or_shared_envelope(
    db_pool: &DbPool,
    envelope_name: &str,
    user_id_str: &str,
) -> Result<Option<Envelope>, Error> {
    let conn_guard = db_pool.lock().await;
    let conn = conn_guard.as_ref().map_err(|e| {
        Error::Database(format!("Failed to acquire database connection: {}", e))
    })?;

    let mut stmt = conn.prepare(
        "SELECT id, name, category, allocation, balance, is_individual, user_id, rollover, is_deleted
         FROM envelopes 
         WHERE name = ? AND is_deleted = 0 AND (
             (is_individual = 0 AND user_id IS NULL) OR 
             (is_individual = 1 AND user_id = ?)
         )"
    ).map_err(|e| Error::Database(format!("Failed to prepare statement: {}", e)))?;
    
    // ... 30+ more lines of manual row parsing
}
```

#### 4.3.2. New SeaORM Approach
```rust
// New simplified query (5 lines)
pub async fn get_user_or_shared_envelope(
    db: &DatabaseConnection,
    envelope_name: &str,
    user_id: &str,
) -> Result<Option<envelope::Model>, Error> {
    envelope::Entity::find()
        .filter(envelope::Column::Name.eq(envelope_name))
        .filter(envelope::Column::IsDeleted.eq(false))
        .filter(
            envelope::Column::IsIndividual.eq(false)
                .and(envelope::Column::UserId.is_null())
                .or(
                    envelope::Column::IsIndividual.eq(true)
                        .and(envelope::Column::UserId.eq(user_id))
                )
        )
        .one(db)
        .await
        .map_err(|e| Error::Database(e.to_string()))
}
```

### 4.4. Migration Strategy

#### 4.4.1. Phase 1: Setup SeaORM (Week 1, Days 1-2)
1. Add SeaORM dependencies to `Cargo.toml`
2. Create entity definitions from existing schema
3. Set up migration framework
4. Create initial migration matching current schema
5. Test database connection and basic queries

#### 4.4.2. Phase 2: Core Layer (Week 1, Days 3-5)
1. Create core business logic modules
2. Implement envelope operations with SeaORM
3. Implement transaction operations with SeaORM
4. Implement product operations with SeaORM
5. Add comprehensive unit tests for core logic

#### 4.4.3. Phase 3: Bot Layer Refactor (Week 2, Days 1-3)
1. Refactor command handlers to use core layer
2. Preserve all Discord command signatures and descriptions
3. Maintain autocomplete functionality
4. Keep all embed formatting and user experience
5. Test all commands thoroughly

#### 4.4.4. Phase 4: Testing and Cleanup (Week 2, Days 4-5)
1. Add integration tests
2. Performance testing with large datasets
3. Remove old database layer code
4. Update documentation
5. Final testing and deployment

## 5. Detailed Implementation Specifications

### 5.1. Core Business Logic Functions

#### 5.1.1. Envelope Operations (`core/envelope.rs`)
```rust
use sea_orm::*;
use crate::entities::{envelope, prelude::*};
use crate::errors::{Error, Result};

/// Get all active envelopes for reporting
pub async fn get_all_active_envelopes(db: &DatabaseConnection) -> Result<Vec<envelope::Model>> {
    Envelope::find()
        .filter(envelope::Column::IsDeleted.eq(false))
        .order_by_asc(envelope::Column::Name)
        .all(db)
        .await
        .map_err(|e| Error::Database(e.to_string()))
}

/// Create or re-enable an envelope with flexible parameters
pub async fn create_or_reenable_envelope(
    db: &DatabaseConnection,
    name: &str,
    category: Option<&str>,
    allocation: Option<f64>,
    is_individual: Option<bool>,
    rollover: Option<bool>,
    user_id_1: &str,
    user_id_2: &str,
) -> Result<Vec<String>> {
    // Check if envelope exists (including soft-deleted)
    let existing = Envelope::find()
        .filter(envelope::Column::Name.eq(name))
        .one(db)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

    match existing {
        Some(env) if env.is_deleted => {
            // Re-enable existing envelope
            reenable_envelope(db, env, category, allocation, rollover).await
        }
        Some(_) => {
            // Active envelope already exists
            Err(Error::Command(format!("Envelope '{}' already exists", name)))
        }
        None => {
            // Create new envelope
            create_new_envelope(db, name, category, allocation, is_individual, rollover, user_id_1, user_id_2).await
        }
    }
}

/// Update envelope balance atomically
pub async fn update_envelope_balance(
    db: &DatabaseConnection,
    envelope_id: i64,
    new_balance: f64,
) -> Result<()> {
    envelope::ActiveModel {
        id: Set(envelope_id),
        balance: Set(new_balance),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;
    
    Ok(())
}

/// Soft delete an envelope
pub async fn soft_delete_envelope(
    db: &DatabaseConnection,
    envelope_name: &str,
    user_id: Option<&str>,
) -> Result<String> {
    let envelope = get_user_or_shared_envelope(db, envelope_name, user_id).await?
        .ok_or_else(|| Error::Command(format!("Envelope '{}' not found", envelope_name)))?;

    envelope::ActiveModel {
        id: Set(envelope.id),
        is_deleted: Set(true),
        ..Default::default()
    }
    .update(db)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    Ok(format!("Envelope '{}' has been deleted", envelope_name))
}
```

#### 5.1.2. Transaction Operations (`core/transaction.rs`)
```rust
use sea_orm::*;
use crate::entities::{transaction, envelope, prelude::*};
use crate::errors::{Error, Result};
use chrono::Utc;

/// Execute a spend transaction atomically
pub async fn execute_spend_transaction(
    db: &DatabaseConnection,
    envelope_id: i64,
    current_balance: f64,
    amount: f64,
    description: &str,
    user_id: &str,
    message_id: Option<&str>,
) -> Result<SpendResult> {
    let txn = db.begin().await.map_err(|e| Error::Database(e.to_string()))?;

    // Update envelope balance
    let new_balance = current_balance - amount;
    envelope::ActiveModel {
        id: Set(envelope_id),
        balance: Set(new_balance),
        ..Default::default()
    }
    .update(&txn)
    .await
    .map_err(|e| Error::Database(e.to_string()))?;

    // Create transaction record
    let transaction_model = transaction::ActiveModel {
        envelope_id: Set(envelope_id),
        amount: Set(amount),
        description: Set(description.to_string()),
        timestamp: Set(Utc::now()),
        user_id: Set(user_id.to_string()),
        message_id: Set(message_id.map(|s| s.to_string())),
        transaction_type: Set("spend".to_string()),
        ..Default::default()
    };

    let created_transaction = transaction_model.insert(&txn).await
        .map_err(|e| Error::Database(e.to_string()))?;

    txn.commit().await.map_err(|e| Error::Database(e.to_string()))?;

    Ok(SpendResult {
        transaction_id: created_transaction.id,
        new_balance,
        overdraft: new_balance < 0.0,
    })
}

/// Get spending for current month for progress tracking
pub async fn get_actual_spending_this_month(
    db: &DatabaseConnection,
    envelope_id: i64,
    year: i32,
    month: u32,
) -> Result<f64> {
    let start_of_month = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .ok_or_else(|| Error::Command("Invalid date".to_string()))?
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| Error::Command("Invalid time".to_string()))?
        .and_utc();

    let next_month = if month == 12 {
        chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .ok_or_else(|| Error::Command("Invalid date".to_string()))?
    .and_hms_opt(0, 0, 0)
    .ok_or_else(|| Error::Command("Invalid time".to_string()))?
    .and_utc();

    let total: Option<f64> = Transaction::find()
        .filter(transaction::Column::EnvelopeId.eq(envelope_id))
        .filter(transaction::Column::TransactionType.eq("spend"))
        .filter(transaction::Column::Timestamp.gte(start_of_month))
        .filter(transaction::Column::Timestamp.lt(next_month))
        .select_only()
        .column_as(transaction::Column::Amount.sum(), "total")
        .into_tuple()
        .one(db)
        .await
        .map_err(|e| Error::Database(e.to_string()))?;

    Ok(total.unwrap_or(0.0))
}

#[derive(Debug)]
pub struct SpendResult {
    pub transaction_id: i64,
    pub new_balance: f64,
    pub overdraft: bool,
}
```

#### 5.1.3. Product Operations (`core/product.rs`)
```rust
use sea_orm::*;
use crate::entities::{product, envelope, prelude::*};
use crate::errors::{Error, Result};

/// Add a new product
pub async fn add_product(
    db: &DatabaseConnection,
    name: &str,
    total_price: f64,
    quantity: i64,
    envelope_name: &str,
    description: Option<&str>,
) -> Result<String> {
    // Find the envelope
    let envelope = Envelope::find()
        .filter(envelope::Column::Name.eq(envelope_name))
        .filter(envelope::Column::IsDeleted.eq(false))
        .one(db)
        .await
        .map_err(|e| Error::Database(e.to_string()))?
        .ok_or_else(|| Error::Command(format!("Envelope '{}' not found", envelope_name)))?;

    let unit_price = total_price / quantity as f64;

    let product_model = product::ActiveModel {
        name: Set(name.to_string()),
        price: Set(unit_price),
        envelope_id: Set(envelope.id),
        description: Set(description.map(|s| s.to_string())),
        ..Default::default()
    };

    product_model.insert(db).await
        .map_err(|e| Error::Database(e.to_string()))?;

    Ok(format!(
        "Product '{}' added with unit price ${:.2} (${:.2} ÷ {} units)",
        name, unit_price, total_price, quantity
    ))
}

/// Consume a product (create expense transaction)
pub async fn consume_product(
    db: &DatabaseConnection,
    product_name: &str,
    quantity: i64,
    user_id: &str,
    message_id: Option<&str>,
) -> Result<ProductConsumeResult> {
    // Find product with envelope information
    let product_with_envelope = Product::find()
        .filter(product::Column::Name.eq(product_name))
        .find_also_related(Envelope)
        .one(db)
        .await
        .map_err(|e| Error::Database(e.to_string()))?
        .ok_or_else(|| Error::Command(format!("Product '{}' not found", product_name)))?;

    let (product, envelope_opt) = product_with_envelope;
    let envelope = envelope_opt
        .ok_or_else(|| Error::Command("Product has invalid envelope reference".to_string()))?;

    if envelope.is_deleted {
        return Err(Error::Command(format!(
            "Product '{}' is linked to a deleted envelope '{}'",
            product_name, envelope.name
        )));
    }

    // Determine which envelope instance to use
    let target_envelope = if envelope.is_individual {
        // Find user's instance of this envelope type
        Envelope::find()
            .filter(envelope::Column::Name.eq(&envelope.name))
            .filter(envelope::Column::UserId.eq(user_id))
            .filter(envelope::Column::IsDeleted.eq(false))
            .one(db)
            .await
            .map_err(|e| Error::Database(e.to_string()))?
            .ok_or_else(|| Error::Command(format!(
                "You don't have an individual '{}' envelope",
                envelope.name
            )))?
    } else {
        envelope
    };

    let total_cost = product.price * quantity as f64;
    let description = format!("Product: {} ({}x)", product.name, quantity);

    // Execute the spend transaction
    let spend_result = crate::core::transaction::execute_spend_transaction(
        db,
        target_envelope.id,
        target_envelope.balance,
        total_cost,
        &description,
        user_id,
        message_id,
    ).await?;

    Ok(ProductConsumeResult {
        product_name: product.name,
        quantity,
        unit_price: product.price,
        total_cost,
        envelope_name: target_envelope.name,
        new_balance: spend_result.new_balance,
        overdraft: spend_result.overdraft,
    })
}

#[derive(Debug)]
pub struct ProductConsumeResult {
    pub product_name: String,
    pub quantity: i64,
    pub unit_price: f64,
    pub total_cost: f64,
    pub envelope_name: String,
    pub new_balance: f64,
    pub overdraft: bool,
}
```

### 5.2. Discord Command Handlers

#### 5.2.1. Spend Command (`bot/commands/transaction.rs`)
```rust
use poise::serenity_prelude as serenity;
use crate::bot::{Context, Error};
use crate::bot::autocomplete::envelope_name_autocomplete;
use crate::bot::utils::send_mini_report_embed;
use crate::core;
use tracing::{info, instrument};

/// Record an expense from an envelope.
///
/// Allows a user to specify an envelope they own or a shared envelope,
/// an amount, and an optional description to record a spending transaction.
/// The envelope's balance will be debited by the amount, and a transaction
/// record will be created. A mini-report showing the updated envelope status
/// will be sent as a reply.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn spend(
    ctx: Context<'_>,
    #[description = "Name of the envelope to spend from"]
    #[autocomplete = "envelope_name_autocomplete"]
    envelope_name: String,
    #[description = "Amount to spend (e.g., 50.00)"] 
    amount: f64,
    #[description = "Description of the expense (optional)"] 
    description: Option<String>,
    #[description = "User to spend on behalf of (for individual envelopes)"] 
    user: Option<serenity::User>,
) -> Result<(), Error> {
    let author_id = ctx.author().id.to_string();
    let target_user_id = user
        .as_ref()
        .map_or(author_id.clone(), |u| u.id.to_string());

    info!(
        "Spend command: user={}, envelope={}, amount={}, target_user={}",
        ctx.author().name, envelope_name, amount, target_user_id
    );

    if amount <= 0.0 {
        ctx.say("The amount must be a positive number.").await?;
        return Ok(());
    }

    let data = ctx.data();
    let db = &data.db;

    // Get the target envelope
    let envelope = core::envelope::get_user_or_shared_envelope(
        db, &envelope_name, &target_user_id
    ).await?
        .ok_or_else(|| Error::Command(format!(
            "Could not find an envelope named '{}' that you can use.",
            envelope_name
        )))?;

    // Permission check
    if user.is_some() && !envelope.is_individual {
        ctx.say(format!(
            "Cannot specify a user for a shared envelope like '{}'.",
            envelope_name
        )).await?;
        return Ok(());
    }

    let desc = description.as_deref().unwrap_or("No description");
    let message_id = ctx.id().to_string();

    // Execute the spend
    let result = core::transaction::execute_spend_transaction(
        db,
        envelope.id,
        envelope.balance,
        amount,
        desc,
        &author_id,
        Some(&message_id),
    ).await?;

    // Get updated envelope for mini-report
    let updated_envelope = core::envelope::get_user_or_shared_envelope(
        db, &envelope_name, &target_user_id
    ).await?
        .ok_or_else(|| Error::Command("Failed to fetch updated envelope".to_string()))?;

    // Send mini-report
    send_mini_report_embed(ctx, &updated_envelope, &data.app_config, db).await?;

    Ok(())
}
```

#### 5.2.2. Report Command (`bot/commands/envelope.rs`)
```rust
/// Shows current balances, allocations, and spending progress for all active envelopes.
#[poise::command(slash_command)]
#[instrument(skip(ctx))]
pub async fn report(ctx: Context<'_>) -> Result<(), Error> {
    info!("Full Report command received from user: {}", ctx.author().name);
    
    let data = ctx.data();
    let db = &data.db;
    let app_config = &data.app_config;

    let envelopes = core::envelope::get_all_active_envelopes(db).await?;

    if envelopes.is_empty() {
        ctx.say("No envelopes found.").await?;
        return Ok(());
    }

    // Generate report using core logic
    let report_embed = core::report::generate_full_report(&envelopes, app_config, db).await?;

    ctx.send(poise::CreateReply::default().embed(report_embed)).await?;
    Ok(())
}
```

### 5.3. Autocomplete Preservation
```rust
// bot/autocomplete.rs
use poise::serenity_prelude as serenity;
use crate::bot::Context;
use crate::core;

/// Provides autocomplete suggestions for envelope names
pub async fn envelope_name_autocomplete<'a>(
    ctx: Context<'_>,
    partial: &'a str,
) -> impl Iterator<Item = String> + 'a {
    let db = &ctx.data().db;
    let user_id = ctx.author().id.to_string();
    
    // Use core logic for suggestions
    let suggestions = core::envelope::suggest_accessible_envelope_names(
        db, partial, &user_id
    ).await.unwrap_or_default();

    suggestions.into_iter()
}

/// Provides autocomplete suggestions for product names
pub async fn product_name_autocomplete<'a>(
    ctx: Context<'_>,
    partial: &'a str,
) -> impl Iterator<Item = String> + 'a {
    let db = &ctx.data().db;
    
    let suggestions = core::product::suggest_product_names(
        db, partial
    ).await.unwrap_or_default();

    suggestions.into_iter()
}
```

## 6. Error Handling Unification

### 6.1. Single Error Type
```rust
// errors.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Command execution error: {0}")]
    Command(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Environment variable error: {0}")]
    EnvVar(#[from] std::env::VarError),

    #[error("SeaORM database error: {0}")]
    SeaOrm(#[from] sea_orm::DbErr),

    #[error("Discord framework error: {0}")]
    Discord(#[from] poise::serenity_prelude::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

### 6.2. Error Conversion Strategy
- **SeaORM Errors**: Automatically converted via `#[from]`
- **Business Logic Errors**: Use `Error::Command` for user-facing errors
- **Database Connection Errors**: Use `Error::Database` for system errors
- **Configuration Errors**: Use `Error::Config` for setup issues

## 7. Testing Strategy

### 7.1. Unit Testing Core Logic
```rust
// core/envelope.rs
#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{Database, DatabaseConnection};
    use crate::entities::*;

    async fn setup_test_db() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        
        // Run migrations
        crate::database::run_migrations(&db).await.unwrap();
        
        db
    }

    #[tokio::test]
    async fn test_create_envelope() {
        let db = setup_test_db().await;
        
        let result = create_or_reenable_envelope(
            &db,
            "test_envelope",
            Some("necessary"),
            Some(100.0),
            Some(false),
            Some(false),
            "user1",
            "user2",
        ).await;

        assert!(result.is_ok());
        
        // Verify envelope was created
        let envelope = get_user_or_shared_envelope(&db, "test_envelope", "user1").await.unwrap();
        assert!(envelope.is_some());
        assert_eq!(envelope.unwrap().allocation, 100.0);
    }

    #[tokio::test]
    async fn test_spend_transaction() {
        let db = setup_test_db().await;
        
        // Create envelope first
        create_or_reenable_envelope(&db, "test", Some("necessary"), Some(100.0), Some(false), Some(false), "user1", "user2").await.unwrap();
        let envelope = get_user_or_shared_envelope(&db, "test", "user1").await.unwrap().unwrap();
        
        // Execute spend
        let result = crate::core::transaction::execute_spend_transaction(
            &db,
            envelope.id,
            envelope.balance,
            50.0,
            "Test spend",
            "user1",
            None,
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().new_balance, 50.0);
    }
}
```

### 7.2. Integration Testing
```rust
// tests/integration_tests.rs
use envelope_buddy::*;
use sea_orm::{Database, DatabaseConnection};

#[tokio::test]
async fn test_full_spend_workflow() {
    let db = setup_test_database().await;
    let config = setup_test_config();
    
    // Create envelope
    let envelope_result = core::envelope::create_or_reenable_envelope(
        &db, "groceries", Some("necessary"), Some(500.0), Some(false), Some(false),
        &config.user_id_1, &config.user_id_2
    ).await;
    assert!(envelope_result.is_ok());
    
    // Spend from envelope
    let envelope = core::envelope::get_user_or_shared_envelope(&db, "groceries", &config.user_id_1).await.unwrap().unwrap();
    let spend_result = core::transaction::execute_spend_transaction(
        &db, envelope.id, envelope.balance, 100.0, "Grocery shopping", &config.user_id_1, None
    ).await;
    assert!(spend_result.is_ok());
    
    // Verify balance updated
    let updated_envelope = core::envelope::get_user_or_shared_envelope(&db, "groceries", &config.user_id_1).await.unwrap().unwrap();
    assert_eq!(updated_envelope.balance, 400.0);
    
    // Verify transaction recorded
    let spending = core::transaction::get_actual_spending_this_month(&db, envelope.id, 2024, 12).await.unwrap();
    assert_eq!(spending, 100.0);
}
```

## 8. Migration Implementation Plan

### 8.1. Week 1: Foundation Setup

#### Day 1: SeaORM Setup
- [ ] Add SeaORM dependencies to `Cargo.toml`
- [ ] Create entity definitions (`entities/` directory)
- [ ] Set up migration framework
- [ ] Create initial migration matching current schema
- [ ] Test basic database operations

#### Day 2: Database Layer
- [ ] Implement database connection management
- [ ] Create migration runner
- [ ] Test entity relationships
- [ ] Verify data integrity with existing database

#### Day 3-4: Core Business Logic
- [ ] Implement `core/envelope.rs` with all envelope operations
- [ ] Implement `core/transaction.rs` with transaction logic
- [ ] Implement `core/product.rs` with product operations
- [ ] Add comprehensive unit tests for core modules

#### Day 5: Report Generation
- [ ] Implement `core/report.rs` with report generation logic
- [ ] Implement `core/monthly.rs` with monthly update logic
- [ ] Test all core functionality thoroughly

### 8.2. Week 2: Bot Layer Migration

#### Day 1-2: Command Handler Refactoring
- [ ] Refactor all command handlers to use core layer
- [ ] Preserve all command signatures, descriptions, and parameters
- [ ] Maintain autocomplete functionality
- [ ] Test each command individually

#### Day 3: Embed and UX Preservation
- [ ] Ensure all Discord embeds render identically
- [ ] Preserve mini-report functionality
- [ ] Maintain all user-facing messages and formatting
- [ ] Test user experience thoroughly

#### Day 4: Integration Testing
- [ ] Run full integration tests
- [ ] Performance testing with realistic data
- [ ] Memory usage analysis
- [ ] Cross-platform testing

#### Day 5: Cleanup and Documentation
- [ ] Remove old database layer code
- [ ] Update documentation
- [ ] Final testing and bug fixes
- [ ] Prepare for deployment

## 9. Performance Considerations

### 9.1. Database Performance
- **Connection Pooling**: SeaORM provides built-in connection pooling
- **Query Optimization**: SeaORM generates optimized SQL queries
- **Indexing**: Preserve existing database indexes
- **Transaction Batching**: Use database transactions for multi-step operations

### 9.2. Memory Usage
- **Reduced Allocations**: SeaORM reduces string allocations compared to raw SQL
- **Connection Reuse**: Better connection management
- **Query Caching**: SeaORM caches prepared statements

### 9.3. Response Time
- **Async Operations**: All database operations remain async
- **Reduced Parsing**: No manual row parsing overhead
- **Type Safety**: Compile-time optimizations

## 10. Risk Assessment and Mitigation

### 10.1. High Risk Items
1. **Data Migration**: Risk of data loss during schema changes
   - **Mitigation**: Comprehensive backup strategy, gradual migration
2. **Functionality Regression**: Risk of losing existing features
   - **Mitigation**: Extensive testing, feature parity checklist
3. **Performance Degradation**: Risk of slower performance
   - **Mitigation**: Performance benchmarking, optimization

### 10.2. Medium Risk Items
1. **SeaORM Learning Curve**: Team unfamiliarity with ORM
   - **Mitigation**: Documentation, examples, gradual adoption
2. **Deployment Complexity**: Changes to deployment process
   - **Mitigation**: Docker containerization, automated deployment

### 10.3. Low Risk Items
1. **Dependency Updates**: New dependencies to manage
   - **Mitigation**: Regular dependency audits
2. **Code Review Overhead**: More code to review during migration
   - **Mitigation**: Incremental reviews, pair programming

## 11. Success Criteria

### 11.1. Functional Requirements
- [ ] All existing Discord commands work identically
- [ ] All autocomplete functionality preserved
- [ ] All embed formatting and user experience identical
- [ ] All business logic produces same results
- [ ] All configuration and deployment processes unchanged

### 11.2. Non-Functional Requirements
- [ ] Code complexity reduced by >40%
- [ ] Database query code reduced by >60%
- [ ] Unit test coverage increased to >80%
- [ ] Build time improved or maintained
- [ ] Runtime performance maintained or improved

### 11.3. Maintainability Requirements
- [ ] New features can be added without touching Discord code
- [ ] Business logic can be tested independently
- [ ] Database schema changes use migration system
- [ ] Error handling is consistent across all modules
- [ ] Code follows consistent patterns and conventions

## 12. Post-Migration Benefits

### 12.1. Developer Experience
- **Easier Debugging**: Type-safe queries with better error messages
- **Faster Development**: Less boilerplate code for database operations
- **Better Testing**: Core logic can be unit tested without Discord
- **Clearer Architecture**: Separation of concerns makes code easier to understand

### 12.2. Maintenance Benefits
- **Reduced Bug Surface**: Type safety prevents many runtime errors
- **Easier Refactoring**: Clear module boundaries make changes safer
- **Better Documentation**: Self-documenting code through types
- **Simplified Debugging**: Fewer layers of abstraction

### 12.3. Future Development
- **New Features**: Easier to add new functionality
- **API Potential**: Core logic could be exposed via REST API
- **Multiple Frontends**: Could support web interface, CLI tools
- **Data Analysis**: Easier to write reports and analytics

## 13. Conclusion

The V2 architecture represents a significant improvement in code quality, maintainability, and developer experience while preserving 100% of the existing functionality that users depend on. The migration to SeaORM eliminates the complexity of raw SQL management while the new layered architecture creates clear separation of concerns.

The estimated 5-7 day migration timeline is realistic given the comprehensive planning and incremental approach. The benefits of improved maintainability, testability, and future development velocity far outweigh the short-term migration effort.

This design provides a solid foundation for future enhancements while ensuring that the current user experience remains unchanged during and after the migration.

## 14. Future Frontend Support

### 14.1. API Layer (Future)
- **REST API**: HTTP endpoints for alternative frontends
- **Data Models**: Clean serialization for any frontend
- **Authentication**: API key or JWT support
- **CORS**: Web frontend support

### 14.2. Alternative Frontends
- **Apple Watch**: REST API integration
- **iPhone App**: REST API integration
- **Web Interface**: REST API integration
- **CLI Tool**: Direct core logic usage