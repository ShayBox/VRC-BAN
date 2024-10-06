use color_eyre::{eyre::OptionExt, Report, Result};
use serde_json::Value;
use sqlx::{mysql::MySqlQueryResult, prelude::*, MySqlPool};
use vrchatapi::models::GroupAuditLogEntry;

/// Wrapper around `sqlx::MySqlPool`
pub struct LogsDB(pub MySqlPool);

/// `GroupAuditLogEntry` is not strict
#[derive(Clone, Debug, FromRow)]
pub struct Log {
    pub id: String,
    pub created_at: String,
    pub group_id: String,
    pub actor_id: String,
    pub actor_display_name: Option<String>,
    pub target_id: Option<String>,
    pub event_type: String,
    pub description: String,
    pub data: Value,
}

/// Staff Member Leaderboard Statistics
#[derive(Clone, Debug, FromRow)]
pub struct StaffStats {
    pub username: String,
    pub all_bans: i32,
    pub all_kick: i32,
    pub all_warn: i32,
    pub new_bans: i32,
    pub new_kick: i32,
    pub new_warn: i32,
}

/// Convert between `GroupAuditLogEntry` and `Log`
/// `GroupAuditLogEntry` is not strict enough.
impl TryFrom<GroupAuditLogEntry> for Log {
    type Error = color_eyre::eyre::Error;

    fn try_from(log: GroupAuditLogEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            id: log.id.ok_or_eyre("id")?,
            created_at: log.created_at.ok_or_eyre("created_at")?,
            group_id: log.group_id.ok_or_eyre("group_id")?,
            actor_id: log.actor_id.ok_or_eyre("actor_id")?,
            actor_display_name: log.actor_display_name,
            target_id: log.target_id,
            event_type: log.event_type.ok_or_eyre("event_type")?,
            description: log.description.ok_or_eyre("description")?,
            data: serde_json::to_value(&log.data)?,
        })
    }
}

impl LogsDB {
    /// # Connect to the database
    ///
    /// # Errors
    /// Will return `Err` if `MySqlPool::connect` fails.
    pub async fn connect(url: &str) -> Result<Self> {
        MySqlPool::connect(url)
            .await
            .map(LogsDB)
            .map_err(Report::msg)
    }

    /// # Get the most recent matching action logs (bans and pardons)
    ///
    /// # Errors
    /// Will return `Err` if `sqlx::query_as` fails.
    pub async fn get_recent_actions_by_id(&self, target_id: &str) -> Result<Vec<Log>> {
        sqlx::query_as(
            r"
                SELECT * FROM logs
                WHERE target_id = ? AND (event_type = 'group.user.ban' OR event_type = 'group.user.unban')
                ORDER BY created_at DESC
             ",
        )
        .bind(target_id)
        .fetch_all(&self.0)
        .await
        .map_err(Report::msg)
    }

    /// # Get all the action logs sorted by most recent.
    ///
    /// # Errors
    /// Will return `Err` if `sqlx::query_as` fails.
    pub async fn get_all_recent_actions(&self) -> Result<Vec<Log>> {
        sqlx::query_as(
            r"
                SELECT * FROM logs
                WHERE event_type IN ('group.user.ban','group.user.unban')
                ORDER BY created_at DESC
             ",
        )
        .fetch_all(&self.0)
        .await
        .map_err(Report::msg)
    }

    /// # Insert a log into the database
    ///
    /// # Errors
    /// Will return `Err` if `sqlx::query` fails.
    pub async fn insert_log(&self, log: Log) -> Result<MySqlQueryResult> {
        sqlx::query(
            r"INSERT INTO logs (
                id,
                created_at,
                group_id,
                actor_id,
                actor_display_name,
                target_id,
                event_type,
                description,
                data
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(log.id)
        .bind(log.created_at)
        .bind(log.group_id)
        .bind(log.actor_id)
        .bind(log.actor_display_name)
        .bind(log.target_id)
        .bind(log.event_type)
        .bind(log.description)
        .bind(log.data)
        .execute(&self.0)
        .await
        .map_err(Report::msg)
    }
}
