EnvelopeBuddy Design Document (Updated)
EnvelopeBuddy is a Discord bot designed to help a couple track shared and individual expenses using an envelope-style budgeting system. The bot utilizes slash commands, features detailed reporting, and includes mechanisms for quick expense logging. It focuses on maintaining current envelope balances and is intended to run on a Raspberry Pi.

1. Core Concepts
1.1. Envelopes

Types: Envelopes can be for short-term expenses (resetting monthly) or longer-term savings/spending (carrying over a balance).
Categories: Each envelope is assigned a category, typically "necessary" (e.g., groceries) or "quality_of_life" (e.g., eating out), for classification and potential future reporting.
Ownership: Envelopes can be shared between the couple or individual.
Individual envelope "types" (e.g., "Hobby") are instantiated for both users, each with their own balance and allocation.
Attributes: Each envelope has a monthly allocation amount and a current_balance.
Rollover: A boolean flag indicates if the remaining balance should roll over to the next month or if the envelope should reset to its allocation.
Soft Delete & Re-enable: Envelopes can be soft-deleted (marked inactive). Attempting to create an envelope with the same name (and user context for individual envelopes) as a soft-deleted one will re-enable it, optionally updating its attributes. The is_individual status of an envelope is fixed upon its initial creation and cannot be changed by re-enabling.
1.2. Transactions

Represent financial activities (spending, adding funds, product usage).
Each transaction is linked to an envelope and records the amount, description, user who initiated it, timestamp, and type (e.g., "spend", "deposit").
1.3. Products

Fixed-price items that map to a specific pre-defined envelope instance.
Users can define products with a name, unit price, and linked envelope.
Using a product deducts its price (multiplied by quantity) from the linked envelope.
1.4. Shortcuts

(Implementation currently deferred. To be revisited if desired.)
Original Concept: Command templates with parameters for variable-price items, allowing users to quickly execute pre-filled commands (primarily for spending) by providing a variable amount and an optional note.
1.5. Splurge Mechanism (Future Plan)

Allows a single expense to be split between a primary target envelope and a dedicated "Splurge" envelope.
The "Splurge" envelope is an individual type (one instance per user, or a shared couple's fund). Its funding is handled manually (e.g., via /addfunds or future custom allocation cycles), not via a standard monthly allocation.
Users will specify the total expense and the amount to be covered by the "Splurge" envelope; the remainder comes from the primary envelope.
This will likely be a subcommand or option on /spend.
Transactions will ideally be recorded as a single logical event with multiple legs, or clearly linked.
Mini-reports after a splurge will show the impact on both affected envelopes.
1.6. Recurring Subscriptions (Future Plan)

Automated logging of recurring expenses (and potentially income).
Users will define subscriptions (name, amount, envelope, frequency, day of processing).
A scheduling mechanism (e.g., daily check) within the bot will process due subscriptions.
If an envelope has insufficient funds, the transaction will still be logged (overdraft), and users will be notified/pinged.
Successful processing will also trigger a notification.
Management commands (/subscription add/list/delete/edit) will be needed.
1.7. Savings Goals (Future Plan)

Users can define a target amount for a specific (likely rollover) envelope.
The bot can report progress and potentially notify when a goal is met, possibly integrated with the daily/update mechanism.
2. Data Model
2.1. envelopes

SQL
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
2.2. transactions

SQL
CREATE TABLE IF NOT EXISTS transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    envelope_id INTEGER NOT NULL,
    amount REAL NOT NULL,           -- Always positive, transaction_type indicates direction
    description TEXT NOT NULL,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_id TEXT NOT NULL,          -- Discord User ID of the person who initiated
    message_id TEXT,                -- Discord interaction/message ID for reference
    transaction_type TEXT NOT NULL, -- e.g., 'spend', 'deposit', 'adjustment', 'splurge_primary', 'splurge_fund', 'subscription'
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
2.3. products

SQL
CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    price REAL NOT NULL,        -- Unit price
    envelope_id INTEGER NOT NULL,
    description TEXT,
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE -- Or SET NULL if preferred
);
2.4. shortcuts

(Schema based on original design; implementation deferred)

