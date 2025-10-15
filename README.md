# EnvelopeBuddy

A Discord bot for envelope-style budgeting, designed for couples to track shared and individual expenses. Built in Rust for performance and reliability.

## Features

- **Envelope System**: Shared and individual envelopes with monthly allocations
- **Rollover Support**: Choose between resetting monthly or rolling over unused balances
- **Quick Logging**: Pre-defined products for instant expense tracking
- **Rich Reporting**: Visual progress indicators and spending analysis
- **Autocomplete**: Smart suggestions for envelope and product names
- **Monthly Updates**: Automated rollover/reset handling

## Quick Start

### Prerequisites

- Rust 1.82+ (for 2024 edition support)
- Discord bot token from [Discord Developer Portal](https://discord.com/developers/applications)

### Installation

1. **Clone and build**:
   ```bash
   git clone <repository-url>
   cd envelope-buddy
   cargo build --release
   ```

2. **Configure**:
   ```bash
   cp .env.example .env
   nano .env  # Add your DISCORD_BOT_TOKEN
   ```

3. **Set up initial envelopes** (optional):
   Edit `config.toml` to define starting envelopes. Example:
   ```toml
   [[envelopes]]
   name = "groceries"
   category = "necessary"
   allocation = 500.0
   is_individual = false
   rollover = false
   ```

   **Important**: `config.toml` is only used for **initial database seeding**. Once an envelope exists in the database, it won't be re-created or updated from the config file. After first run, the database is your source of truth. Use Discord commands (`/create_envelope`, `/update_envelope`) to manage envelopes.

4. **Run**:
   ```bash
   ./target/release/envelope-buddy
   ```

### Configuration

**Required** (in `.env`):
- `DISCORD_BOT_TOKEN` - Your Discord bot token

**Optional** (in `.env`):
- `DEV_GUILD_ID` - Guild ID for fast command registration during development
- `DATABASE_URL` - Database path (default: `sqlite://data/envelope_buddy.sqlite`)
- `RUST_LOG` - Logging level (default: `info`)

**Important Note**: The `config.toml` file is only read during initial database creation. After first run, all envelope data lives in the SQLite database. To modify envelopes after initial setup, use Discord commands, not the config file.

## Core Concepts

### Envelopes

Logical containers for budgeting specific categories:
- **Shared** or **Individual** per user
- Monthly `allocation` amount
- Current `balance`
- **Rollover**: Unused balance carries to next month, or resets to allocation
- **Soft Delete**: Can be deleted and re-enabled later

### Transactions

Track all financial activities:
- Linked to a specific envelope
- Records amount, description, user, timestamp, and type

### Products

Fixed-price items for quick expense logging:
- Define once with name, price, and linked envelope
- Use instantly with `/use_product` command
- Automatically deducts from correct user's envelope (for individual envelopes)

## Commands

### General
- `/ping` - Health check

### Envelope Management
- `/report` - View all envelopes with balances and progress
- `/create_envelope` - Create or re-enable an envelope
- `/update_envelope` - Modify allocation or settings
- `/delete_envelope` - Soft-delete an envelope
- `/envelopes` - List all active envelopes
- `/envelope_info` - Detailed info for a specific envelope
- `/update` - Process monthly rollover/reset (manual trigger)

### Transactions
- `/spend` - Record an expense
- `/addfunds` - Add money to an envelope

### Products
- `/product add` - Define a new product
- `/product list` - View all products
- `/product update` - Change product price
- `/product delete` - Remove a product
- `/use_product` - Log an expense using a pre-defined product

## Data Model

### Database Tables

**envelopes**
- `id`, `name`, `category`, `allocation`, `balance`
- `is_individual`, `user_id`, `rollover`, `is_deleted`

**transactions**
- `id`, `envelope_id`, `amount`, `description`
- `timestamp`, `user_id`, `message_id`, `transaction_type`

**products**
- `id`, `name`, `price`, `envelope_id`, `description`, `is_deleted`

**system_state**
- `key`, `value`, `updated_at` (tracks monthly updates)

## Tech Stack

- **Language**: Rust (2024 edition)
- **Discord**: `serenity` + `poise` framework
- **Database**: SQLite via `sea-orm` ORM
- **Async**: `tokio` runtime
- **Logging**: `tracing` with `tracing-subscriber`

## Deployment (Raspberry Pi)

### Prerequisites

```bash
# Install required tools
cargo install cross cargo-make
```

**SSH Configuration**: Set up an SSH alias `rp` in `~/.ssh/config`:
```
Host rp
    HostName <your-pi-ip-or-hostname>
    User pi
```

### Deploy Commands

All deployment is handled via `cargo-make`:

```bash
# Full deployment (build, stop, copy, start)
cargo make deploy

# Just build for Pi
cargo make pi

# View live logs
cargo make logs

# Check service status
cargo make status

# Restart without rebuilding
cargo make restart
```

**Environment variables** (optional):
```bash
# Override defaults
PI_HOST=mypi cargo make deploy
PI_DIR=/opt/envelope-buddy cargo make deploy
```

### First-Time Pi Setup

1. **Create directory and .env on Pi**:
   ```bash
   ssh rp "mkdir -p ~/envelope-buddy"
   ssh rp "nano ~/envelope-buddy/.env"
   # Add: DISCORD_BOT_TOKEN=your_token_here
   ```

2. **Copy config.toml** (optional, for initial envelope seeding):
   ```bash
   scp config.toml rp:~/envelope-buddy/
   ```

3. **Create systemd service** (`/etc/systemd/system/envelope-buddy.service`):
   ```ini
   [Unit]
   Description=EnvelopeBuddy Discord Bot
   After=network.target

   [Service]
   Type=simple
   User=pi
   WorkingDirectory=/home/pi/envelope-buddy
   ExecStart=/home/pi/envelope-buddy/envelope-buddy
   Restart=always
   RestartSec=10

   [Install]
   WantedBy=multi-user.target
   ```

4. **Enable service**:
   ```bash
   ssh rp "sudo systemctl enable envelope_buddy.service"
   ```

5. **Deploy**:
   ```bash
   cargo make deploy
   ```

The deploy command will build, copy, and start the service automatically!

## Development

### Project Structure

```
src/
├── main.rs              # Entry point
├── bot/                 # Discord interface layer
│   ├── commands/        # Slash command handlers
│   └── handlers/        # Autocomplete handlers
├── core/                # Business logic
│   ├── envelope.rs
│   ├── transaction.rs
│   ├── product.rs
│   ├── monthly.rs
│   └── report.rs
├── entities/            # SeaORM entity definitions
├── config/              # Configuration handling
└── errors.rs            # Error types
```

### Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_create_envelope
```

### Code Quality

```bash
# Check for issues
cargo clippy

# Format code
cargo fmt

# Build release
cargo build --release
```

## Future Plans

- **Splurge Mechanism**: Split expenses between primary and "splurge" envelope
- **Recurring Transactions**: Automated subscription/income tracking
- **Savings Goals**: Set and track envelope targets
- **Transaction History**: View historical spending by month
- **Fund Transfers**: Move money between envelopes

## License

Apache License 2.0 - See [LICENSE](LICENSE) file for details.

## Architecture Notes

This project follows a clean architecture:
- **Bot Layer**: Discord-specific code (commands, embeds, autocomplete)
- **Core Layer**: Framework-agnostic business logic
- **Database Layer**: SeaORM entities and migrations

The V2 rewrite (current version) replaced complex raw SQL with SeaORM for better maintainability and type safety. All 70+ core business logic tests ensure reliability.
