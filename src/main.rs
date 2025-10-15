use dotenvy::dotenv;
use envelope_buddy::{bot, config, core::envelope, errors::Error};
use sea_orm::{Database, DatabaseConnection};
use std::env;
use tracing::{error, info, warn};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Load environment variables from .env file
    dotenv().ok();

    // Initialize tracing/logging
    init_tracing()?;
    info!("EnvelopeBuddy v0.2.0 starting...");

    // Load database configuration
    let db_url = config::database::get_database_url()?;
    info!("Connecting to database...");

    // Connect to database
    let db = Database::connect(&db_url)
        .await
        .map_err(|e| Error::Database(Box::new(e)))?;

    info!("Database connected successfully");

    // Create tables if they don't exist
    if let Err(e) = config::database::create_tables(&db).await {
        error!("Failed to create database tables: {}", e);
        return Err(e);
    }

    // Seed initial envelopes from config.toml
    seed_envelopes(&db).await?;

    // Get Discord bot token
    let token = env::var("DISCORD_BOT_TOKEN").map_err(|_| Error::Config {
        message: "DISCORD_BOT_TOKEN environment variable not set".to_string(),
    })?;

    info!("Starting Discord bot...");
    run_bot(token, db).await?;

    Ok(())
}

/// Initializes the tracing subscriber for logging
fn init_tracing() -> Result<(), Error> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(std::io::stdout))
        .init();

    Ok(())
}

/// Runs the Discord bot with the given token and database connection
async fn run_bot(token: String, db: DatabaseConnection) -> Result<(), Error> {
    use poise::serenity_prelude as serenity;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                // General commands
                bot::ping(),
                bot::help(),
                // Transaction commands
                bot::spend(),
                bot::addfunds(),
                // Envelope commands
                bot::report(),
                bot::update(),
                bot::create_envelope(),
                bot::delete_envelope(),
                bot::envelopes(),
                bot::envelope_info(),
                bot::update_envelope(),
                // Product commands
                bot::product_manage(),
                bot::use_product(),
            ],
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            Box::pin(async move {
                info!("Logged in as {}", ready.user.name);

                // Register commands
                if let Ok(guild_id_str) = std::env::var("DEV_GUILD_ID") {
                    if let Ok(guild_id_val) = guild_id_str.parse::<u64>() {
                        let guild_id = serenity::GuildId::new(guild_id_val);
                        info!("Registering commands in development guild: {}", guild_id);

                        poise::builtins::register_in_guild(
                            ctx,
                            &framework.options().commands,
                            guild_id,
                        )
                        .await?;

                        info!("Commands registered in guild {}", guild_id);
                    }
                } else {
                    info!("Registering commands globally...");
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    info!("Commands registered globally");
                }

                Ok(bot::BotData::new(db))
            })
        })
        .build();

    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    info!("Creating Discord client...");
    let mut client = serenity::Client::builder(&token, intents)
        .framework(framework)
        .await
        .map_err(|e| Error::Config {
            message: format!("Failed to create Discord client: {}", e),
        })?;

    info!("Starting bot client...");
    client.start().await.map_err(|e| Error::Config {
        message: format!("Bot client error: {}", e),
    })?;

    Ok(())
}

/// Error handler for poise framework
async fn on_error(error: poise::FrameworkError<'_, bot::BotData, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            panic!("Failed to start bot: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            error!("Error in command `{}`: {:?}", ctx.command().name, error);
            if let Err(e) = ctx.say(format!("❌ An error occurred: {}", error)).await {
                error!("Failed to send error message: {}", e);
            }
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                error!("Error while handling error: {}", e);
            }
        }
    }
}

/// Seeds initial envelopes from config.toml if they don't already exist
///
/// This function loads envelope definitions from config.toml and creates them
/// in the database if they don't already exist (checked by name). Individual
/// envelopes are created as templates (no user_id) and will be instantiated
/// per-user when first accessed.
async fn seed_envelopes(db: &DatabaseConnection) -> Result<(), Error> {
    // Load config file
    let config = match config::envelopes::load_default_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            warn!(
                "Could not load config.toml, skipping envelope seeding: {}",
                e
            );
            return Ok(());
        }
    };

    info!(
        "Seeding {} envelopes from config.toml...",
        config.envelopes.len()
    );

    for env_config in &config.envelopes {
        // Check if envelope already exists
        let existing = envelope::get_envelope_by_name(db, &env_config.name).await?;

        if existing.is_some() {
            info!("Envelope '{}' already exists, skipping", env_config.name);
            continue;
        }

        // Create the envelope
        // Individual envelopes are created as templates (no user_id)
        match envelope::create_envelope(
            db,
            env_config.name.clone(),
            None, // No user_id for template envelopes
            env_config.category.clone(),
            env_config.allocation,
            env_config.is_individual,
            env_config.rollover,
        )
        .await
        {
            Ok(_) => {
                info!(
                    "✓ Created envelope '{}' ({}, ${:.2})",
                    env_config.name, env_config.category, env_config.allocation
                );
            }
            Err(e) => {
                error!("Failed to create envelope '{}': {}", env_config.name, e);
                return Err(e);
            }
        }
    }

    info!("Envelope seeding complete");
    Ok(())
}
