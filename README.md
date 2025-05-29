# EnvelopeBuddy 

EnvelopeBuddy is a Discord bot designed to help a couple track shared and individual expenses using an envelope-style budgeting system. The bot utilizes slash commands, features detailed reporting, and includes mechanisms for quick expense logging. It focuses on maintaining current envelope balances and is intended to run on a Raspberry Pi.

## 1. Core Concepts

### 1.1. Envelopes
* **Types:** Envelopes can be for short-term expenses (resetting monthly) or longer-term savings/spending (carrying over a balance).
* **Categories:** Each envelope is assigned a category, typically "necessary" (e.g., groceries) or "quality\_of\_life" (e.g., eating out), for classification and potential future reporting.
* **Ownership:** Envelopes can be **shared** between the couple or **individual**.
    * Individual envelope "types" (e.g., "Hobby") are instantiated for *both* users, each with their own balance and allocation.
* **Attributes:** Each envelope has a monthly `allocation` amount and a `current_balance`.
* **Rollover:** A boolean flag indicates if the remaining balance should roll over to the next month or if the envelope should reset to its allocation.
* **Soft Delete & Re-enable:** Envelopes can be soft-deleted (marked inactive). Attempting to create an envelope with the same name (and user context for individual envelopes) as a soft-deleted one will re-enable it, optionally updating its attributes. The `is_individual` status of an envelope is fixed upon its initial creation and cannot be changed by re-enabling.

### 1.2. Transactions
* Represent financial activities (spending, adding funds, product usage).
* Each transaction is linked to an envelope and records the amount, description, user who initiated it, timestamp, and type (e.g., "spend", "deposit").

### 1.3. Products
* Fixed-price items that map to a specific pre-defined envelope instance.
* Users can define products with a name, unit price (calculated from total price and optional quantity at creation/update), and linked envelope.
* Using a product deducts its price (multiplied by quantity) from the correct user's instance of the linked envelope type (if individual) or the shared envelope.

### 1.4. Shortcuts
* *(Implementation currently deferred. To be revisited if desired.)*
* Original Concept: Command templates with parameters for variable-price items, allowing users to quickly execute pre-filled commands (primarily for spending) by providing a variable amount and an optional note.

