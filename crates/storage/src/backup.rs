use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    list_all_memory_items, list_session_events, list_session_messages, list_task_steps,
    list_tasks, MemoryItemRow, StorageError, TaskRow,
};

const BACKUP_FORMAT: &str = "evohime-backup";
const BACKUP_VERSION: u32 = 1;
const MAX_MEMORY_ITEMS: i64 = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDump {
    pub format: String,
    pub version: u32,
    pub exported_at: DateTime<Utc>,
    pub sessions: Vec<BackupSession>,
    pub memory_items: Vec<MemoryItemRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSession {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub archived: bool,
    pub title: Option<String>,
    pub workspace_path: Option<String>,
    pub messages: Vec<BackupMessage>,
    pub tasks: Vec<BackupTask>,
    pub events: Vec<BackupEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTask {
    #[serde(flatten)]
    pub task: TaskRow,
    pub steps: Vec<crate::TaskStepRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMessage {
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEvent {
    pub sequence: i64,
    pub created_at: DateTime<Utc>,
    pub event_json: serde_json::Value,
}

#[derive(Debug, Clone, FromRow)]
struct BackupSessionRow {
    id: Uuid,
    created_at: DateTime<Utc>,
    archived: bool,
    title: Option<String>,
    workspace_path: Option<String>,
}

impl BackupDump {
    pub fn empty() -> Self {
        Self {
            format: BACKUP_FORMAT.into(),
            version: BACKUP_VERSION,
            exported_at: Utc::now(),
            sessions: Vec::new(),
            memory_items: Vec::new(),
        }
    }
}

pub async fn collect_backup(pool: &PgPool) -> Result<BackupDump, StorageError> {
    let session_rows = sqlx::query_as::<_, BackupSessionRow>(
        r#"
        SELECT id, created_at, archived_at IS NOT NULL AS archived, title, workspace_path
        FROM sessions
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut sessions = Vec::with_capacity(session_rows.len());
    for session in session_rows {
        let messages = list_session_messages(pool, session.id)
            .await?
            .into_iter()
            .map(|message| BackupMessage {
                role: message.role,
                content: message.content,
                created_at: message.created_at,
            })
            .collect();
        let events = list_session_events(pool, session.id)
            .await?
            .into_iter()
            .map(|event| BackupEvent {
                sequence: event.sequence,
                created_at: event.created_at,
                event_json: event.event_json,
            })
            .collect();
        let mut tasks = Vec::new();
        for task in list_tasks(pool, Some(session.id)).await? {
            let steps = list_task_steps(pool, task.id).await?;
            tasks.push(BackupTask { task, steps });
        }

        sessions.push(BackupSession {
            id: session.id,
            created_at: session.created_at,
            archived: session.archived,
            title: session.title,
            workspace_path: session.workspace_path,
            messages,
            tasks,
            events,
        });
    }

    Ok(BackupDump {
        sessions,
        memory_items: list_all_memory_items(pool, MAX_MEMORY_ITEMS).await?,
        ..BackupDump::empty()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_backup_has_stable_format_and_collections() {
        let dump = BackupDump::empty();

        assert_eq!(dump.format, "evohime-backup");
        assert_eq!(dump.version, 1);
        assert!(dump.sessions.is_empty());
        assert!(dump.memory_items.is_empty());

        let json = serde_json::to_value(dump).unwrap();
        assert_eq!(json["format"], "evohime-backup");
        assert!(json["sessions"].is_array());
        assert!(json["memory_items"].is_array());
    }
}