SQL
CREATE TABLE IF NOT EXISTS shortcuts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    command_template TEXT NOT NULL -- e.g., "spend <envelope_name> {} <base_description>"
);
2.5. system_state

SQL
CREATE TABLE IF NOT EXISTS system_state (
    key TEXT PRIMARY KEY,
    value TEXT
);
2.6. savings_goals (Future Table)

SQL
CREATE TABLE IF NOT EXISTS savings_goals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    goal_name TEXT NOT NULL UNIQUE,
    envelope_id INTEGER NOT NULL,
    target_amount REAL NOT NULL,
    target_date TEXT, -- ISO8601 date string, optional
    notes TEXT,
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
2.7. recurring_transactions (Future Table for Subscriptions/Income)

SQL
CREATE TABLE IF NOT EXISTS recurring_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    amount REAL NOT NULL, -- Positive for income, negative for expense, or use type
    transaction_type TEXT NOT NULL, -- 'expense' or 'income'
    envelope_id INTEGER NOT NULL,
    frequency TEXT NOT NULL, -- e.g., "monthly", "yearly", "weekly"
    day_of_month INTEGER, -- For monthly (1-31, handle short months)
    month_of_year INTEGER, -- For yearly (1-12)
    day_of_week INTEGER, -- For weekly (0-6)
    description TEXT,
    next_due_date DATETIME NOT NULL,
    user_id TEXT NOT NULL, -- Who defined it/whose context if individual envelope
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
3. Configuration
.env file:
DISCORD_BOT_TOKEN: The bot's Discord token.
DATABASE_PATH: Path to the SQLite database file (e.g., data/envelope_buddy.sqlite).
COUPLE_USER_ID_1: Discord User ID of the first user.
COUPLE_USER_ID_2: Discord User ID of the second user.
USER_NICKNAME_1: Nickname for User 1 (e.g., "PartnerA").
USER_NICKNAME_2: Nickname for User 2 (e.g., "PartnerB").
RUST_LOG: Logging configuration (e.g., envelope_buddy=debug,info).
DEV_GUILD_ID (Optional): For fast registration of slash commands to a specific test server.
LOG_CHANNEL_ID (Future): Channel ID for persistent logging of database-modifying commands.
config.toml file:
Defines the initial set of envelopes (name, category, allocation, is_individual, rollover). These are seeded into the database on first run or if missing (with re-enable logic).
4. Command System (Poise Framework with Slash Commands)
4.1. Implemented Commands

