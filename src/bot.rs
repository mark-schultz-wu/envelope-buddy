use crate::config::AppConfig;
use crate::db::DbPool;
use crate::errors;
use crate::{commands, models::CachedEnvelopeInfo};
use poise::serenity_prelude as serenity;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, instrument, warn};

// User data, which is stored and accessible in all command invocations
#[derive(Debug)]
#[allow(dead_code)]
pub struct Data {
    pub app_config: Arc<AppConfig>,
    pub db_pool: DbPool,
    pub envelope_names_cache: Arc<RwLock<Vec<CachedEnvelopeInfo>>>,
    pub product_names_cache: Arc<RwLock<Vec<String>>>,
}

// Type alias for the error type Poise will use
pub(crate) type Error = errors::Error;
pub(crate) type Context<'a> = poise::Context<'a, Data, Error>;

async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    match error {
        poise::FrameworkError::Setup { error, .. } => {
            panic!("Failed to start bot: {:?}", error);
        }
        poise::FrameworkError::Command { error, ctx, .. } => {
            tracing::error!("Error in command `{}`: {:?}", ctx.command().name, error);
            if let Err(e) = ctx.say(format!("An error occurred: {}", error)).await {
                tracing::error!("Failed to send error message: {}", e);
            }
        }
        error => {
            if let Err(e) = poise::builtins::on_error(error).await {
                tracing::error!("Error while handling error: {}", e)
            }
        }
    }
}

#[instrument(skip(token, config, db_pool))]
pub async fn run_bot(
    token: String,
    config: Arc<AppConfig>,
    db_pool: DbPool,
) -> Result<(), serenity::Error> {
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping(),
                commands::report(),
                commands::spend(),
                commands::addfunds(),
                commands::update(),
                commands::product_use(),
                commands::manage(),
            ],
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            let config_clone_for_data = Arc::clone(&config); // Use the passed-in config
            let db_pool_clone_for_data = db_pool.clone();
            Box::pin(async move {
                info!("Logged in as {}", ready.user.name);
                // Log the names of the commands poise is aware of before registration
                let command_names_to_register: Vec<String> = framework
                    .options()
                    .commands
                    .iter()
                    .map(|cmd| cmd.name.clone()) // Or cmd.qualified_name for more detail
                    .collect();
                info!(
                    "Poise is configured to register the following commands: {:?}",
                    command_names_to_register
                );
                info!("Attempting to clear all global commands...");
                let empty_commands_slice: &[poise::Command<Data, crate::errors::Error>] = &[];
                match poise::builtins::register_globally(ctx, empty_commands_slice).await {
                    // Pass an empty slice
                    Ok(_) => info!("Successfully cleared all global commands."),
                    Err(e) => error!("Failed to clear global commands: {:?}", e),
                }
                if let Ok(guild_id_str) = std::env::var("DEV_GUILD_ID") {
                    if let Ok(guild_id_val) = guild_id_str.parse::<u64>() {
                        let guild_id = serenity::GuildId::new(guild_id_val);
                        info!(
                            "Attempting to register commands in development guild: {}",
                            guild_id
                        );
                        match poise::builtins::register_in_guild(
                            ctx,
                            &framework.options().commands,
                            guild_id,
                        )
                        .await
                        {
                            Ok(_) => info!(
                                "Successfully submitted command registration to guild {}.",
                                guild_id
                            ),
                            Err(e) => {
                                error!("Failed to register commands in guild {}: {:?}", guild_id, e)
                            }
                        }
                    } else {
                        warn!(
                            "DEV_GUILD_ID is set but is not a valid u64: {}",
                            guild_id_str
                        );
                        // Fallback to global if guild registration is intended only for dev with valid ID
                        // Or handle as an error if guild registration is critical for dev
                    }
                } else {
                    info!("DEV_GUILD_ID not set. Registering commands globally...");
                    // Fallback to global registration if no dev guild ID is set
                    match poise::builtins::register_globally(ctx, &framework.options().commands)
                        .await
                    {
                        Ok(_) => {
                            info!("Successfully submitted global command registration to Discord.")
                        }
                        Err(e) => error!("Failed to register commands globally: {:?}", e),
                    }
                }
                // --- INITIALIZE AND POPULATE CACHES ---
                let data_instance = Data {
                    app_config: config_clone_for_data,
                    db_pool: db_pool_clone_for_data,
                    envelope_names_cache: Arc::new(RwLock::new(Vec::new())),
                    product_names_cache: Arc::new(RwLock::new(Vec::new())),
                };

                // Populate initial caches (calls functions from crate::cache)
                if let Err(e) = crate::cache::refresh_envelope_names_cache(
                    &data_instance.db_pool,
                    &data_instance.envelope_names_cache,
                )
                .await
                {
                    error!("Failed to initialize envelope names cache: {}", e);
                }
                if let Err(e) = crate::cache::refresh_product_names_cache(
                    &data_instance.db_pool,
                    &data_instance.product_names_cache,
                )
                .await
                {
                    error!("Failed to initialize product names cache: {}", e);
                }
                Ok(data_instance)
            })
        })
        .build();

    // Define necessary gateway intents
    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT; // If you plan to use prefix commands too

    info!("Setting up Serenity client for Poise framework...");
    let client = serenity::Client::builder(&token, intents)
        .framework(framework)
        .await;

    match client {
        Ok(mut c) => {
            info!("Starting bot client...");
            if let Err(why) = c.start().await {
                tracing::error!("Client error: {:?}", why);
                return Err(why);
            }
        }
        Err(e) => {
            tracing::error!("Error creating client: {:?}", e);
            return Err(e);
        }
    }
    Ok(())
}
