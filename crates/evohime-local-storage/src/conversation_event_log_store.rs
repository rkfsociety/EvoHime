use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Schema version written on every persisted conversation event.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum number of events returned by a single history page.
pub const MAX_PAGE_EVENTS: usize = 200;

/// Persisted conversation event with separate authoritative and renderer payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredConversationEvent {
    /// Conversation that owns the event.
    pub conversation_id: String,
    /// Globally unique event identifier.
    pub event_id: String,
    /// Monotonic sequence within the conversation.
    pub sequence: u64,
    /// Event timestamp in Unix milliseconds.
    pub timestamp_ms: i64,
    /// Event kind used by consumers.
    pub kind: String,
    /// Event category used for routing or filtering.
    pub category: String,
    /// Canonical payload used by Core as authoritative state.
    pub authoritative_payload: Vec<u8>,
    /// Sanitized payload suitable for renderer delivery.
    pub renderer_payload: Vec<u8>,
    /// Optional request/correlation identifier.
    pub correlation_id: Option<String>,
    /// Optional identifier of the event that caused this event.
    pub causation_id: Option<String>,
    /// Optional task identifier associated with the event.
    pub task_id: Option<String>,
    /// Optional run identifier associated with the event.
    pub run_id: Option<String>,
    /// Optional conversation turn identifier.
    pub turn_id: Option<String>,
    /// Optional client message ID used for idempotent acceptance.
    pub client_message_id: Option<String>,
    /// Retention class that governs compaction eligibility.
    pub persistence_class: String,
    /// Sensitivity classification for the event data.
    pub sensitivity: String,
    /// Payload schema version.
    pub schema_version: u32,
}

/// Input fields for appending a conversation event.
#[derive(Debug, Clone)]
pub struct NewConversationEvent<'a> {
    /// Conversation receiving the event.
    pub conversation_id: &'a str,
    /// Workspace that owns the conversation.
    pub workspace_id: &'a str,
    /// Event kind.
    pub kind: &'a str,
    /// Event category.
    pub category: &'a str,
    /// Canonical Core payload.
    pub authoritative_payload: &'a [u8],
    /// Renderer-safe payload.
    pub renderer_payload: &'a [u8],
    /// Optional correlation identifier.
    pub correlation_id: Option<&'a str>,
    /// Optional causating event identifier.
    pub causation_id: Option<&'a str>,
    /// Optional task identifier.
    pub task_id: Option<&'a str>,
    /// Optional run identifier.
    pub run_id: Option<&'a str>,
    /// Optional turn identifier.
    pub turn_id: Option<&'a str>,
    /// Optional client message identifier.
    pub client_message_id: Option<&'a str>,
    /// Retention class (`durable`, `compactable`, `transient_stream`, or `derived_only`).
    pub persistence_class: &'a str,
    /// Sensitivity classification.
    pub sensitivity: &'a str,
    /// Event timestamp in Unix milliseconds.
    pub timestamp_ms: i64,
}

/// Result of accepting a client message into the conversation log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageAcceptance {
    /// Accepted or previously stored event.
    pub event: StoredConversationEvent,
    /// Task identifier bound to this accepted message.
    pub task_id: String,
    /// Whether the request matched a previously accepted message.
    pub deduplicated: bool,
    /// Current durable dispatch state for the message.
    pub dispatch_state: String,
}

/// A bounded conversation history page and its cursor metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationEventPage {
    /// Events included in sequence order.
    pub events: Vec<StoredConversationEvent>,
    /// First event sequence in this page, if non-empty.
    pub oldest_sequence: Option<u64>,
    /// Last event sequence in this page, if non-empty.
    pub newest_sequence: Option<u64>,
    /// Whether an earlier page may be available.
    pub has_older: bool,
    /// Whether a later page may be available.
    pub has_newer: bool,
    /// Oldest sequence still valid as replay history.
    pub earliest_available_sequence: u64,
}

/// Validation, idempotency, cursor-retention, and database errors for conversation storage.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConversationStoreError {
    /// An identifier, payload, or page parameter failed validation.
    #[error("conversation input is invalid")]
    InvalidInput,
    /// A client message ID was reused with a different content hash.
    #[error("client message id was reused with different content")]
    IdempotencyConflict,
    /// Requested history precedes the retained replay boundary.
    #[error("history cursor is no longer retained")]
    CursorExpired {
        /// Earliest sequence that remains available for history paging.
        earliest_available_sequence: u64,
    },
    /// The conversation metadata does not exist.
    #[error("conversation was not found")]
    ConversationNotFound,
    /// An underlying SQL operation or persisted-data check failed.
    #[error("conversation storage failed: {0}")]
    Sql(String),
}