General:
/ping: Replies "Pong!" to check bot responsiveness.
Envelope Management:
/report: Displays a full report of all active envelopes in an embed, including balance, allocation, actual amount spent this month, expected spending pace for the month, and a status indicator (🟢🟡🔴⚪).
/create_envelope name:<name> category:<cat> allocation:<amt> is_individual:<bool> rollover:<bool>: Creates a new envelope. If an envelope with the same name (and user context for individual) was soft-deleted, it re-enables it and updates its properties based on provided optional parameters (otherwise, old properties are kept; balance resets to new/effective allocation). For new individual types, creates instances for both configured users.
/delete_envelope name:<name>: Soft-deletes an envelope (sets is_deleted = true). User is informed it can be re-enabled.
/update: Performs the monthly envelope processing:
Checks system_state to prevent multiple runs for the same month.
For each active envelope:
If rollover = true, balance = balance + allocation.
If rollover = false, balance = allocation.
Prunes transactions older than a defined period (e.g., ~13 months).
Updates system_state with the month processed (e.g., "YYYY-MM").
Transaction Posting:
/spend envelope_name:<env> amount:<amt> [description:<desc>]: Records a spend transaction from the specified envelope (user's own individual or shared). Autocompletes envelope names. Replies with a mini-report embed for the affected envelope.
/addfunds envelope_name:<env> amount:<amt> [description:<desc>]: Adds funds to the specified envelope. Records a "deposit" transaction. Autocompletes envelope names. Replies with a mini-report embed.
Product Management (as subcommands of /product):
/product add name:<name> total_price:<price> envelope:<env_name> [quantity:<num>] [description:<text>]: Adds a new product, calculating unit price from total price and quantity (quantity defaults to 1). envelope_name uses autocomplete.
/product list: Lists all defined products and their details (price, linked envelope) in an embed.
/product update name:<name> total_price:<price> [quantity:<num>]: Updates an existing product's unit price, calculated from total price and quantity (quantity defaults to 1). name uses autocomplete.
/product consume name:<name> [quantity:<num>]: Records an expense by "consuming" a product. Debits the product's linked envelope. If the product is linked to an individual envelope "type" (e.g., "Hobby"), it debits the command issuer's specific instance of that "Hobby" envelope. name uses autocomplete. Quantity defaults to 1. Replies with a mini-report.
4.2. Planned Future Commands & Features

Splurge Spending:
Modify /spend or create /splurgespend to allow splitting an expense between a primary envelope and the dedicated "Splurge" envelope. User specifies total expense and amount for splurge.
Undo Command:
/undo: Reverts the effects of the last global financial transaction (spend, addfunds, product/shortcut use). Shows a confirmation and mini-report. Handles overdrafts by allowing balance to go negative.
Transaction History (Enhanced):
/history envelope_name:<env> [month:<1-12>] [year:<YYYY>]: Displays transactions for the specified envelope and period (defaults to current month/year). Formats output clearly (date, description, amount with sign, type).
Transaction Editing:
(Re-evaluation needed) Original idea: reply-based editing. For slash commands, a dedicated /edit_transaction <id> ... might be more appropriate if this feature is pursued.
Recurring Subscriptions / Income:
/subscription add name:<name> amount:<amt> envelope:<env> frequency:<freq> day:<day_of_month_or_week> [description:<desc>]: Define recurring expenses.
/recurring_income add ...: Similar for recurring income.
/subscription list, /subscription delete, /subscription edit.
Automatic daily processing by the bot. Notifications for processing and overdrafts.
Savings Goals:
/goal set <envelope_name> <goal_name> <target_amount> [target_date].
/goal remove <goal_name>.
/goal list.
Automatic check and notification on goal met (e.g., during daily process).
Fund Transfers:
/transfer <amount> from_envelope:<source> to_envelope:<target>.
Define rules for allowed transfer paths (e.g., individual to shared, individual A to individual B).
Product/Shortcut Management:
/product delete <name> (uses product_name_autocomplete).
/shortcut add <name> <template_string> (if shortcuts are re-prioritized).
/shortcut delete <name> (if shortcuts are re-prioritized).
/shortcuts list (if shortcuts are re-prioritized).
5. Message Processing and UX
Slash Commands: Primary interaction method via poise.
Autocomplete: Implemented for envelope names, product names, and shortcut names to improve UX. Case-insensitive.
Mini-Reports: Embeds showing the status of an affected envelope are sent as part of the reply to commands like /spend, /addfunds, /product consume.
Dual Output (Future):
Commands modifying the database will provide an ephemeral reply to the command issuer.
A non-ephemeral, persistent copy of this reply/log will be sent to a designated log channel (ID set via .env or command).
The log channel will receive a daily full /report triggered by the bot's internal scheduler.
6. Key Operations Logic (Implemented)
Monthly Update (/update): Handles rollovers/resets based on envelope.rollover. Prevents re-run for the same month using system_state. Prunes transactions older than ~13 months.
Spending Progress Tracking (/report): Calculates daily allocation, expected spending vs. actual spending (from transactions for the month for display, and allocation - balance for pace indicator) for the current month. Displays 🟢🟡🔴⚪ indicators.
Initial Data Seeding: config.toml provides initial envelope definitions. On startup, these are seeded into the database, respecting the "re-enable if soft-deleted" logic. Individual envelope types from config create two instances, one for each configured user.
7. Implementation Stack (Current)
Language: Rust
Discord Interaction: serenity crate, with poise framework for slash commands.
Database: SQLite via rusqlite.
Async Runtime: tokio.
Logging: tracing with tracing-subscriber.
Configuration: toml for config.toml, dotenvy for .env.
Date/Time: chrono.
8. Installation and Setup
Deployment target: Raspberry Pi.
Configuration via .env (for secrets and instance-specifics) and config.toml (for initial envelope structure).
(Future: Systemd service for automatic startup on Pi).