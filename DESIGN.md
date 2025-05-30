# EnvelopeBuddy Design Document 

## 1. Introduction

EnvelopeBuddy is a Discord bot designed to help a couple track shared and individual expenses using an envelope-style budgeting system. The bot utilizes slash commands, features detailed reporting with progress indicators, and includes mechanisms for quick expense logging via predefined products. It focuses on maintaining current envelope balances, is built in Rust for performance and reliability, and is intended for self-hosting (e.g., on a Raspberry Pi). This document outlines its current features, planned enhancements, and development standards.

## 2. Core Concepts

### 2.1. Envelopes
* **Definition:** Logical containers for budgeting specific categories of expenses.
* **Types:** Envelopes can be for short-term expenses (potentially resetting monthly) or longer-term savings/spending (carrying over a balance).
* **Categories:** Each envelope is assigned a category (e.g., "necessary", "quality\_of\_life") for classification, potential future reporting, and budgeting strategy.
* **Ownership:** Envelopes can be **shared** between the two configured users or **individual**.
    * Individual envelope "types" (e.g., an envelope named "Hobby") are instantiated for *both* configured users, each with their own separate balance and benefiting from the type's defined allocation.
* **Attributes:** Each envelope instance has a monthly `allocation` amount and a `current_balance`.
* **Rollover:** A boolean flag (`rollover`) indicates if the remaining balance should be added to the next month's allocation or if the envelope should reset to its base allocation amount.
* **Soft Delete & Re-enable:** Envelopes can be soft-deleted (marked as `is_deleted = true`). Attempting to create an envelope via `/manage envelope create` with the same identifying characteristics (name for shared; name and user context for individual) as a soft-deleted one will re-enable it. When re-enabling, users can optionally provide new attributes (category, allocation, rollover); if not provided, old attributes are kept. The balance is reset to the effective (new or old) allocation upon re-enablement. The `is_individual` status of an envelope is fixed upon its initial creation and cannot be changed by re-enabling.

### 2.2. Transactions
* Represent all financial activities affecting envelopes.
* Each transaction is linked to a specific envelope instance.
* Records the `amount` (always positive), `description`, the `user_id` of the person initiating the transaction, a `timestamp`, and a `transaction_type` (e.g., "spend", "deposit", "adjustment"). Future types include "splurge\_primary", "splurge\_fund", "subscription\_expense", "recurring\_income".

### 2.3. Products
* Globally defined, fixed-price items that map to a specific "template" envelope instance (either a shared envelope or an individual envelope type name).
* Users define products with a name, unit price (calculated from total price and optional quantity at creation/update), and the name of the envelope it's linked to.
* When a product is "consumed" (used via `/product <product_name>`), an expense is recorded. If the product is linked to an individual envelope type, the expense is debited from the command issuer's specific instance of that individual envelope type. If linked to a shared envelope, the shared envelope is debited.

### 2.4. Shortcuts (Implementation Currently Deferred)
* User-defined aliases for frequently used variable-price spend operations.
* A shortcut definition would store a name and a `command_template` string (e.g., `"spend CoffeeFund {} Morning coffee run"` where `{}` is a placeholder for the amount).
* Using a shortcut would involve providing the shortcut name, an amount, and an optional note, which then get parsed from the template to execute a spend.

### 2.5. Splurge Mechanism (Future Plan)
* Allows a single expense to be split between a primary target envelope and a dedicated "Splurge" envelope.
* **Splurge Envelope:** A dedicated envelope, treated as an individual type (one instance per configured user, or a shared "couple's fund"). Its funding is handled manually (e.g., via `/addfunds`) or via future custom allocation cycles (e.g., semi-yearly). Specific naming and setup TBD.
* **Invocation:** User specifies the total expense and the amount to be covered by their "Splurge" envelope; the remainder comes from the primary envelope. Likely an option on `/spend` or a dedicated subcommand like `/manage spend splurge ...`.
* **Transaction Recording:** Ideally recorded as a single logical event with multiple financial legs, or as clearly linked transactions.
* **Feedback:** Mini-reports after a splurge spend will show the impact on *both* affected envelopes.
* **Overdraft Handling:** To be determined if either leg can overdraft.

### 2.6. Recurring Transactions (Subscriptions & Income - Future Plan)
* Automated logging for recurring expenses and income.
* **Definition:** Users define items with name, amount, `transaction_type` ("expense" or "income"), target envelope, frequency (e.g., monthly, yearly, weekly), and processing day/date.
* **Execution:** A daily scheduling mechanism within the bot will process due items.
* **Failure Handling:** If an envelope has insufficient funds for an expense, the transaction will still be logged (creating an overdraft), and users will be pinged/notified.
* **Notification:** Users will be notified when a recurring item is successfully processed.
* **Management Commands:** Grouped under `/manage subscription ...` or `/manage income ...`.
* **Open Questions:** Precise handling for variable month lengths (e.g., day 31 in February), exact frequency options.

