use anyhow::Context;
use serde_json::Value;
use sqlx::{mysql::MySqlQueryResult, prelude::*, MySqlPool, Result};
use vrchatapi::models::GroupAuditLogEntry;

/// Wrapper around `MySqlPool`
pub struct LogsDB(pub MySqlPool);

#[derive(Debug, FromRow)]
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

#[derive(Debug, FromRow)]
pub struct StaffStats {
    pub username: String,
    pub all_bans: i32,
    pub all_kick: i32,
    pub all_warn: i32,
    pub new_bans: i32,
    pub new_kick: i32,
    pub new_warn: i32,
}

/// Convert between `GroupAuditLogEntry` and Log
impl TryFrom<GroupAuditLogEntry> for Log {
    type Error = anyhow::Error;

    fn try_from(log: GroupAuditLogEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            id: log.id.context("id")?,
            created_at: log.created_at.context("created_at")?,
            group_id: log.group_id.context("group_id")?,
            actor_id: log.actor_id.context("actor_id")?,
            actor_display_name: log.actor_display_name,
            target_id: log.target_id,
            event_type: log.event_type.context("event_type")?,
            description: log.description.context("description")?,
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
        MySqlPool::connect(url).await.map(LogsDB)
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
    }

    /// # Get recent logs of a certain type
    ///
    /// # Errors
    /// Will return `Err` if `sqlx::query_as` fails.
    pub async fn get_recent_logs(&self, event_type: &str, limit: i32) -> Result<Vec<Log>> {
        sqlx::query_as(
            r"
                SELECT * FROM logs
                WHERE event_type = ?
                ORDER BY created_at DESC
                LIMIT ?
            ",
        )
        .bind(event_type)
        .bind(limit)
        .fetch_all(&self.0)
        .await
    }

    /// # Find recent logs of a certain type and target
    ///
    /// # Errors
    /// Will return `Err` if `sqlx::query_as` fails.
    pub async fn find_recent_logs(
        &self,
        event_type: &str,
        target_id: &str,
        limit: i32,
    ) -> Result<Vec<Log>> {
        sqlx::query_as(
            r"
                SELECT * FROM logs
                WHERE event_type = ? AND target_id = ?
                ORDER BY created_at DESC
                LIMIT ?
             ",
        )
        .bind(event_type)
        .bind(target_id)
        .bind(limit)
        .fetch_all(&self.0)
        .await
    }

    /// # Get the staff statistics
    ///
    /// # Errors
    /// Will return `Err` if `sqlx::query_as` fails.
    pub async fn get_staff_stats(&self) -> Result<Vec<StaffStats>> {
        sqlx::query_as(
            r"
                WITH LatestNames AS (
                    SELECT id, actor_display_name
                    FROM logs
                    WHERE actor_display_name IS NOT NULL
                    AND event_type IN ('group.user.ban', 'group.instance.kick', 'group.instance.warn')
                    AND created_at = (
                        SELECT MAX(created_at)
                        FROM logs l2
                        WHERE l2.id = logs.id
                        AND l2.actor_display_name IS NOT NULL
                    )
                )
                SELECT DISTINCT ln.actor_display_name AS username,    
                    COUNT(CASE WHEN l.event_type = 'group.user.ban' THEN 1 END) AS all_bans,
                    COUNT(CASE WHEN l.event_type = 'group.instance.kick' THEN 1 END) AS all_kick,
                    COUNT(CASE WHEN l.event_type = 'group.instance.warn' THEN 1 END) AS all_warn,
                    COUNT(CASE WHEN l.event_type = 'group.user.ban' AND l.created_at > NOW() - INTERVAL 1 DAY THEN 1 END) AS new_bans,
                    COUNT(CASE WHEN l.event_type = 'group.instance.kick' AND l.created_at > NOW() - INTERVAL 1 DAY THEN 1 END) AS new_kick,
                    COUNT(CASE WHEN l.event_type = 'group.instance.warn' AND l.created_at > NOW() - INTERVAL 1 DAY THEN 1 END) AS new_warn
                FROM logs l
                JOIN LatestNames ln ON l.id = ln.id
                GROUP BY ln.actor_display_name
            ",
        )
        .fetch_all(&self.0)
        .await
    }
}
