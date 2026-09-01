use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_PAGE_EVENTS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredConversationEvent {
    pub conversation_id: String,
    pub event_id: String,
    pub sequence: u64,
    pub timestamp_ms: i64,
    pub kind: String,
    pub category: String,
    pub authoritative_payload: Vec<u8>,
    pub renderer_payload: Vec<u8>,
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub turn_id: Option<String>,
    pub client_message_id: Option<String>,
    pub persistence_class: String,
    pub sensitivity: String,
    pub schema_version: u32,
}

#[derive(Debug, Clone)]
pub struct NewConversationEvent<'a> {
    pub conversation_id: &'a str,
    pub workspace_id: &'a str,
    pub kind: &'a str,
    pub category: &'a str,
    pub authoritative_payload: &'a [u8],
    pub renderer_payload: &'a [u8],
    pub correlation_id: Option<&'a str>,
    pub causation_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub run_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub client_message_id: Option<&'a str>,
    pub persistence_class: &'a str,
    pub sensitivity: &'a str,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageAcceptance {
    pub event: StoredConversationEvent,
    pub task_id: String,
    pub deduplicated: bool,
    pub dispatch_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEventPage {
    pub events: Vec<StoredConversationEvent>,
    pub oldest_sequence: Option<u64>,
    pub newest_sequence: Option<u64>,
    pub has_older: bool,
    pub has_newer: bool,
    pub earliest_available_sequence: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConversationStoreError {
    #[error("conversation input is invalid")]
    InvalidInput,
    #[error("client message id was reused with different content")]
    IdempotencyConflict,
    #[error("history cursor is no longer retained")]
    CursorExpired { earliest_available_sequence: u64 },
    #[error("conversation was not found")]
    ConversationNotFound,
    #[error("conversation storage failed: {0}")]
    Sql(String),
}

impl From<rusqlite::Error> for ConversationStoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value.to_string())
    }
}