### 1.5. Splurge Mechanism (Future Plan)
* Allows a single expense to be split between a primary target envelope and a dedicated "Splurge" envelope.
* The "Splurge" envelope is an individual type (one instance per user, or a shared couple's fund). Its funding is handled manually (e.g., via `/addfunds` or future custom allocation cycles), not via a standard monthly allocation.
* Users will specify the total expense and the amount to be covered by the "Splurge" envelope; the remainder comes from the primary envelope.
* Command: Likely `/spend <primary_envelope> <total_amount> <description> --splurge <amount_for_splurge>` (or a dedicated subcommand).
* Transaction Recording: Ideally a single logical transaction with multiple legs or clearly linked transactions.
* Reporting/Feedback: Mini-report after a splurge spend will show the impact on *both* affected envelopes.
* **Open Questions:** Splurge envelope naming/setup? Individual vs. shared nature of the "Splurge" fund itself? Overdraft handling for either leg?

### 1.6. Recurring Subscriptions (Future Plan)
* Automated logging of recurring expenses (and potentially income).
* Users will define subscriptions (name, amount, envelope, frequency, day of processing).
* A scheduling mechanism (e.g., daily check) within the bot will process due subscriptions.
* If an envelope has insufficient funds, the transaction will still be logged (overdraft), and users will be pinged/notified.
* Successful processing will also trigger a notification.
* Management commands (`/subscription add/list/delete/edit`) will be needed.
* **Open Questions:** Precise handling for "day 31" in short months? How to differentiate recurring income vs. expenses if using a similar system (e.g., via transaction type or signed amounts in definition)?

### 1.7. Savings Goals (Future Plan)
* Users can define a target amount for a specific (likely rollover) envelope.
* Commands: `/goal set <envelope_name> <goal_name> <target_amount> [target_date]`, `/goal remove <goal_name>`, `/goal list`.
* The bot can report progress and potentially notify (e.g., via daily mechanism) when a goal is met.

## 2. Data Model

### 2.1. `envelopes`
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

### 2.2. `transactions`
```sql
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
```

### 2.3. `products`
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

### 2.4. `shortcuts`
*(Schema based on original design; implementation deferred. If pursued with "normal" template approach)*
```sql
CREATE TABLE IF NOT EXISTS shortcuts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    command_template TEXT NOT NULL -- e.g., "spend CoffeeFund {} My usual large latte"
);
```

### 2.5. `system_state`
```sql
CREATE TABLE IF NOT EXISTS system_state (
    key TEXT PRIMARY KEY,
    value TEXT
);
```

### 2.6. `savings_goals` (Future Table)
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
### 2.7. `recurring_transactions` (Future Table for Subscriptions/Income)
```sql
CREATE TABLE IF NOT EXISTS recurring_transactions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    amount REAL NOT NULL,
    transaction_type TEXT NOT NULL, -- 'expense' or 'income'
    envelope_id INTEGER NOT NULL,
    frequency TEXT NOT NULL, -- e.g., "monthly", "yearly", "weekly"
    day_of_month INTEGER,
    month_of_year INTEGER,
    day_of_week INTEGER,
    description TEXT,
    next_due_date DATETIME NOT NULL,
    user_id TEXT NOT NULL,
    FOREIGN KEY (envelope_id) REFERENCES envelopes (id) ON DELETE CASCADE
);
```

## 3. Configuration

* **`.env` file:**
    * `DISCORD_BOT_TOKEN`: The bot's Discord token.
    * `DATABASE_PATH`: Path to the SQLite database file (e.g., `data/envelope_buddy.sqlite`).
    * `COUPLE_USER_ID_1`: Discord User ID of the first user.
    * `COUPLE_USER_ID_2`: Discord User ID of the second user.
    * `USER_NICKNAME_1`: Nickname for User 1.
    * `USER_NICKNAME_2`: Nickname for User 2.
    * `RUST_LOG`: Logging configuration (e.g., `envelope_buddy=debug,info`).
    * `DEV_GUILD_ID` (Optional): For fast registration of slash commands to a specific test server.
    * `LOG_CHANNEL_ID` (Future): Channel ID for persistent logging.
* **`config.toml` file:**
    * Defines the initial set of envelopes (name, category, allocation, is\_individual, rollover). Seeded into the database.

## 4. Command System (Poise Framework with Slash Commands)

Organized into modules (`general`, `envelope`, `transaction`, `product`, `utils`).

### 4.1. Implemented Commands

* **General:**
    * `/ping`: Replies "Pong!"
* **Envelope Management (Parent: `/envelope` - or individual commands if not grouped yet)**
    * `/report`: Displays a full embed report of active envelopes: balance, allocation, actual spent, expected pace, status indicator (🟢🟡🔴⚪).
    * `/create_envelope name:<name> [category:<cat>] [allocation:<amt>] [is_individual:<bool>] [rollover:<bool>]`: Creates/re-enables an envelope. Optional parameters preserve old values on re-enable; balance resets to effective allocation. For new individual types, creates instances for both configured users.
    * `/delete_envelope name:<name>`: Soft-deletes an envelope.
    * `/update`: (Monthly processing) Handles rollovers/resets, prunes old transactions, updates `system_state`.
* **Transaction Posting:**
    * `/spend envelope_name:<env> amount:<amt> [description:<desc>]`: Records a spend. Autocompletes envelope names. Replies with a mini-report embed.
    * `/addfunds envelope_name:<env> amount:<amt> [description:<desc>]`: Adds funds. Records a "deposit" transaction. Autocompletes envelope names. Replies with a mini-report embed.
* **Product Management (Parent: `/product`, Subcommands: `add`, `list`, `update`, `consume`)**
    * `/product add name:<name> total_price:<price> envelope:<env_name> [quantity:<num>] [description:<text>]`: Adds a new product, unit price calculated. `envelope_name` uses autocomplete.
    * `/product list`: Lists all defined products and their details in an embed.
    * `/product update name:<name> total_price:<price> [quantity:<num>]`: Updates product's unit price. `name` uses autocomplete.
    * `/product consume name:<name> [quantity:<num>]`: Records expense using a product. Debits contextual user/shared envelope. `name` uses autocomplete. Replies with mini-report.

### 4.2. Planned Future Commands & Features (Summary)

* **Splurge Spending:** `/spend ... --splurge ...` or `/splurgespend ...` (or `/product splurge ...` if related to a product).
* **Undo Command:** `/undo` (reverts last financial transaction for the couple).
* **Transaction History (Enhanced):** `/history envelope_name:<env> [month:<num>] [year:<num>]`.
* **Transaction Editing:** (Re-evaluate) Potentially `/edit_transaction <id> ...`
* **Recurring Subscriptions/Income:** `/subscription add/list/delete/edit`, `/recurring_income add/...`. Automatic daily processing.
* **Savings Goals:** `/goal set/list/remove`. Automatic check and notification.
* **Fund Transfers:** `/transfer <amount> from:<env> to:<env>`.
* **(Deferred) Shortcuts:** `/shortcut add/list/delete`, `/useshortcut`.

## 5. Message Processing and UX

* **Slash Commands:** Primary interaction.
* **Autocomplete:** For envelope names, product names. Case-insensitive.
* **Mini-Reports:** Embeds showing status of affected envelope after `/spend`, `/addfunds`, `/product consume`.
* **Dual Output (Future):**
    * Database-modifying commands: Ephemeral reply to issuer, persistent log to `LOG_CHANNEL_ID`.
    * Daily full `/report` to `LOG_CHANNEL_ID` via scheduler.

## 6. Key Operations Logic (Implemented & Planned)

* **Monthly Update (`/update`):** Implemented.
* **Spending Progress Tracking (`/report`):** Implemented.
* **Initial Data Seeding:** Implemented.
* **Error Handling:** Basic implemented, ongoing refinement.
* **Scheduling Mechanism (Future):** Needed for daily reports and recurring transactions.

## 7. Implementation Stack

* **Language:** Rust
* **Discord Interaction:** `serenity` crate, `poise` framework.
* **Database:** SQLite via `rusqlite`.
* **Async Runtime:** `tokio`.
* **Logging:** `tracing` with `tracing-subscriber`.
* **Configuration:** `toml`, `dotenvy`.
* **Date/Time:** `chrono`.

## 8. Installation and Setup

* Target: Raspberry Pi.
* Configuration: `.env` and `config.toml`.
* Persistence: `systemd` service.
* Remote Development: SSH, VS Code Remote - SSH.