### 2.7. Savings Goals (Future Plan)
* Users can define named goals with a target amount (and optional target date) linked to a specific (likely rollover) envelope.
* Commands will be grouped under `/manage goal ...`.
* The bot can report progress and will notify (e.g., via daily scheduler or during `/update`) when a goal is met.

## 3. Data Model (SQLite)

### 3.1. `envelopes`
```sql
CREATE TABLE IF NOT EXISTS envelopes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    allocation REAL NOT NULL,
    balance REAL NOT NULL,
    is_individual BOOLEAN NOT NULL DEFAULT FALSE,
    user_id TEXT, -- Discord User ID. NULL if shared.
    rollover BOOLEAN NOT NULL DEFAULT FALSE,
    is_deleted BOOLEAN NOT NULL DEFAULT FALSE -- For soft deletes
);
-- Index for unique shared envelope names (active or soft-deleted)
CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_shared_envelope_name
    ON envelopes(name)
    WHERE user_id IS NULL;
-- Index for unique individual envelope names per user (active or soft-deleted)
CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_individual_envelope_name_user
    ON envelopes(name, user_id)
    WHERE user_id IS NOT NULL;
```

### 3.2. `transactions`
```sql
CREATE TABLE IF NOT EXISTS transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    envelope_id INTEGER NOT NULL,
    amount REAL NOT NULL,           -- Always positive, transaction_type indicates direction
    description TEXT NOT NULL,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_id TEXT NOT NULL,          -- Discord User ID of the person who initiated
    message_id TEXT,                -- Discord interaction/message ID for reference
    transaction_type TEXT NOT NULL, -- 'spend', 'deposit', 'adjustment', 'splurge_primary', 'splurge_fund', 'subscription_expense', 'recurring_income'
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
```

### 3.3. `products`
```sql
CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    price REAL NOT NULL,        -- Unit price
    envelope_id INTEGER NOT NULL,
    description TEXT,
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
```

### 3.4. `shortcuts` (Implementation Deferred)
```sql
CREATE TABLE IF NOT EXISTS shortcuts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    command_template TEXT NOT NULL -- e.g., "spend EnvelopeName {} Base Description"
);
```

### 3.5. `system_state`
```sql
CREATE TABLE IF NOT EXISTS system_state (
    key TEXT PRIMARY KEY,
    value TEXT -- e.g., key='last_update_processed_month', value='2025-05'
);
```

### 3.6. `savings_goals` (Future Table)
```sql
CREATE TABLE IF NOT EXISTS savings_goals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    goal_name TEXT NOT NULL UNIQUE,
    envelope_id INTEGER NOT NULL,
    target_amount REAL NOT NULL,
    target_date TEXT, -- ISO8601 date string, optional
    notes TEXT,
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
```

### 3.7. `recurring_transactions` (Future Table)
```sql
CREATE TABLE IF NOT EXISTS recurring_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    amount REAL NOT NULL,
    transaction_type TEXT NOT NULL, -- 'expense' or 'income'
    envelope_id INTEGER NOT NULL,
    frequency TEXT NOT NULL, -- e.g., "monthly", "yearly", "weekly:MONDAY", "days:14", "bimonthly:15_and_last"
    day_of_month INTEGER, 
    month_of_year INTEGER, 
    day_of_week INTEGER, 
    description TEXT,
    next_due_date DATETIME NOT NULL,
    last_processed_date DATETIME,
    user_id TEXT NOT NULL, 
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
```

## 4. Configuration

* **`.env` file:** For secrets and instance-specific settings.
    * `DISCORD_BOT_TOKEN`
    * `DATABASE_PATH` (e.g., `data/envelope_buddy.sqlite`)
    * `COUPLE_USER_ID_1`, `COUPLE_USER_ID_2`
    * `USER_NICKNAME_1`, `USER_NICKNAME_2`
    * `RUST_LOG` (e.g., `envelope_buddy=info`)
    * `DEV_GUILD_ID` (Optional, for fast command registration during development)
    * `BACKUP_CHANNEL_ID` (For bot-assisted backup to Discord feature)
    * `LOG_CHANNEL_ID` (Future, for persistent command logging)
* **`config.toml` file:**
    * Defines the initial set of envelopes (name, category, allocation, is\_individual, rollover). Seeded into the database.

## 5. Command System (Poise Framework with Slash Commands)

Commands are structured into top-level "action" commands for frequent use and grouped "management" commands under `/manage`.

### 5.1. Action Commands (Top-Level)

