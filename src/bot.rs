use crate::config::AppConfig;
use crate::db::DbPool;
use crate::{commands, errors};
use poise::serenity_prelude as serenity;
use std::env;
use std::sync::Arc;
use tracing::{info, instrument};

// User data, which is stored and accessible in all command invocations
#[derive(Debug)]
#[allow(dead_code)]
pub struct Data {
    pub app_config: Arc<AppConfig>,
    pub db_pool: DbPool,
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

#[instrument(skip(initial_app_config, db_pool))]
pub async fn run_bot(
    initial_app_config: AppConfig,
    db_pool: DbPool,
) -> Result<(), serenity::Error> {
    let token =
        env::var("DISCORD_BOT_TOKEN").expect("Expected a DISCORD_BOT_TOKEN in the environment");
    let app_config = Arc::new(initial_app_config);

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ping(),
                // Add more commands here
            ],
            on_error: |error| Box::pin(on_error(error)),
            ..Default::default()
        })
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                info!("Logged in as {}", ready.user.name);
                info!("Registering commands globally...");
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                // Example for guild-specific registration (faster updates during dev):
                // if let Ok(guild_id_str) = env::var("DEV_GUILD_ID") {
                //     if let Ok(guild_id_val) = guild_id_str.parse::<u64>() {
                //         let guild_id = serenity::GuildId::new(guild_id_val);
                //         poise::builtins::register_in_guild(ctx, &framework.options().commands, guild_id).await?;
                //         info!("Registered commands in guild {}", guild_id);
                //     }
                // }
                Ok(Data {
                    app_config: Arc::clone(&app_config),
                    db_pool,
                })
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
