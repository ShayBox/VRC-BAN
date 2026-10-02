use std::time::Duration;

#[cfg(feature = "discord")]
use color_eyre::{eyre::Error, Result};
use config::Config;
use logsdb::{Log, LogsDB};
#[cfg(feature = "discord")]
use poise::{serenity_prelude::*, Framework, FrameworkContext};
use sqlx::{error::ErrorKind, Error as SqlxError};
use vrchat::{VRChat, MAX};

#[cfg(feature = "discord")]
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
    #[cfg(feature = "discord")]
    pub async fn setup(
        self,
        ctx: &Context,
        _ready: &Ready,
        framework: &Framework<Self, Error>,
    ) -> Result<Self> {
        #[cfg(debug_assertions)]
        let guild_id = GuildId::new(1_555_654_113_201_623_111);
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
    #[cfg(feature = "discord")]
    pub async fn event_handler(
        &self,
        _ctx: &Context,
        event: &FullEvent,
        _framework: FrameworkContext<'_, Self, Error>,
    ) -> Result<()> {
        let FullEvent::Ready { data_about_bot: _ } = event else {
            return Ok(());
        };

        self.sync_logs().await;

        Ok(())
    }

    /// # Sync the group audit logs into the database
    ///
    /// First backfills the full history of audit logs, stopping early once it
    /// reaches entries that are already stored in `LogsDB`, then loops forever,
    /// fetching the most recent audit logs from `VRChat` and inserting them.
    pub async fn sync_logs(&self) {
        let Self {
            config,
            logsdb,
            vrchat,
        } = self;

        /* Backfill the full history of audit logs */
        let mut offset = 0i32;
        loop {
            let Ok(logs) = vrchat
                .get_group_audit_logs(&config.vrc_group_id, MAX, offset)
                .await
            else {
                tokio::time::sleep(Duration::from_secs(600)).await;
                continue; // Retry the same page
            };

            let mut new_logs = 0usize;
            for log in logs {
                if insert_log_quiet(logsdb, log).await {
                    new_logs += 1;
                }
            }

            if new_logs == 0 {
                break; // Reached already-synced history (or the end)
            }
            offset += MAX;
        }

        /* Keep polling for new audit logs */
        loop {
            let Ok(logs) = vrchat
                .get_group_audit_logs(&config.vrc_group_id, MAX, 0)
                .await
            else {
                tokio::time::sleep(Duration::from_secs(600)).await;
                continue;
            };

            for log in logs {
                insert_log_quiet(logsdb, log).await;
            }

            tokio::time::sleep(Duration::from_secs(600)).await;
        }
    }
}

/// Insert a log into `LogsDB`, skipping entries that are already present.
///
/// Returns `true` if the log was not previously stored, or `false` if it was
/// skipped because it is already present.
async fn insert_log_quiet(logsdb: &LogsDB, log: Log) -> bool {
    match logsdb.insert_log(log).await {
        Ok(_) => true,
        Err(error) => {
            let duplicate = error.chain().any(|source| {
                source
                    .downcast_ref::<SqlxError>()
                    .and_then(SqlxError::as_database_error)
                    .is_some_and(|db_err| db_err.kind() == ErrorKind::UniqueViolation)
            });

            if !duplicate {
                eprintln!("Error: {error}");
            }

            !duplicate
        }
    }
}