* **Implemented:**
    * `/ping`: Replies "Pong!"
    * `/report`: Full embed report of active envelopes (balance, allocation, actual spent, expected pace, status indicators).
    * `/spend <envelope_name> <amount> [description]`: Records a spend. Autocompletes envelope. Replies with an ephemeral message + mini-report. (Future: logs to channel).
    * `/addfunds <envelope_name> <amount> [description]`: Adds funds. Records "deposit". Autocompletes envelope. Replies with ephemeral message + mini-report. (Future: logs to channel).
    * `/product <name_to_use> [quantity] [note]`: Uses a predefined product to log an expense. `name_to_use` uses autocomplete. Quantity defaults to 1. Note appends to description. Replies with ephemeral message + mini-report. (Future: logs to channel).
    * `/update`: Performs monthly envelope rollovers/resets and prunes old transactions.
* **Planned Future Action Commands (Top-Level):**
    * `/history <envelope_name> [month:<num>] [year:<num>]`: Displays transaction history.
    * `/undo`: Reverts the last global financial transaction.
    * `/splurgespend` (or similar, if not an option on `/spend`): For splurge mechanism.
    * `/shortcut <name_to_use> <amount> [note]` (If Shortcuts are implemented).

### 5.2. Management Commands (Grouped under `/manage`)

* **Parent Command:** `/manage`
    * Running `/manage` by itself shows help for its available entity subcommands.

* **Implemented Management Subcommands:**
    * `/manage envelope create name:<name> category:<cat> allocation:<amt> [is_individual:<bool>] [rollover:<bool>]`: Creates or re-enables/updates a soft-deleted envelope.
    * `/manage envelope delete name:<name>`: Soft-deletes an envelope.
    * `/manage product add name:<name> total_price:<price> envelope:<env_name> [quantity:<num>] [description:<text>]`: Adds a new product (unit price calculated).
    * `/manage product list`: Lists all defined products.
    * `/manage product update name:<name> total_price:<price> [quantity:<num>]`: Updates a product's unit price.
    * `/manage product delete name:<name>`: Deletes a product.
* **Planned Future Management Subcommands:**
    * `/manage envelope set_balance <envelope_name> <balance> [user:<user>] [reason:<text>]`
    * `/manage envelope edit <name> [new_attributes...]`
    * `/manage subscription add/list/delete/edit ...`
    * `/manage recurring_income add/list/delete/edit ...`
    * `/manage goal set/list/remove ...`
    * `/manage set_log_channel <channel>`
    * `/manage backup create_and_upload`
    * `/manage shortcut add/list/delete ...` (If Shortcuts are implemented).

## 6. Message Processing and UX

* **Interaction Model:** Primarily Slash Commands using `poise`.
* **Autocomplete:** Case-insensitive for envelope names, product names, (future) shortcut names, powered by in-memory caches that refresh on CRUD operations and startup.
* **Feedback:** Embeds for reports. Mini-report embeds after data-modifying actions.
* **Confirmations (Future):** For destructive actions (e.g., delete commands), use button-based confirmations.
* **Help System (Future):** A comprehensive custom help command that correctly reflects the grouped command structure.
* **Dual Output (Future Plan):**
    * Database-modifying commands: Ephemeral reply to issuer; persistent log to `LOG_CHANNEL_ID`.
    * Daily automated full `/report` to `LOG_CHANNEL_ID`.

## 7. Key Operations Logic

* **Monthly Update (`/update`):** Implemented. Handles rollovers/resets. Prevents re-run for the same month. Prunes transactions older than ~13 months. Will prompt to reconcile overdrafts.
* **Spending Progress Tracking (`/report`):** Implemented. Calculates daily allocation, expected spending vs. "spent from allocation" (for indicator emoji) and "actual spent this month" (for display). Displays 🟢🟡🔴⚪ indicators.
* **Initial Data Seeding (`config.toml`):** Implemented with "re-enable if soft-deleted" logic for envelopes.
* **Cache Management:** Implemented for autocomplete (envelope info, product names); populated on startup and refreshed after relevant CRUD operations.
* **Scheduling Mechanism (Future):** Required for automated daily reports, processing recurring transactions, and savings goal notifications. Will likely run as a `tokio::task` within the bot, checking daily at a set time (e.g., midnight Pi local time).
* **Overdraft Handling:** Overdrafts are allowed. A warning will be issued when an overdraft occurs. Monthly process may prompt for reconciliation.

## 8. Logging

* **Engine:** `tracing` with `tracing-subscriber`.
* **Output:** Configured to log to both `stdout`/`stderr` (for development/direct execution) AND a local file (e.g., `envelope_buddy.log`) using `tracing_appender` for rotation and non-blocking writes.
* **Level:** Configurable via `RUST_LOG` environment variable.

## 9. Coding & Documentation Standards

