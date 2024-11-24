use std::time::Duration;

use color_eyre::{eyre::Error, Result};
use config::Config;
use logsdb::LogsDB;
use poise::{serenity_prelude::*, Framework, FrameworkContext};
use vrchat::VRChat;

pub mod commands;
pub mod config;
pub mod logsdb;
pub mod vrchat;

pub struct Data {
    pub config: config::Config,
    pub logsdb: logsdb::LogsDB,
    pub vrchat: vrchat::VRChat,
}

impl Data {
    #[must_use]
    pub const fn new(config: Config, logsdb: LogsDB, vrchat: VRChat) -> Self {
        Self {
            config,
            logsdb,
            vrchat,
        }
    }

    /// # Setup the data for the bot
    ///
    /// # Errors
    /// Will return `Err` if `register_in_guild` or `register_globally` fails.
    pub async fn setup(
        self,
        ctx: &Context,
        _ready: &Ready,
        framework: &Framework<Self, Error>,
    ) -> Result<Self> {
        #[cfg(debug_assertions)]
        let guild_id = GuildId::new(824_865_729_445_888_041);
        let commands = &framework.options().commands;

        #[cfg(debug_assertions)]
        poise::builtins::register_in_guild(ctx, commands, guild_id).await?;

        #[cfg(not(debug_assertions))]
        poise::builtins::register_globally(ctx, commands).await?;

        Ok(self)
    }

    /// # Handle Events
    ///
    /// # Errors
    /// Will not return `Err`
    pub async fn event_handler(
        &self,
        _ctx: &Context,
        event: &FullEvent,
        _framework: FrameworkContext<'_, Self, Error>,
    ) -> Result<()> {
        let Self {
            config,
            logsdb,
            vrchat,
        } = self;

        let FullEvent::Ready { data_about_bot: _ } = event else {
            return Ok(());
        };

        loop {
            let Ok(logs) = vrchat
                .get_group_audit_logs(&config.vrc_group_id, 100, 0)
                .await
            else {
                continue;
            };

            for log in logs {
                if let Err(error) = logsdb.insert_log(log).await {
                    eprintln!("Error: {error}");
                    break;
                };
            }

            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    }
}