pub fn install_schema(connection: &Connection) -> Result<(), ConversationStoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS conversation_log_metadata (
            conversation_id TEXT PRIMARY KEY NOT NULL,
            workspace_id TEXT NOT NULL,
            next_sequence INTEGER NOT NULL DEFAULT 0,
            oldest_available_sequence INTEGER NOT NULL DEFAULT 1,
            schema_version INTEGER NOT NULL,
            created_at_ms INTEGER NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS conversation_log_events (
            conversation_id TEXT NOT NULL REFERENCES conversation_log_metadata(conversation_id) ON DELETE CASCADE,
            sequence INTEGER NOT NULL,
            event_id TEXT NOT NULL UNIQUE,
            timestamp_ms INTEGER NOT NULL,
            kind TEXT NOT NULL,
            category TEXT NOT NULL,
            authoritative_payload BLOB NOT NULL,
            renderer_payload BLOB NOT NULL,
            correlation_id TEXT,
            causation_id TEXT,
            task_id TEXT,
            run_id TEXT,
            turn_id TEXT,
            client_message_id TEXT,
            persistence_class TEXT NOT NULL,
            sensitivity TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            PRIMARY KEY(conversation_id, sequence)
        );
        CREATE INDEX IF NOT EXISTS idx_conversation_log_events_kind
            ON conversation_log_events(conversation_id, kind, sequence);
        CREATE TABLE IF NOT EXISTS conversation_log_client_messages (
            conversation_id TEXT NOT NULL REFERENCES conversation_log_metadata(conversation_id) ON DELETE CASCADE,
            client_message_id TEXT NOT NULL,
            task_id TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            snapshot_payload BLOB NOT NULL DEFAULT X'',
            event_sequence INTEGER NOT NULL,
            accepted_at_ms INTEGER NOT NULL,
            dispatch_state TEXT NOT NULL DEFAULT 'accepted',
            PRIMARY KEY(conversation_id, client_message_id),
            FOREIGN KEY(conversation_id, event_sequence)
                REFERENCES conversation_log_events(conversation_id, sequence) ON DELETE CASCADE
        );
        CREATE TABLE IF NOT EXISTS conversation_log_task_bindings (
            task_id TEXT PRIMARY KEY NOT NULL,
            conversation_id TEXT NOT NULL REFERENCES conversation_log_metadata(conversation_id) ON DELETE CASCADE,
            client_message_id TEXT NOT NULL,
            bound_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS conversation_log_compacted_ranges (
            conversation_id TEXT NOT NULL REFERENCES conversation_log_metadata(conversation_id) ON DELETE CASCADE,
            first_sequence INTEGER NOT NULL,
            last_sequence INTEGER NOT NULL,
            snapshot_ref TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            compacted_at_ms INTEGER NOT NULL,
            PRIMARY KEY(conversation_id, first_sequence, last_sequence),
            CHECK(first_sequence > 0 AND last_sequence >= first_sequence)
        );",
    )?;
    ensure_column(
        connection,
        "conversation_log_client_messages",
        "dispatch_state",
        "TEXT NOT NULL DEFAULT 'accepted'",
    )?;
    ensure_column(
        connection,
        "conversation_log_compacted_ranges",
        "snapshot_payload",
        "BLOB NOT NULL DEFAULT X''",
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn accept_message(
    connection: &Connection,
    conversation_id: &str,
    workspace_id: &str,
    task_id: &str,
    client_message_id: &str,
    authoritative_payload: &[u8],
    renderer_payload: &[u8],
    content_hash: &str,
    timestamp_ms: i64,
) -> Result<MessageAcceptance, ConversationStoreError> {
    validate_id(conversation_id)?;
    validate_id(workspace_id)?;
    validate_id(task_id)?;
    validate_id(client_message_id)?;
    validate_payload(authoritative_payload, renderer_payload)?;
    if content_hash.len() != 64 || !content_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ConversationStoreError::InvalidInput);
    }

    let transaction = connection.unchecked_transaction()?;
    if let Some((stored_hash, stored_task_id, sequence, stored_workspace_id, dispatch_state)) = transaction
        .query_row(
            "SELECT messages.content_hash, messages.task_id, messages.event_sequence, metadata.workspace_id, messages.dispatch_state
             FROM conversation_log_client_messages messages
             JOIN conversation_log_metadata metadata ON metadata.conversation_id=messages.conversation_id
             WHERE messages.conversation_id=?1 AND messages.client_message_id=?2",
            params![conversation_id, client_message_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?
    {
        if stored_hash != content_hash {
            return Err(ConversationStoreError::IdempotencyConflict);
        }
        if stored_workspace_id != workspace_id {
            return Err(ConversationStoreError::InvalidInput);
        }
        let event =
            load_event(&transaction, conversation_id, sequence as u64)?.ok_or_else(|| {
                ConversationStoreError::Sql("accepted message event is missing".into())
            })?;
        transaction.commit()?;
        return Ok(MessageAcceptance {
            event,
            task_id: stored_task_id,
            deduplicated: true,
            dispatch_state,
        });
    }

    ensure_conversation(&transaction, conversation_id, workspace_id, timestamp_ms)?;
    let sequence = allocate_sequence(&transaction, conversation_id, timestamp_ms)?;
    let event = StoredConversationEvent {
        conversation_id: conversation_id.to_owned(),
        event_id: Uuid::now_v7().to_string(),
        sequence,
        timestamp_ms,
        kind: "user_message_accepted".into(),
        category: "message".into(),
        authoritative_payload: authoritative_payload.to_vec(),
        renderer_payload: renderer_payload.to_vec(),
        correlation_id: Some(client_message_id.to_owned()),
        causation_id: None,
        task_id: Some(task_id.to_owned()),
        run_id: None,
        turn_id: Some(task_id.to_owned()),
        client_message_id: Some(client_message_id.to_owned()),
        persistence_class: "durable".into(),
        sensitivity: "user_content".into(),
        schema_version: CONTRACT_VERSION,
    };
    insert_event(&transaction, &event)?;
    transaction.execute(
        "INSERT INTO conversation_log_client_messages
         (conversation_id, client_message_id, task_id, content_hash, event_sequence, accepted_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            conversation_id,
            client_message_id,
            task_id,
            content_hash,
            sequence as i64,
            timestamp_ms
        ],
    )?;
    transaction.execute(
        "INSERT INTO conversation_log_task_bindings
         (task_id, conversation_id, client_message_id, bound_at_ms) VALUES (?1,?2,?3,?4)",
        params![task_id, conversation_id, client_message_id, timestamp_ms],
    )?;
    transaction.commit()?;
    Ok(MessageAcceptance {
        event,
        task_id: task_id.to_owned(),
        deduplicated: false,
        dispatch_state: "accepted".into(),
    })
}

pub fn claim_message_dispatch(
    connection: &Connection,
    conversation_id: &str,
    client_message_id: &str,
) -> Result<bool, ConversationStoreError> {
    validate_id(conversation_id)?;
    validate_id(client_message_id)?;
    Ok(connection.execute(
        "UPDATE conversation_log_client_messages SET dispatch_state='dispatching' WHERE conversation_id=?1 AND client_message_id=?2 AND dispatch_state='accepted'",
        params![conversation_id, client_message_id],
    )? == 1)
}

pub fn finish_message_dispatch(
    connection: &Connection,
    conversation_id: &str,
    client_message_id: &str,
    dispatched: bool,
) -> Result<(), ConversationStoreError> {
    validate_id(conversation_id)?;
    validate_id(client_message_id)?;
    let state = if dispatched { "dispatched" } else { "accepted" };
    if connection.execute(
        "UPDATE conversation_log_client_messages SET dispatch_state=?3 WHERE conversation_id=?1 AND client_message_id=?2 AND dispatch_state='dispatching'",
        params![conversation_id, client_message_id, state],
    )? != 1 { return Err(ConversationStoreError::Sql("message dispatch state changed concurrently".into())); }
    Ok(())
}

pub fn append_event(
    connection: &Connection,
    event: NewConversationEvent<'_>,
) -> Result<StoredConversationEvent, ConversationStoreError> {
    validate_id(event.conversation_id)?;
    validate_id(event.workspace_id)?;
    validate_id(event.kind)?;
    validate_id(event.category)?;
    validate_payload(event.authoritative_payload, event.renderer_payload)?;
    if !matches!(
        event.persistence_class,
        "durable" | "compactable" | "transient_stream" | "derived_only"
    ) || event.sensitivity.is_empty()
        || event.sensitivity.len() > 64
    {
        return Err(ConversationStoreError::InvalidInput);
    }
    if let Some(task_id) = event.task_id {
        validate_id(task_id)?;
    }
    for value in [
        event.correlation_id,
        event.causation_id,
        event.run_id,
        event.turn_id,
    ]
    .into_iter()
    .flatten()
    {
        validate_id(value)?;
    }
    if let Some(client_message_id) = event.client_message_id {
        validate_id(client_message_id)?;
    }

    let transaction = connection.unchecked_transaction()?;
    ensure_conversation(
        &transaction,
        event.conversation_id,
        event.workspace_id,
        event.timestamp_ms,
    )?;
    let sequence = allocate_sequence(&transaction, event.conversation_id, event.timestamp_ms)?;
    let stored = StoredConversationEvent {
        conversation_id: event.conversation_id.to_owned(),
        event_id: Uuid::now_v7().to_string(),
        sequence,
        timestamp_ms: event.timestamp_ms,
        kind: event.kind.to_owned(),
        category: event.category.to_owned(),
        authoritative_payload: event.authoritative_payload.to_vec(),
        renderer_payload: event.renderer_payload.to_vec(),
        correlation_id: event.correlation_id.map(str::to_owned),
        causation_id: event.causation_id.map(str::to_owned),
        task_id: event.task_id.map(str::to_owned),
        run_id: event.run_id.map(str::to_owned),
        turn_id: event.turn_id.map(str::to_owned),
        client_message_id: event.client_message_id.map(str::to_owned),
        persistence_class: event.persistence_class.to_owned(),
        sensitivity: event.sensitivity.to_owned(),
        schema_version: CONTRACT_VERSION,
    };
    insert_event(&transaction, &stored)?;
    transaction.commit()?;
    Ok(stored)
}

pub fn history_after(
    connection: &Connection,
    conversation_id: &str,
    after_sequence: u64,
    limit: usize,
) -> Result<ConversationEventPage, ConversationStoreError> {
    validate_id(conversation_id)?;
    if limit == 0 || limit > MAX_PAGE_EVENTS {
        return Err(ConversationStoreError::InvalidInput);
    }
    let (oldest_available, latest) = conversation_range(connection, conversation_id)?;
    if after_sequence.saturating_add(1) < oldest_available {
        verify_compacted_boundary(connection, conversation_id, oldest_available)?;
        return Err(ConversationStoreError::CursorExpired {
            earliest_available_sequence: oldest_available,
        });
    }
    let mut statement = connection.prepare(
        "SELECT conversation_id,event_id,sequence,timestamp_ms,kind,category,
                authoritative_payload,renderer_payload,correlation_id,causation_id,task_id,run_id,turn_id,
                client_message_id,persistence_class,sensitivity,schema_version
         FROM conversation_log_events
         WHERE conversation_id=?1 AND sequence>?2 AND sequence>=?4 ORDER BY sequence ASC LIMIT ?3",
    )?;
    let events = statement
        .query_map(
            params![
                conversation_id,
                after_sequence as i64,
                (limit + 1) as i64,
                oldest_available as i64
            ],
            map_event,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let has_newer = events.len() > limit;
    let events = events.into_iter().take(limit).collect::<Vec<_>>();
    let oldest_sequence = events.first().map(|event| event.sequence);
    let newest_sequence = events.last().map(|event| event.sequence);
    let has_older = oldest_sequence.is_some_and(|sequence| sequence > oldest_available);
    Ok(ConversationEventPage {
        events,
        oldest_sequence,
        newest_sequence,
        has_older,
        has_newer: has_newer || newest_sequence.is_some_and(|sequence| sequence < latest),
        earliest_available_sequence: oldest_available,
    })
}

pub fn history_before(
    connection: &Connection,
    conversation_id: &str,
    before_sequence: u64,
    limit: usize,
) -> Result<ConversationEventPage, ConversationStoreError> {
    validate_id(conversation_id)?;
    if before_sequence == 0 || limit == 0 || limit > MAX_PAGE_EVENTS {
        return Err(ConversationStoreError::InvalidInput);
    }
    let (oldest_available, latest) = conversation_range(connection, conversation_id)?;
    if before_sequence <= oldest_available {
        return Ok(ConversationEventPage {
            events: Vec::new(),
            oldest_sequence: None,
            newest_sequence: None,
            has_older: false,
            has_newer: latest >= before_sequence,
            earliest_available_sequence: oldest_available,
        });
    }
    let mut statement = connection.prepare(
        "SELECT conversation_id,event_id,sequence,timestamp_ms,kind,category,
                authoritative_payload,renderer_payload,correlation_id,causation_id,task_id,run_id,turn_id,
                client_message_id,persistence_class,sensitivity,schema_version
         FROM conversation_log_events
         WHERE conversation_id=?1 AND sequence<?2 AND sequence>=?4 ORDER BY sequence DESC LIMIT ?3",
    )?;
    let mut events = statement
        .query_map(
            params![
                conversation_id,
                before_sequence as i64,
                (limit + 1) as i64,
                oldest_available as i64
            ],
            map_event,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let has_older = events.len() > limit;
    events.truncate(limit);
    events.reverse();
    let oldest_sequence = events.first().map(|event| event.sequence);
    let newest_sequence = events.last().map(|event| event.sequence);
    Ok(ConversationEventPage {
        has_newer: newest_sequence.is_some_and(|sequence| sequence < latest),
        events,
        oldest_sequence,
        newest_sequence,
        has_older: has_older || oldest_sequence.is_some_and(|sequence| sequence > oldest_available),
        earliest_available_sequence: oldest_available,
    })
}

/// Advances the logical retention boundary after a durable compacted snapshot
/// has been stored. Old rows remain as local audit material, but history APIs
/// can no longer use them as replay state and report a typed expired cursor.
pub fn record_compacted_prefix(
    connection: &Connection,
    conversation_id: &str,
    through_sequence: u64,
    snapshot_ref: &str,
    snapshot_payload: &[u8],
    compacted_at_ms: i64,
) -> Result<u64, ConversationStoreError> {
    validate_id(conversation_id)?;
    validate_id(snapshot_ref)?;
    if through_sequence == 0 || snapshot_payload.is_empty() || snapshot_payload.len() > 64 * 1024 {
        return Err(ConversationStoreError::InvalidInput);
    }
    let transaction = connection.unchecked_transaction()?;
    let (oldest, latest) = conversation_range(&transaction, conversation_id)?;
    if through_sequence < oldest || through_sequence >= latest {
        return Err(ConversationStoreError::InvalidInput);
    }
    transaction.execute(
        "INSERT INTO conversation_log_compacted_ranges
         (conversation_id,first_sequence,last_sequence,snapshot_ref,content_hash,snapshot_payload,compacted_at_ms)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![
            conversation_id,
            oldest as i64,
            through_sequence as i64,
            snapshot_ref,
            hex::encode(Sha256::digest(snapshot_payload)),
            snapshot_payload,
            compacted_at_ms
        ],
    )?;
    let next = through_sequence + 1;
    transaction.execute(
        "UPDATE conversation_log_metadata SET oldest_available_sequence=?2,updated_at_ms=?3
         WHERE conversation_id=?1",
        params![conversation_id, next as i64, compacted_at_ms],
    )?;
    transaction.commit()?;
    Ok(next)
}

fn verify_compacted_boundary(
    connection: &Connection,
    conversation_id: &str,
    oldest_available: u64,
) -> Result<(), ConversationStoreError> {
    let snapshot = connection.query_row(
        "SELECT content_hash,snapshot_payload FROM conversation_log_compacted_ranges WHERE conversation_id=?1 AND last_sequence=?2 ORDER BY compacted_at_ms DESC LIMIT 1",
        params![conversation_id, oldest_available.saturating_sub(1) as i64],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
    ).optional()?;
    let Some((expected, payload)) = snapshot else {
        return Err(ConversationStoreError::Sql(
            "compacted snapshot is missing".into(),
        ));
    };
    if payload.is_empty() || hex::encode(Sha256::digest(&payload)) != expected {
        return Err(ConversationStoreError::Sql(
            "compacted snapshot checksum mismatch".into(),
        ));
    }
    Ok(())
}

pub fn task_binding(
    connection: &Connection,
    task_id: &str,
) -> Result<Option<(String, String, String)>, ConversationStoreError> {
    validate_id(task_id)?;
    Ok(connection
        .query_row(
            "SELECT binding.conversation_id, binding.client_message_id, metadata.workspace_id
             FROM conversation_log_task_bindings binding
             JOIN conversation_log_metadata metadata ON metadata.conversation_id=binding.conversation_id
             WHERE binding.task_id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

fn validate_id(value: &str) -> Result<(), ConversationStoreError> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        return Err(ConversationStoreError::InvalidInput);
    }
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), ConversationStoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|known| known == column) {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

fn validate_payload(authoritative: &[u8], renderer: &[u8]) -> Result<(), ConversationStoreError> {
    if authoritative.len() > 64 * 1024 || renderer.len() > 64 * 1024 {
        return Err(ConversationStoreError::InvalidInput);
    }
    serde_json::from_slice::<serde_json::Value>(authoritative)
        .map_err(|_| ConversationStoreError::InvalidInput)?;
    serde_json::from_slice::<serde_json::Value>(renderer)
        .map_err(|_| ConversationStoreError::InvalidInput)?;
    Ok(())
}

fn ensure_conversation(
    connection: &Connection,
    conversation_id: &str,
    workspace_id: &str,
    timestamp_ms: i64,
) -> Result<(), ConversationStoreError> {
    connection.execute(
        "INSERT INTO conversation_log_metadata
         (conversation_id,workspace_id,next_sequence,oldest_available_sequence,schema_version,created_at_ms,updated_at_ms)
         VALUES (?1,?2,0,1,?3,?4,?4)
         ON CONFLICT(conversation_id) DO UPDATE SET updated_at_ms=excluded.updated_at_ms
         WHERE conversation_log_metadata.workspace_id=excluded.workspace_id",
        params![conversation_id, workspace_id, CONTRACT_VERSION, timestamp_ms],
    )?;
    let actual_workspace: String = connection.query_row(
        "SELECT workspace_id FROM conversation_log_metadata WHERE conversation_id=?1",
        [conversation_id],
        |row| row.get(0),
    )?;
    if actual_workspace != workspace_id {
        return Err(ConversationStoreError::InvalidInput);
    }
    Ok(())
}

fn allocate_sequence(
    connection: &Connection,
    conversation_id: &str,
    timestamp_ms: i64,
) -> Result<u64, ConversationStoreError> {
    let current: i64 = connection.query_row(
        "SELECT next_sequence FROM conversation_log_metadata WHERE conversation_id=?1",
        [conversation_id],
        |row| row.get(0),
    )?;
    let next = current
        .checked_add(1)
        .ok_or(ConversationStoreError::InvalidInput)?;
    connection.execute(
        "UPDATE conversation_log_metadata SET next_sequence=?2,updated_at_ms=?3 WHERE conversation_id=?1",
        params![conversation_id, next, timestamp_ms],
    )?;
    Ok(next as u64)
}

fn insert_event(
    connection: &Connection,
    event: &StoredConversationEvent,
) -> Result<(), ConversationStoreError> {
    connection.execute(
        "INSERT INTO conversation_log_events
         (conversation_id,sequence,event_id,timestamp_ms,kind,category,authoritative_payload,
          renderer_payload,correlation_id,causation_id,task_id,run_id,turn_id,client_message_id,
          persistence_class,sensitivity,schema_version)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            event.conversation_id,
            event.sequence as i64,
            event.event_id,
            event.timestamp_ms,
            event.kind,
            event.category,
            event.authoritative_payload,
            event.renderer_payload,
            event.correlation_id,
            event.causation_id,
            event.task_id,
            event.run_id,
            event.turn_id,
            event.client_message_id,
            event.persistence_class,
            event.sensitivity,
            event.schema_version,
        ],
    )?;
    Ok(())
}

fn load_event(
    connection: &Connection,
    conversation_id: &str,
    sequence: u64,
) -> Result<Option<StoredConversationEvent>, ConversationStoreError> {
    Ok(connection
        .query_row(
            "SELECT conversation_id,event_id,sequence,timestamp_ms,kind,category,
                    authoritative_payload,renderer_payload,correlation_id,causation_id,task_id,run_id,turn_id,
                    client_message_id,persistence_class,sensitivity,schema_version
             FROM conversation_log_events WHERE conversation_id=?1 AND sequence=?2",
            params![conversation_id, sequence as i64],
            map_event,
        )
        .optional()?)
}

fn map_event(row: &Row<'_>) -> rusqlite::Result<StoredConversationEvent> {
    Ok(StoredConversationEvent {
        conversation_id: row.get(0)?,
        event_id: row.get(1)?,
        sequence: row.get::<_, i64>(2)? as u64,
        timestamp_ms: row.get(3)?,
        kind: row.get(4)?,
        category: row.get(5)?,
        authoritative_payload: row.get(6)?,
        renderer_payload: row.get(7)?,
        correlation_id: row.get(8)?,
        causation_id: row.get(9)?,
        task_id: row.get(10)?,
        run_id: row.get(11)?,
        turn_id: row.get(12)?,
        client_message_id: row.get(13)?,
        persistence_class: row.get(14)?,
        sensitivity: row.get(15)?,
        schema_version: row.get::<_, i64>(16)? as u32,
    })
}

fn conversation_range(
    connection: &Connection,
    conversation_id: &str,
) -> Result<(u64, u64), ConversationStoreError> {
    connection
        .query_row(
            "SELECT oldest_available_sequence,next_sequence FROM conversation_log_metadata WHERE conversation_id=?1",
            [conversation_id],
            |row| Ok((row.get::<_, i64>(0)? as u64, row.get::<_, i64>(1)? as u64)),
        )
        .optional()?
        .ok_or(ConversationStoreError::ConversationNotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_payload(text: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({"content": text})).unwrap()
    }

    #[test]
    fn accepts_messages_once_and_keeps_per_conversation_sequence() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let first_payload = message_payload("одинаковый текст");
        let first = accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-1",
            "client-1",
            &first_payload,
            &first_payload,
            &"1".repeat(64),
            10,
        )
        .unwrap();
        let duplicate = accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-retry",
            "client-1",
            &first_payload,
            &first_payload,
            &"1".repeat(64),
            11,
        )
        .unwrap();
        let second_payload = message_payload("одинаковый текст");
        let second = accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-2",
            "client-2",
            &second_payload,
            &second_payload,
            &"2".repeat(64),
            12,
        )
        .unwrap();

        assert_eq!(first.event.sequence, 1);
        assert!(!first.deduplicated);
        assert_eq!(duplicate.event.event_id, first.event.event_id);
        assert_eq!(duplicate.task_id, "task-1");
        assert!(duplicate.deduplicated);
        assert_eq!(second.event.sequence, 2);
    }

    #[test]
    fn rejects_reusing_a_client_message_id_for_different_content() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let payload = message_payload("first");
        accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-1",
            "client-1",
            &payload,
            &payload,
            &"1".repeat(64),
            10,
        )
        .unwrap();

        let error = accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-2",
            "client-1",
            &message_payload("second"),
            &message_payload("second"),
            &"2".repeat(64),
            11,
        )
        .unwrap_err();
        assert_eq!(error, ConversationStoreError::IdempotencyConflict);
    }

    #[test]
    fn accepted_message_dispatch_is_claimed_once_and_retryable_after_definite_failure() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let payload = message_payload("dispatch");
        accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-1",
            "client-1",
            &payload,
            &payload,
            &"1".repeat(64),
            10,
        )
        .unwrap();
        assert!(claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
        assert!(!claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
        finish_message_dispatch(&connection, "conversation-1", "client-1", false).unwrap();
        assert!(claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
        finish_message_dispatch(&connection, "conversation-1", "client-1", true).unwrap();
        assert!(!claim_message_dispatch(&connection, "conversation-1", "client-1").unwrap());
    }

    #[test]
    fn duplicate_message_cannot_cross_the_conversation_workspace_binding() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        let payload = message_payload("same");
        accept_message(
            &connection,
            "conversation-1",
            "workspace-1",
            "task-1",
            "client-1",
            &payload,
            &payload,
            &"1".repeat(64),
            10,
        )
        .unwrap();

        let error = accept_message(
            &connection,
            "conversation-1",
            "workspace-2",
            "task-2",
            "client-1",
            &payload,
            &payload,
            &"1".repeat(64),
            11,
        )
        .unwrap_err();
        assert_eq!(error, ConversationStoreError::InvalidInput);
    }

    #[test]
    fn history_after_uses_a_stable_cursor_while_new_events_arrive() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        for index in 1..=3 {
            let payload = message_payload(&format!("event-{index}"));
            append_event(
                &connection,
                NewConversationEvent {
                    conversation_id: "conversation-1",
                    workspace_id: "workspace-1",
                    kind: "task_state",
                    category: "task",
                    authoritative_payload: &payload,
                    renderer_payload: &payload,
                    correlation_id: None,
                    causation_id: None,
                    task_id: Some("task-1"),
                    run_id: None,
                    turn_id: None,
                    client_message_id: None,
                    persistence_class: "durable",
                    sensitivity: "internal",
                    timestamp_ms: index,
                },
            )
            .unwrap();
        }

        let first_page = history_after(&connection, "conversation-1", 0, 2).unwrap();
        let later_payload = message_payload("event-4");
        append_event(
            &connection,
            NewConversationEvent {
                conversation_id: "conversation-1",
                workspace_id: "workspace-1",
                kind: "task_state",
                category: "task",
                authoritative_payload: &later_payload,
                renderer_payload: &later_payload,
                correlation_id: None,
                causation_id: None,
                task_id: Some("task-1"),
                run_id: None,
                turn_id: None,
                client_message_id: None,
                persistence_class: "durable",
                sensitivity: "internal",
                timestamp_ms: 4,
            },
        )
        .unwrap();
        let second_page = history_after(
            &connection,
            "conversation-1",
            first_page.newest_sequence.unwrap(),
            2,
        )
        .unwrap();

        assert_eq!(
            first_page
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(first_page.has_newer);
        assert_eq!(
            second_page
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
        assert!(!second_page.has_newer);
    }

    #[test]
    fn history_before_and_compacted_cursor_have_explicit_bounds() {
        let connection = Connection::open_in_memory().unwrap();
        install_schema(&connection).unwrap();
        for index in 1..=4 {
            let payload = message_payload(&format!("event-{index}"));
            append_event(
                &connection,
                NewConversationEvent {
                    conversation_id: "conversation-1",
                    workspace_id: "workspace-1",
                    kind: "task_state",
                    category: "task",
                    authoritative_payload: &payload,
                    renderer_payload: &payload,
                    correlation_id: None,
                    causation_id: None,
                    task_id: Some("task-1"),
                    run_id: None,
                    turn_id: None,
                    client_message_id: None,
                    persistence_class: "durable",
                    sensitivity: "internal",
                    timestamp_ms: index,
                },
            )
            .unwrap();
        }
        let before = history_before(&connection, "conversation-1", 4, 2).unwrap();
        assert_eq!(
            before
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(before.has_older);
        assert!(before.has_newer);

        let earliest = record_compacted_prefix(
            &connection,
            "conversation-1",
            2,
            "snapshot-1",
            br#"{"summary":"events 1-2"}"#,
            10,
        )
        .unwrap();
        assert_eq!(earliest, 3);
        assert_eq!(
            history_after(&connection, "conversation-1", 0, 2).unwrap_err(),
            ConversationStoreError::CursorExpired {
                earliest_available_sequence: 3
            }
        );
        let retained = history_after(&connection, "conversation-1", 2, 10).unwrap();
        assert_eq!(retained.earliest_available_sequence, 3);
        assert_eq!(
            retained
                .events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![3, 4]
        );
        connection
            .execute(
                "UPDATE conversation_log_compacted_ranges SET snapshot_payload=X'00' WHERE conversation_id='conversation-1'",
                [],
            )
            .unwrap();
        assert!(matches!(
            history_after(&connection, "conversation-1", 0, 2),
            Err(ConversationStoreError::Sql(message)) if message.contains("checksum")
        ));
    }

    #[test]
    fn schema_49_migrates_and_sequence_survives_database_reopen() {
        let path = std::env::temp_dir().join(format!(
            "evohime-conversation-log-{}-{}.db",
            std::process::id(),
            Uuid::now_v7()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let legacy = Connection::open(&path).unwrap();
            legacy.pragma_update(None, "user_version", 49).unwrap();
        }
        {
            let database = crate::LocalDatabase::open(&path).unwrap();
            assert_eq!(database.schema_version().unwrap(), crate::SCHEMA_VERSION);
            let payload = message_payload("first");
            let first = append_event(
                database.connection(),
                NewConversationEvent {
                    conversation_id: "conversation-1",
                    workspace_id: "workspace-1",
                    kind: "task_state",
                    category: "task",
                    authoritative_payload: &payload,
                    renderer_payload: &payload,
                    correlation_id: None,
                    causation_id: None,
                    task_id: Some("task-1"),
                    run_id: None,
                    turn_id: None,
                    client_message_id: None,
                    persistence_class: "durable",
                    sensitivity: "internal",
                    timestamp_ms: 1,
                },
            )
            .unwrap();
            assert_eq!(first.sequence, 1);
        }
        {
            let database = crate::LocalDatabase::open(&path).unwrap();
            let payload = message_payload("second");
            let second = append_event(
                database.connection(),
                NewConversationEvent {
                    conversation_id: "conversation-1",
                    workspace_id: "workspace-1",
                    kind: "task_state",
                    category: "task",
                    authoritative_payload: &payload,
                    renderer_payload: &payload,
                    correlation_id: None,
                    causation_id: None,
                    task_id: Some("task-2"),
                    run_id: None,
                    turn_id: None,
                    client_message_id: None,
                    persistence_class: "durable",
                    sensitivity: "internal",
                    timestamp_ms: 2,
                },
            )
            .unwrap();
            assert_eq!(second.sequence, 2);
        }
        let _ = std::fs::remove_file(path.with_extension("db.bak"));
        let _ = std::fs::remove_file(path);
    }
}