**A. For User-Exposed Slash Commands**
1.  **Clarity and Naming:**
    * Top-level commands for frequent actions (e.g., `/spend`, `/product` (for using)) are direct and intuitive.
    * Management/setup commands are grouped under `/manage` (e.g., `/manage envelope create`).
2.  **Descriptions (Discord UI):**
    * Every command, subcommand group, subcommand, and parameter **must** have a clear, concise `description` in its `#[poise::command(...)]` or parameter attribute, adhering to Discord's character limits.
    * Parent command descriptions summarize purpose or list subcommands.
3.  **Parameters:**
    * Consistent, descriptive, `snake_case` names.
    * Use `Option<T>` for optional parameters; default behavior documented in `description`.
    * Employ `#[autocomplete = "..."]` for parameters selecting from existing entities. Autocomplete must be case-insensitive.
    * Input validation at the start of command logic; user-friendly error messages for invalid input.
4.  **Output & Feedback:**
    * **Success:** Clear confirmation messages. Use Discord Embeds for structured data.
    * **Errors:** User-friendly error messages, avoiding raw internal details. Log detailed errors internally.
    * **Dual Output (Future Standard):** Data-modifying commands: ephemeral reply to issuer, persistent log to `LOG_CHANNEL_ID`.
5.  **Testing:** Logic within commands should be testable, potentially by extracting to helpers. Manual end-to-end testing in Discord is essential.

**B. For All Rust Functions (Backend, DB, Helpers, etc.)**
1.  **Documentation Comments:** `///` for all `pub` items and important `pub(crate)` items (purpose, parameters, returns, errors/panics).
2.  **Internal Comments:** `//` for complex or non-obvious logic.
3.  **Unit Tests:** Comprehensive unit tests for all `db` module functions, `cache` module functions, and significant logic helpers. Cover success paths, errors, edge cases. Use in-memory SQLite for DB tests. Use `init_test_tracing()`.
4.  **Error Handling:** Consistent use of `crate::errors::Error` and `Result<T, Error>`. Propagate with `?`. Map underlying errors into custom `Error` variants. Avoid `unwrap()`/`expect()` in application logic (ok in tests for setup).
5.  **Logging (`tracing`):** Consistent use of `trace!`, `debug!`, `info!`, `warn!`, `error!`. Use `#[instrument]` judiciously, skipping sensitive data.
6.  **Modularity & Readability:** Adhere to `cargo fmt`. Keep functions/modules focused. Descriptive names. Run `cargo clippy` and address lints (including `result_large_err` by boxing error variants or returning `Box<Error>`).
7.  **Currency Precision (Future Consideration):** Note potential future need to switch from `f64` to a decimal type or integer (cents) for currency if precision issues arise.

## 10. Data Backup Strategy
* Regular off-Pi backups of the `envelope_buddy.sqlite` database are critical.
* SQLite database files are expected to remain small for personal use.
* **User-Initiated Method (Primary for now):**
    * `/manage backup create_and_upload`: Creates an immediate backup and uploads it to the Discord channel specified by `BACKUP_CHANNEL_ID` in `.env`.
* **Future Automated Options:**
    * Scheduled backups to personal cloud storage (e.g., using `rclone` via a cron job on the Pi).
    * Bot reminder to prompt users to run the manual backup command.

## 11. Future Architectural Improvements (Deferred)
* **Refactor Database Layer to use an ORM:** Consider migrating from `rusqlite` with raw SQL to an ORM like `SeaORM` for more idiomatic Rust database interactions and to reduce manual SQL.
* **Decouple Core Logic Layer from Frontend (Poise/Discord):** To allow the core budgeting engine to be potentially used by other frontends and improve testability of business logic independent of Discord.
* **Improve Atomicity for Complex Operations:** Ensure multi-step database operations (e.g., monthly update, splurge) are fully transactional.

## 12. Implementation Stack

* **Language:** Rust
* **Discord Interaction:** `serenity` crate, `poise` framework.
* **Database:** SQLite via `rusqlite`.
* **Async Runtime:** `tokio`.
* **Logging:** `tracing`, `tracing-subscriber`, `tracing-appender` (for file logging).
* **Configuration:** `toml` crate for `config.toml`, `dotenvy` for `.env`.
* **Date/Time:** `chrono`.

## 13. Installation and Setup

* **Target Deployment:** Raspberry Pi.
* **Compilation:** Cross-compilation from a development machine using `cross` is recommended.
* **Deployment:** Transfer compiled binary, `.env`, and `config.toml` to a dedicated application directory on the Pi (e.g., `/home/pi/envelope_buddy_app/`).
* **Persistence:** Runs as a `systemd` service for automatic startup and management.
* **Remote Development:** SSH access with key-based authentication. VS Code Remote - SSH for managing the Pi. VS Code Tasks on the local development machine for automating the build-deploy-restart cycle.