impl From<rusqlite::Error> for ConversationStoreError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sql(value.to_string())
    }
}

/// Creates the conversation event log and its deduplication, binding, and compaction tables.
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

/// Fields for idempotently accepting a client message into a conversation.
pub struct AcceptMessageInput<'a> {
    /// Conversation receiving the message.
    pub conversation_id: &'a str,
    /// Workspace expected to own the conversation.
    pub workspace_id: &'a str,
    /// Task created or associated with this message.
    pub task_id: &'a str,
    /// Client-generated idempotency identifier.
    pub client_message_id: &'a str,
    /// Canonical message payload used by Core.
    pub authoritative_payload: &'a [u8],
    /// Renderer-safe message payload.
    pub renderer_payload: &'a [u8],
    /// Expected lowercase or uppercase hexadecimal content hash.
    pub content_hash: &'a str,
    /// Acceptance timestamp in Unix milliseconds.
    pub timestamp_ms: i64,
}

/// Accepts a message once by client ID and content hash, returning prior state on identical replay.
pub fn accept_message(
    connection: &Connection,
    input: AcceptMessageInput<'_>,
) -> Result<MessageAcceptance, ConversationStoreError> {
    let AcceptMessageInput {
        conversation_id,
        workspace_id,
        task_id,
        client_message_id,
        authoritative_payload,
        renderer_payload,
        content_hash,
        timestamp_ms,
    } = input;
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

/// Claims one accepted message for dispatch; only the first caller receives `true`.
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

/// Marks a claimed message dispatched or returns it to accepted state after failure.
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

/// Appends a conversation event in its own transaction.
pub fn append_event(
    connection: &Connection,
    event: NewConversationEvent<'_>,
) -> Result<StoredConversationEvent, ConversationStoreError> {
    let transaction = connection.unchecked_transaction()?;
    let stored = append_event_in_transaction(&transaction, event)?;
    transaction.commit()?;
    Ok(stored)
}

/// Appends a conversation event using the caller's transaction.
pub fn append_event_in_transaction(
    transaction: &Transaction<'_>,
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

    ensure_conversation(
        transaction,
        event.conversation_id,
        event.workspace_id,
        event.timestamp_ms,
    )?;
    let sequence = allocate_sequence(transaction, event.conversation_id, event.timestamp_ms)?;
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
    insert_event(transaction, &stored)?;
    Ok(stored)
}

/// Reads events after a sequence cursor, rejecting cursors older than retained history.
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

/// Reads events before a sequence cursor, returning them in ascending sequence order.
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

/// Reads only durable user and assistant messages before a conversation cursor.
///
/// The newest matching messages are returned in chronological order. This is
/// intended for bounded model context assembly and deliberately excludes tool,
/// usage, status, and streaming events.
pub fn model_history_before(
    connection: &Connection,
    conversation_id: &str,
    before_sequence: u64,
    limit: usize,
) -> Result<Vec<StoredConversationEvent>, ConversationStoreError> {
    validate_id(conversation_id)?;
    if before_sequence == 0 || limit == 0 || limit > MAX_PAGE_EVENTS {
        return Err(ConversationStoreError::InvalidInput);
    }
    let (oldest_available, _) = conversation_range(connection, conversation_id)?;
    if before_sequence <= oldest_available {
        return Ok(Vec::new());
    }
    let mut statement = connection.prepare(
        "SELECT conversation_id,event_id,sequence,timestamp_ms,kind,category,
                authoritative_payload,renderer_payload,correlation_id,causation_id,task_id,run_id,turn_id,
                client_message_id,persistence_class,sensitivity,schema_version
         FROM conversation_log_events
         WHERE conversation_id=?1 AND sequence<?2 AND sequence>=?3
           AND persistence_class='durable'
           AND kind IN ('user_message_accepted','assistant_message_finalized')
         ORDER BY sequence DESC LIMIT ?4",
    )?;
    let mut events = statement
        .query_map(
            params![
                conversation_id,
                before_sequence as i64,
                oldest_available as i64,
                limit as i64
            ],
            map_event,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    events.reverse();
    Ok(events)
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

/// Returns the conversation, client message, and workspace bound to a task, if present.
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
#[path = "conversation_event_log_store_tests.rs"]
mod tests;
