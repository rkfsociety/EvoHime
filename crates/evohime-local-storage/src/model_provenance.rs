//! Durable Core-owned model request provenance (планы 05.2, 05.5, 05.7, 05.8).

use evohime_model_provenance::{
    validate_no_credentials, ModelRequestEnvelopeV1, ModelRequestReceiptV1, ProvenanceError,
    RequestStatus, MAX_REQUEST_ENVELOPE_BYTES, PROVENANCE_RETENTION_DAYS,
};
use evohime_receipts::canonicalize_json;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;

pub const MODEL_PROVENANCE_SCHEMA_VERSION: u32 = 2;
pub const PROVENANCE_RETENTION_MS: i64 = PROVENANCE_RETENTION_DAYS * 24 * 60 * 60 * 1000;

#[derive(Debug, Error)]
pub enum ModelProvenanceError {
    #[error("{0}")]
    Contract(#[from] ProvenanceError),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("REQUEST_HASH_COLLISION")]
    HashCollision,
    #[error("REQUEST_PROVENANCE_COMMIT_FAILED: {0}")]
    CommitFailed(String),
}

pub type Result<T> = std::result::Result<T, ModelProvenanceError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitMode {
    FullForDispatch,
    HashOnlyStorage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestRecord {
    pub request_id: String,
    pub logical_request_id: String,
    pub attempt: u32,
    pub parent_request_id: Option<String>,
    pub previous_request_hash: Option<String>,
    pub request_kind: String,
    pub ledger_id: String,
    pub provider: String,
    pub model: String,
    pub envelope_version: u32,
    pub payload_mode: String,
    pub envelope_hash: Option<String>,
    pub envelope_blob: Vec<u8>,
    pub context_projection_hash: String,
    pub route_snapshot_hash: String,
    pub policy_snapshot_hash: String,
    pub route_policy_hash_shared: bool,
    pub status: String,
    pub dispatch_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResponseRecord {
    pub response_id: String,
    pub request_id: String,
    pub status: String,
    pub output: Option<String>,
    pub output_hash: Option<String>,
    pub finish_reason: Option<String>,
    pub started_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolIntentRecord {
    pub intent_id: String,
    pub origin_request_id: String,
    pub origin_request_envelope_hash: String,
    pub response_id: Option<String>,
    pub ordinal: u32,
    pub origin_kind: String,
    pub tool_name: String,
    pub tool_args_hash: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowOriginalRecord {
    pub shadow_id: String,
    pub ledger_id: String,
    pub request_id: String,
    pub original_kind: String,
    pub original_id: String,
    pub operation: String,
    pub parent_shadow_id: Option<String>,
    pub content_block_hash: Option<String>,
    pub source_state: String,
    pub original_content_hash: Option<String>,
    pub byte_len: u64,
    pub created_at: i64,
}

pub trait ProvenanceBundleSigner {
    fn key_id(&self) -> String;
    fn sign_manifest_digest(&self, digest: &[u8]) -> Result<Vec<u8>>;
    fn public_key_hex(&self) -> Option<String> {
        None
    }
    fn key_history_jsonl(&self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
    fn checkpoints_jsonl(&self) -> Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleVerification {
    pub valid: bool,
    pub request_id: String,
    pub verification_state: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestReceiptRecord {
    pub receipt_id: String,
    pub request_id: String,
    pub receipt_hash: String,
    pub request_envelope_hash: String,
    pub previous_receipt_hash: Option<String>,
    pub key_id: String,
    pub created_at: i64,
}

pub fn install_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS model_provenance_meta (id INTEGER PRIMARY KEY CHECK(id=1), schema_version INTEGER NOT NULL);
         INSERT OR IGNORE INTO model_provenance_meta(id, schema_version) VALUES (1, 2);
         CREATE TABLE IF NOT EXISTS model_requests (
           request_id TEXT PRIMARY KEY,
           logical_request_id TEXT NOT NULL,
           attempt INTEGER NOT NULL CHECK(attempt > 0),
           parent_request_id TEXT,
           previous_request_hash TEXT,
           request_kind TEXT NOT NULL,
           ledger_id TEXT NOT NULL,
           provider TEXT NOT NULL,
           model TEXT NOT NULL,
           envelope_version INTEGER NOT NULL,
           payload_mode TEXT NOT NULL CHECK(payload_mode IN ('full','hash_only')),
           envelope_hash TEXT,
           envelope_blob BLOB NOT NULL,
           context_projection_hash TEXT NOT NULL,
           route_snapshot_hash TEXT NOT NULL,
           policy_snapshot_hash TEXT NOT NULL,
           route_policy_hash_shared INTEGER NOT NULL CHECK(route_policy_hash_shared IN (0,1)),
           status TEXT NOT NULL CHECK(status IN ('active','completed','failed','interrupted','unknown_outcome','redacted','retention_pruned')),
           dispatch_at INTEGER,
           completed_at INTEGER,
           UNIQUE(logical_request_id, attempt),
           FOREIGN KEY(ledger_id) REFERENCES context_ledger(id),
           CHECK((payload_mode='full' AND envelope_hash IS NOT NULL) OR (payload_mode='hash_only' AND envelope_hash IS NULL)),
           CHECK((attempt=1 AND parent_request_id IS NULL AND previous_request_hash IS NULL) OR (attempt>1 AND parent_request_id IS NOT NULL AND previous_request_hash IS NOT NULL))
         );
         CREATE TABLE IF NOT EXISTS model_request_sources (
           request_id TEXT NOT NULL REFERENCES model_requests(request_id), ordinal INTEGER NOT NULL,
           source_ref_id TEXT NOT NULL UNIQUE, source_kind TEXT NOT NULL, source_id TEXT NOT NULL,
           source_version TEXT, source_hash TEXT, PRIMARY KEY(request_id, ordinal)
         );
         CREATE TABLE IF NOT EXISTS model_request_blocks (
           content_hash TEXT PRIMARY KEY, byte_len INTEGER NOT NULL, bytes BLOB, refcount INTEGER NOT NULL DEFAULT 0, last_referenced_at INTEGER
         );
         CREATE TABLE IF NOT EXISTS model_request_block_refs (
           request_id TEXT NOT NULL REFERENCES model_requests(request_id), ordinal INTEGER NOT NULL,
           role TEXT NOT NULL, block_ref_id TEXT NOT NULL UNIQUE, content_hash TEXT NOT NULL REFERENCES model_request_blocks(content_hash),
           PRIMARY KEY(request_id, ordinal)
         );
         CREATE TABLE IF NOT EXISTS model_responses (
           response_id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE REFERENCES model_requests(request_id),
           status TEXT NOT NULL CHECK(status IN ('complete','interrupted','failed','redacted')), output TEXT, output_hash TEXT,
           finish_reason TEXT, started_at INTEGER NOT NULL, completed_at INTEGER
         );
         CREATE TABLE IF NOT EXISTS tool_intents (
           intent_id TEXT PRIMARY KEY, origin_request_id TEXT NOT NULL REFERENCES model_requests(request_id),
           origin_request_envelope_hash TEXT NOT NULL, response_id TEXT REFERENCES model_responses(response_id), ordinal INTEGER NOT NULL,
           origin_kind TEXT NOT NULL CHECK(origin_kind IN ('assistant_response','system','recovery')), tool_name TEXT NOT NULL,
           tool_args_hash TEXT NOT NULL, state TEXT NOT NULL, UNIQUE(response_id, ordinal)
         );
         CREATE TABLE IF NOT EXISTS tool_intent_receipt_links (
           intent_id TEXT PRIMARY KEY REFERENCES tool_intents(intent_id), action_id TEXT NOT NULL,
           terminal_receipt_hash TEXT NOT NULL, linked_at INTEGER NOT NULL,
           UNIQUE(action_id), UNIQUE(terminal_receipt_hash)
         );
         CREATE TABLE IF NOT EXISTS provenance_tombstones (
           tombstone_id TEXT PRIMARY KEY, request_id TEXT NOT NULL REFERENCES model_requests(request_id), subject_kind TEXT NOT NULL,
           subject_ordinal INTEGER, subject_id TEXT, state TEXT NOT NULL CHECK(state IN ('redacted','retention_pruned')),
           source_disposition TEXT NOT NULL CHECK(source_disposition IN ('digest_kept','hash_removed')), marker_version INTEGER NOT NULL DEFAULT 1,
           created_at INTEGER NOT NULL, UNIQUE(request_id, subject_kind, subject_ordinal, subject_id, state)
         );
         CREATE TABLE IF NOT EXISTS context_shadowed_originals (
           shadow_id TEXT PRIMARY KEY, ledger_id TEXT NOT NULL REFERENCES context_ledger(id), request_id TEXT NOT NULL REFERENCES model_requests(request_id),
           original_kind TEXT NOT NULL CHECK(original_kind IN ('selected','compression','dropped')), original_id TEXT NOT NULL,
           operation TEXT NOT NULL CHECK(operation IN ('summary','prune')), parent_shadow_id TEXT REFERENCES context_shadowed_originals(shadow_id),
           content_block_hash TEXT, source_state TEXT NOT NULL CHECK(source_state IN ('full','metadata_hash_only','redacted','retention_pruned')),
           original_content_hash TEXT, byte_len INTEGER NOT NULL, created_at INTEGER NOT NULL,
           UNIQUE(request_id, original_kind, original_id, operation)
         );
         CREATE TABLE IF NOT EXISTS context_shadow_source_refs (
           shadow_id TEXT NOT NULL REFERENCES context_shadowed_originals(shadow_id), request_id TEXT NOT NULL, source_ref_ordinal INTEGER NOT NULL, source_ordinal INTEGER NOT NULL,
           PRIMARY KEY(shadow_id, source_ref_ordinal), FOREIGN KEY(request_id, source_ordinal) REFERENCES model_request_sources(request_id, ordinal)
         );
         CREATE TABLE IF NOT EXISTS context_shadow_blocks (
           content_hash TEXT PRIMARY KEY, byte_len INTEGER NOT NULL, bytes BLOB
         );
         CREATE INDEX IF NOT EXISTS idx_model_requests_logical_attempt ON model_requests(logical_request_id, attempt);
         CREATE INDEX IF NOT EXISTS idx_model_requests_ledger_attempt ON model_requests(ledger_id, attempt);
         CREATE INDEX IF NOT EXISTS idx_model_requests_parent ON model_requests(parent_request_id);
         CREATE INDEX IF NOT EXISTS idx_model_sources_kind_id ON model_request_sources(source_kind, source_id, source_version, request_id);
         CREATE INDEX IF NOT EXISTS idx_model_block_refs_hash ON model_request_block_refs(content_hash, request_id);
         CREATE INDEX IF NOT EXISTS idx_model_block_refs_role ON model_request_block_refs(role, request_id);",
    )?;
    // Existing ledger receipts stay compatible; request linkage is additive
    // and is only populated for the model_request receipt domain.
    let _ = connection.execute(
        "ALTER TABLE context_ledger_receipts ADD COLUMN request_id TEXT",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE context_ledger_receipts ADD COLUMN request_envelope_hash TEXT",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE context_ledger_receipts ADD COLUMN receipt_domain TEXT",
        [],
    );
    let _ = connection.execute(
        "ALTER TABLE context_ledger_receipts ADD COLUMN receipt_type TEXT",
        [],
    );
    let response_sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='model_responses'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if response_sql.is_some_and(|sql| !sql.contains("'redacted'")) {
        let tx = connection.unchecked_transaction()?;
        tx.execute_batch(
            "ALTER TABLE model_responses RENAME TO model_responses_legacy;
             CREATE TABLE model_responses (
               response_id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE REFERENCES model_requests(request_id),
               status TEXT NOT NULL CHECK(status IN ('complete','interrupted','failed','redacted')), output TEXT, output_hash TEXT,
               finish_reason TEXT, started_at INTEGER NOT NULL, completed_at INTEGER
             );
             INSERT INTO model_responses(response_id,request_id,status,output,output_hash,finish_reason,started_at,completed_at)
               SELECT response_id,request_id,status,output,output_hash,finish_reason,started_at,completed_at FROM model_responses_legacy;
             DROP TABLE model_responses_legacy;",
        )?;
        tx.commit()?;
    }
    let _ = connection.execute("CREATE UNIQUE INDEX IF NOT EXISTS idx_context_ledger_receipts_request ON context_ledger_receipts(request_id) WHERE request_id IS NOT NULL", []);
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS model_request_receipts (
           receipt_id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE REFERENCES model_requests(request_id),
           receipt_hash TEXT NOT NULL, canonical_payload BLOB NOT NULL, previous_receipt_hash TEXT,
           key_id TEXT NOT NULL, created_at INTEGER NOT NULL
         );",
    )?;
    Ok(())
}

pub struct ModelProvenanceRepository<'a> {
    connection: &'a Connection,
}

impl<'a> ModelProvenanceRepository<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn commit_envelope(
        &self,
        envelope: &ModelRequestEnvelopeV1,
        mode: CommitMode,
    ) -> Result<ModelRequestRecord> {
        envelope.validate()?;
        let bytes = envelope.canonical_bytes()?;
        validate_no_credentials(&serde_json::to_value(envelope)?)?;
        let envelope_hash = match mode {
            CommitMode::FullForDispatch => Some(envelope.envelope_hash()?),
            CommitMode::HashOnlyStorage => None,
        };
        let storage_bytes = storage_envelope_bytes(envelope)?;
        if matches!(mode, CommitMode::FullForDispatch) && bytes.len() > MAX_REQUEST_ENVELOPE_BYTES {
            return Err(ModelProvenanceError::CommitFailed(
                "full payload missing".into(),
            ));
        }
        let tx = self.connection.unchecked_transaction()?;
        let result = commit_tx(
            &tx,
            envelope,
            &storage_bytes,
            envelope_hash.as_deref(),
            mode,
        );
        match result {
            Ok(record) => {
                tx.commit()?;
                Ok(record)
            }
            Err(error) => Err(error),
        }
    }

    pub fn get(&self, request_id: &str) -> Result<Option<ModelRequestRecord>> {
        Ok(self.connection.query_row("SELECT request_id,logical_request_id,attempt,parent_request_id,previous_request_hash,request_kind,ledger_id,provider,model,envelope_version,payload_mode,envelope_hash,envelope_blob,context_projection_hash,route_snapshot_hash,policy_snapshot_hash,route_policy_hash_shared,status,dispatch_at,completed_at FROM model_requests WHERE request_id=?1", [request_id], row_record).optional()?)
    }

    pub fn mark_dispatch(&self, request_id: &str, at: i64) -> Result<()> {
        let changed = self.connection.execute("UPDATE model_requests SET dispatch_at=?2 WHERE request_id=?1 AND status='active' AND dispatch_at IS NULL", params![request_id, at])?;
        if changed != 1 {
            return Err(ModelProvenanceError::CommitFailed(
                "dispatch marker was not committed".into(),
            ));
        }
        Ok(())
    }

    /// Регистрирует уже подписанный request receipt в одной транзакции с
    /// linkage на ledger. Подпись создаётся существующим Core receipt signer;
    /// repository не принимает prompt или credentials как часть receipt.
    pub fn link_request_receipt(
        &self,
        receipt: &RequestReceiptRecord,
        canonical_payload: &[u8],
    ) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let request: (String, String) = tx.query_row("SELECT ledger_id,envelope_hash FROM model_requests WHERE request_id=?1 AND payload_mode='full'", [&receipt.request_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if request.1 != receipt.request_envelope_hash || canonical_payload.is_empty() {
            return Err(ModelProvenanceError::Contract(
                ProvenanceError::ReceiptLinkageMismatch,
            ));
        }
        let payload: ModelRequestReceiptV1 = serde_json::from_slice(canonical_payload)?;
        payload.validate()?;
        if payload.request_id != receipt.request_id
            || payload.request_envelope_hash != receipt.request_envelope_hash
        {
            return Err(ModelProvenanceError::Contract(
                ProvenanceError::ReceiptLinkageMismatch,
            ));
        }
        tx.execute("INSERT INTO model_request_receipts(receipt_id,request_id,receipt_hash,canonical_payload,previous_receipt_hash,key_id,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(request_id) DO NOTHING", params![receipt.receipt_id, receipt.request_id, receipt.receipt_hash, canonical_payload, receipt.previous_receipt_hash, receipt.key_id, receipt.created_at])?;
        tx.execute("INSERT OR IGNORE INTO context_ledger_receipts(ledger_id,receipt_id,exported) VALUES (?1,?2,0)", params![request.0, receipt.receipt_id])?;
        tx.execute("UPDATE context_ledger_receipts SET request_id=?2,request_envelope_hash=(SELECT envelope_hash FROM model_requests WHERE request_id=?2),receipt_domain='model_request',receipt_type='request_commit' WHERE ledger_id=?1 AND receipt_id=?3", params![request.0, receipt.request_id, receipt.receipt_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_status(
        &self,
        request_id: &str,
        status: RequestStatus,
        completed_at: Option<i64>,
    ) -> Result<()> {
        let status = serde_json::to_string(&status)?.trim_matches('"').to_owned();
        if matches!(status.as_str(), "redacted" | "retention_pruned") {
            return self.redact_request(request_id, &status);
        }
        self.connection.execute("UPDATE model_requests SET status=?2, completed_at=?3 WHERE request_id=?1 AND status='active'", params![request_id, status, completed_at.or_else(now_ms)])?;
        Ok(())
    }

    pub fn insert_response(&self, response: &ModelResponseRecord) -> Result<()> {
        let output_hash = response.output.as_deref().map(hash_bytes);
        self.connection.execute("INSERT INTO model_responses(response_id,request_id,status,output,output_hash,finish_reason,started_at,completed_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(request_id) DO NOTHING", params![response.response_id, response.request_id, response.status, response.output, output_hash, response.finish_reason, response.started_at, response.completed_at])?;
        Ok(())
    }

    /// Согласованный workspace snapshot: bytes и metadata читаются из одного
    /// stable наблюдения; при гонке выполняется bounded повтор, а текущий
    /// файл после commit больше не используется для реконструкции.
    pub fn capture_workspace_evidence(
        &self,
        request_id: &str,
        source_ref_id: &str,
        canonical_path: &Path,
        source_version: &str,
    ) -> Result<String> {
        for _ in 0..3 {
            let before = fs::metadata(canonical_path)?;
            let bytes = fs::read(canonical_path)?;
            let after = fs::metadata(canonical_path)?;
            if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
                continue;
            }
            let hash = hash_bytes_bytes(&bytes);
            if source_version != "workspace-v1" && source_version != hash {
                return Err(ProvenanceError::SourceChanged.into());
            }
            let tx = self.connection.unchecked_transaction()?;
            let changed = tx.execute("UPDATE model_request_sources SET source_kind='workspace_file',source_id=?3,source_version=?4,source_hash=?5 WHERE request_id=?1 AND source_ref_id=?2", params![request_id, source_ref_id, canonical_path.to_string_lossy(), source_version, hash])?;
            if changed != 1 {
                return Err(ProvenanceError::SourceMissing.into());
            }
            tx.execute("INSERT INTO model_request_blocks(content_hash,byte_len,bytes,refcount,last_referenced_at) VALUES (?1,?2,?3,1,?4) ON CONFLICT(content_hash) DO UPDATE SET bytes=COALESCE(model_request_blocks.bytes,excluded.bytes),refcount=model_request_blocks.refcount+1,last_referenced_at=excluded.last_referenced_at", params![hash, bytes.len() as i64, bytes, now_ms()])?;
            tx.execute("INSERT INTO model_request_block_refs(request_id,ordinal,role,block_ref_id,content_hash) SELECT ?1,COALESCE(MAX(ordinal),-1)+1,'evidence',?2,?3 FROM model_request_block_refs WHERE request_id=?1", params![request_id, source_ref_id, hash])?;
            tx.commit()?;
            return Ok(hash);
        }
        Err(ProvenanceError::SourceChanged.into())
    }

    pub fn insert_tool_intent(&self, intent: &ToolIntentRecord) -> Result<()> {
        let request_hash: String = self.connection.query_row(
            "SELECT envelope_hash FROM model_requests WHERE request_id=?1",
            [&intent.origin_request_id],
            |row| row.get(0),
        )?;
        if request_hash != intent.origin_request_envelope_hash {
            return Err(ModelProvenanceError::Contract(
                ProvenanceError::ToolLinkageMismatch,
            ));
        }
        if let Some(response_id) = &intent.response_id {
            let owner: String = self.connection.query_row(
                "SELECT request_id FROM model_responses WHERE response_id=?1",
                [response_id],
                |row| row.get(0),
            )?;
            if owner != intent.origin_request_id {
                return Err(ModelProvenanceError::Contract(
                    ProvenanceError::ToolLinkageMismatch,
                ));
            }
        }
        self.connection.execute("INSERT INTO tool_intents(intent_id,origin_request_id,origin_request_envelope_hash,response_id,ordinal,origin_kind,tool_name,tool_args_hash,state) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)", params![intent.intent_id, intent.origin_request_id, intent.origin_request_envelope_hash, intent.response_id, intent.ordinal, intent.origin_kind, intent.tool_name, intent.tool_args_hash, intent.state])?;
        Ok(())
    }

    pub fn link_tool_receipt(
        &self,
        task_id: &str,
        tool_name: &str,
        action_id: &str,
        terminal_receipt_hash: &str,
    ) -> Result<()> {
        let intent: Option<String> = self
            .connection
            .query_row(
                "SELECT i.intent_id FROM tool_intents i JOIN model_requests r ON r.request_id=i.origin_request_id WHERE r.logical_request_id LIKE ?1 || ':%' AND i.tool_name=?2 AND NOT EXISTS (SELECT 1 FROM tool_intent_receipt_links l WHERE l.intent_id=i.intent_id) ORDER BY r.attempt DESC,i.ordinal LIMIT 1",
                params![task_id, tool_name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(intent_id) = intent {
            self.connection.execute(
                "INSERT OR IGNORE INTO tool_intent_receipt_links(intent_id,action_id,terminal_receipt_hash,linked_at) VALUES (?1,?2,?3,?4)",
                params![intent_id, action_id, terminal_receipt_hash, now_ms()],
            )?;
        }
        Ok(())
    }

    /// Сохраняет вытесненный original append-only. Повторный prune той же
    /// ledger item идемпотентен; содержимое не смешивается с prompt blocks.
    pub fn append_shadow_original(
        &self,
        record: &ShadowOriginalRecord,
        bytes: Option<&[u8]>,
    ) -> Result<()> {
        if record.operation != "summary" && record.operation != "prune" {
            return Err(ModelProvenanceError::Contract(ProvenanceError::Invalid(
                "shadow operation".into(),
            )));
        }
        if bytes.is_some_and(|value| {
            hash_bytes_bytes(value) != record.original_content_hash.clone().unwrap_or_default()
        }) {
            return Err(ModelProvenanceError::Contract(
                ProvenanceError::HashMismatch,
            ));
        }
        let tx = self.connection.unchecked_transaction()?;
        if let Some(value) = bytes {
            tx.execute("INSERT INTO context_shadow_blocks(content_hash,byte_len,bytes) VALUES (?1,?2,?3) ON CONFLICT(content_hash) DO UPDATE SET bytes=COALESCE(context_shadow_blocks.bytes, excluded.bytes)", params![record.original_content_hash, value.len() as i64, value])?;
        }
        tx.execute("INSERT OR IGNORE INTO context_shadowed_originals(shadow_id,ledger_id,request_id,original_kind,original_id,operation,parent_shadow_id,content_block_hash,source_state,original_content_hash,byte_len,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![record.shadow_id, record.ledger_id, record.request_id, record.original_kind, record.original_id, record.operation, record.parent_shadow_id, record.original_content_hash, if bytes.is_some() { "full" } else { "metadata_hash_only" }, record.original_content_hash, record.byte_len, record.created_at])?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_shadow_originals(
        &self,
        request_id: &str,
        limit: usize,
    ) -> Result<Vec<ShadowOriginalRecord>> {
        let mut statement = self.connection.prepare("SELECT shadow_id,ledger_id,request_id,original_kind,original_id,operation,parent_shadow_id,content_block_hash,source_state,original_content_hash,byte_len,created_at FROM context_shadowed_originals WHERE request_id=?1 ORDER BY created_at,shadow_id LIMIT ?2")?;
        let rows = statement
            .query_map(params![request_id, limit.min(4096) as i64], |row| {
                Ok(ShadowOriginalRecord {
                    shadow_id: row.get(0)?,
                    ledger_id: row.get(1)?,
                    request_id: row.get(2)?,
                    original_kind: row.get(3)?,
                    original_id: row.get(4)?,
                    operation: row.get(5)?,
                    parent_shadow_id: row.get(6)?,
                    content_block_hash: row.get(7)?,
                    source_state: row.get(8)?,
                    original_content_hash: row.get(9)?,
                    byte_len: row.get::<_, i64>(10)? as u64,
                    created_at: row.get(11)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// До этапа 05.8 действует bounded cap 8 MiB. Старые shadow blocks
    /// переводятся в явный `metadata_hash_only`, никогда не исчезают молча.
    pub fn compact_shadow_for_task(&self, task_id: &str) -> Result<usize> {
        let total: i64 = self.connection.query_row("SELECT COALESCE(SUM(DISTINCT b.byte_len),0) FROM context_shadow_blocks b JOIN context_shadowed_originals s ON s.original_content_hash=b.content_hash JOIN context_ledger l ON l.id=s.ledger_id WHERE l.task_id=?1 AND s.source_state='full'", [task_id], |row| row.get(0))?;
        if total <= evohime_model_provenance::MAX_SHADOW_BYTES_PER_TASK as i64 {
            return Ok(0);
        }
        let mut remaining = total as usize;
        let mut statement = self.connection.prepare("SELECT s.shadow_id,s.original_content_hash,b.byte_len FROM context_shadowed_originals s JOIN context_shadow_blocks b ON b.content_hash=s.original_content_hash JOIN context_ledger l ON l.id=s.ledger_id WHERE l.task_id=?1 AND s.source_state='full' ORDER BY s.created_at,s.shadow_id")?;
        let candidates = statement
            .query_map([task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? as usize,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut changed = 0;
        for (shadow_id, hash, size) in candidates {
            if remaining <= evohime_model_provenance::MAX_SHADOW_BYTES_PER_TASK {
                break;
            }
            self.connection.execute("UPDATE context_shadowed_originals SET source_state='metadata_hash_only' WHERE shadow_id=?1 AND source_state='full'", [&shadow_id])?;
            self.connection.execute(
                "UPDATE context_shadow_blocks SET bytes=NULL WHERE content_hash=?1",
                [&hash],
            )?;
            remaining = remaining.saturating_sub(size);
            changed += 1;
        }
        Ok(changed)
    }

    pub fn recover_active(&self) -> Result<usize> {
        let ids: Vec<String> = {
            let mut statement = self.connection.prepare("SELECT request_id FROM model_requests WHERE status='active' AND completed_at IS NULL AND payload_mode='full'")?;
            let rows = statement
                .query_map([], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        let mut count = 0;
        for id in ids {
            let response: Option<(String, String, Option<i64>)> = self.connection.query_row("SELECT status,request_id,completed_at FROM model_responses WHERE request_id=(SELECT request_id FROM model_requests WHERE request_id=?1)", [&id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()?;
            let dispatch: Option<i64> = self
                .connection
                .query_row(
                    "SELECT dispatch_at FROM model_requests WHERE request_id=?1",
                    [&id],
                    |row| row.get(0),
                )
                .optional()?
                .flatten();
            let (status, completed) = match response {
                Some((status, _, at)) if status == "complete" && dispatch.is_some() => {
                    ("completed", at.or_else(now_ms))
                }
                Some((status, _, at)) if status == "complete" => {
                    ("unknown_outcome", at.or_else(now_ms))
                }
                Some((status, _, at)) if status == "failed" => ("failed", at.or_else(now_ms)),
                Some((status, _, at)) if status == "interrupted" => {
                    ("interrupted", at.or_else(now_ms))
                }
                None => (
                    if dispatch.is_some() {
                        "unknown_outcome"
                    } else {
                        "interrupted"
                    },
                    now_ms(),
                ),
                _ => ("unknown_outcome", now_ms()),
            };
            count += self.connection.execute("UPDATE model_requests SET status=?2, completed_at=?3 WHERE request_id=?1 AND status='active' AND completed_at IS NULL", params![id, status, completed])?;
        }
        Ok(count)
    }

    pub fn redact_request(&self, request_id: &str, state: &str) -> Result<()> {
        let tx = self.connection.unchecked_transaction()?;
        let disposition = if state == "redacted" {
            "hash_removed"
        } else {
            "digest_kept"
        };
        tx.execute("INSERT OR IGNORE INTO provenance_tombstones(tombstone_id,request_id,subject_kind,state,source_disposition,created_at) VALUES (?1,?2,'request_block',?3,?4,?5)", params![Uuid::now_v7().to_string(), request_id, state, disposition, now_ms()])?;
        tx.execute("INSERT OR IGNORE INTO provenance_tombstones(tombstone_id,request_id,subject_kind,subject_ordinal,subject_id,state,source_disposition,created_at) SELECT lower(hex(randomblob(16))),request_id,'source',ordinal,source_ref_id,?2,?3,?4 FROM model_request_sources WHERE request_id=?1", params![request_id, state, disposition, now_ms()])?;
        tx.execute("INSERT OR IGNORE INTO provenance_tombstones(tombstone_id,request_id,subject_kind,subject_id,state,source_disposition,created_at) SELECT lower(hex(randomblob(16))),request_id,'response_output',response_id,?2,?3,?4 FROM model_responses WHERE request_id=?1 AND output IS NOT NULL", params![request_id, state, disposition, now_ms()])?;
        tx.execute("INSERT OR IGNORE INTO provenance_tombstones(tombstone_id,request_id,subject_kind,subject_ordinal,subject_id,state,source_disposition,created_at) SELECT lower(hex(randomblob(16))),origin_request_id,'tool_args',ordinal,intent_id,?2,?3,?4 FROM tool_intents WHERE origin_request_id=?1", params![request_id, state, disposition, now_ms()])?;
        tx.execute("INSERT OR IGNORE INTO provenance_tombstones(tombstone_id,request_id,subject_kind,subject_id,state,source_disposition,created_at) SELECT lower(hex(randomblob(16))),request_id,'request_block',block_ref_id,?2,?3,?4 FROM model_request_block_refs WHERE request_id=?1", params![request_id, state, disposition, now_ms()])?;
        tx.execute("INSERT OR IGNORE INTO provenance_tombstones(tombstone_id,request_id,subject_kind,subject_id,state,source_disposition,created_at) SELECT lower(hex(randomblob(16))),request_id,'shadow_original',shadow_id,?2,?3,?4 FROM context_shadowed_originals WHERE request_id=?1", params![request_id, state, disposition, now_ms()])?;
        tx.execute("UPDATE model_request_sources SET source_hash=CASE WHEN ?2='redacted' THEN NULL ELSE source_hash END WHERE request_id=?1", params![request_id, state])?;
        tx.execute("UPDATE model_responses SET status='redacted', output=NULL, output_hash=NULL WHERE request_id=?1 AND output IS NOT NULL", [request_id])?;
        tx.execute(
            "UPDATE tool_intents SET state='redacted' WHERE origin_request_id=?1",
            [request_id],
        )?;
        tx.execute(
            "DELETE FROM model_request_block_refs WHERE request_id=?1",
            [request_id],
        )?;
        tx.execute("UPDATE model_request_blocks SET refcount=(SELECT COUNT(*) FROM model_request_block_refs refs WHERE refs.content_hash=model_request_blocks.content_hash)", [])?;
        tx.execute("UPDATE model_request_blocks SET bytes=NULL WHERE refcount=0 AND content_hash NOT IN (SELECT content_hash FROM model_request_block_refs)", [])?;
        tx.execute("UPDATE context_shadowed_originals SET source_state=?2 WHERE request_id=?1 AND source_state IN ('full','metadata_hash_only')", params![request_id, state])?;
        tx.execute("UPDATE context_shadow_blocks SET bytes=NULL WHERE content_hash IN (SELECT original_content_hash FROM context_shadowed_originals WHERE request_id=?1) AND NOT EXISTS (SELECT 1 FROM context_shadowed_originals other WHERE other.original_content_hash=context_shadow_blocks.content_hash AND other.source_state='full')", [request_id])?;
        tx.execute("UPDATE model_requests SET status=?2 WHERE request_id=?1 AND status NOT IN ('redacted','retention_pruned')", params![request_id, state])?;
        tx.commit()?;
        Ok(())
    }

    pub fn retention_pass(&self, cutoff: i64) -> Result<usize> {
        let ids: Vec<String> = {
            let mut statement = self.connection.prepare("SELECT request_id FROM model_requests WHERE status NOT IN ('redacted','retention_pruned') AND dispatch_at IS NOT NULL AND dispatch_at < ?1")?;
            let rows = statement
                .query_map([cutoff], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            rows
        };
        for id in &ids {
            self.redact_request(id, "retention_pruned")?;
        }
        Ok(ids.len())
    }

    /// Создаёт замкнутый bounded bundle в staging-каталоге и публикует его
    /// одной rename-операцией. Файлы JSONL всегда имеют завершающий LF.
    pub fn export_bundle(
        &self,
        request_id: &str,
        destination: &Path,
        signer: &dyn ProvenanceBundleSigner,
    ) -> Result<PathBuf> {
        if destination.exists() {
            return Err(ModelProvenanceError::CommitFailed(
                "export destination exists".into(),
            ));
        }
        let record = self
            .get(request_id)?
            .ok_or(ProvenanceError::SourceMissing)?;
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| ModelProvenanceError::CommitFailed(error.to_string()))?;
        let staging = parent.join(format!(".evohime-provenance-{}", Uuid::now_v7()));
        fs::create_dir_all(staging.join("model_requests/envelopes"))?;
        fs::create_dir_all(staging.join("model_requests/blocks"))?;
        for dir in [
            "receipt_records",
            "context_ledger",
            "request_snapshots",
            "model_responses",
            "model_responses/blocks",
            "tool_intents",
            "tool_intents/blocks",
            "context_evidence",
            "context_shadowed_originals",
            "context_shadow_source_refs",
            "context_shadow_blocks",
            "provenance_tombstones",
        ] {
            fs::create_dir_all(staging.join(dir))?;
        }
        let tombstone_ids: Vec<String> = self.connection
            .prepare("SELECT tombstone_id FROM provenance_tombstones WHERE request_id=?1 ORDER BY created_at,tombstone_id")?
            .query_map([request_id], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let request_json = serde_json::to_vec(&serde_json::json!({
            "request_id": record.request_id, "logical_request_id": record.logical_request_id,
            "attempt": record.attempt, "parent_request_id": record.parent_request_id,
            "previous_request_hash": record.previous_request_hash, "request_kind": record.request_kind,
            "ledger_id": record.ledger_id, "provider": record.provider, "model": record.model,
            "envelope_version": record.envelope_version, "payload_mode": record.payload_mode,
            "envelope_hash": record.envelope_hash, "context_projection_hash": record.context_projection_hash,
            "route_snapshot_hash": record.route_snapshot_hash, "policy_snapshot_hash": record.policy_snapshot_hash,
            "route_policy_hash_shared": record.route_policy_hash_shared, "status": record.status,
            "dispatch_at": record.dispatch_at, "completed_at": record.completed_at,
            "lifecycle_state": record.status, "tombstone_ids": tombstone_ids
        }))?;
        write_bytes(
            &staging.join("model_requests/requests.jsonl"),
            &line(&request_json),
        )?;
        write_bytes(
            &staging.join(format!("model_requests/envelopes/{request_id}.json")),
            &record.envelope_blob,
        )?;
        let mut block_refs = Vec::new();
        let mut blocks = Vec::new();
        let mut stmt = self.connection.prepare("SELECT block_ref_id,role,content_hash FROM model_request_block_refs WHERE request_id=?1 ORDER BY ordinal")?;
        for item in stmt.query_map([request_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            let (block_ref_id, role, content_hash) = item?;
            block_refs.push(serde_json::json!({"block_ref_id":block_ref_id,"role":role,"content_hash":content_hash}));
            let block: Option<(i64, Option<Vec<u8>>)> = self
                .connection
                .query_row(
                    "SELECT byte_len,bytes FROM model_request_blocks WHERE content_hash=?1",
                    [&content_hash],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((byte_len, bytes)) = block {
                if let Some(bytes) = bytes {
                    write_bytes(
                        &staging.join(format!("model_requests/blocks/{content_hash}.bin")),
                        &bytes,
                    )?;
                    blocks.push(serde_json::json!({"content_hash":content_hash,"byte_len":byte_len,"payload_mode":"full","file_path":format!("model_requests/blocks/{content_hash}.bin")}));
                } else {
                    blocks.push(serde_json::json!({"content_hash":content_hash,"byte_len":byte_len,"payload_mode":"hash_only"}));
                }
            }
        }
        write_jsonl(
            &staging.join("model_requests/block_refs.jsonl"),
            &block_refs,
        )?;
        write_jsonl(&staging.join("context_evidence/blocks.jsonl"), &blocks)?;
        let mut ledger_rows = Vec::new();
        let mut ledger_statement = self.connection.prepare("SELECT id,schema_version,task_id,session_id,model_call_id,created_at,provider,model,profile_version,profile_snapshot,tokenizer_version,normalizer_version,strategy_version,mandatory_tokens,selected_optional_tokens,reserves_tokens,estimated_prompt_tokens,selected_items,dropped_items,mandatory_parts,ladder_levels_applied,compression,loadout,fallback_estimator,replan_of,outcome,budget_unavailable,context_ledger_hash FROM context_ledger WHERE id=?1")?;
        for row in ledger_statement.query_map([&record.ledger_id], |row| Ok(serde_json::json!({
            "id": row.get::<_, String>(0)?, "schema_version": row.get::<_, i64>(1)?, "task_id": row.get::<_, String>(2)?, "session_id": row.get::<_, String>(3)?, "model_call_id": row.get::<_, String>(4)?, "created_at": row.get::<_, i64>(5)?, "provider": row.get::<_, String>(6)?, "model": row.get::<_, String>(7)?, "profile_version": row.get::<_, String>(8)?, "profile_snapshot": row.get::<_, String>(9)?, "tokenizer_version": row.get::<_, String>(10)?, "normalizer_version": row.get::<_, String>(11)?, "strategy_version": row.get::<_, String>(12)?, "mandatory_tokens": row.get::<_, i64>(13)?, "selected_optional_tokens": row.get::<_, i64>(14)?, "reserves_tokens": row.get::<_, i64>(15)?, "estimated_prompt_tokens": row.get::<_, i64>(16)?, "selected_items": row.get::<_, String>(17)?, "dropped_items": row.get::<_, String>(18)?, "mandatory_parts": row.get::<_, String>(19)?, "ladder_levels_applied": row.get::<_, String>(20)?, "compression": row.get::<_, String>(21)?, "loadout": row.get::<_, Option<String>>(22)?, "fallback_estimator": row.get::<_, i64>(23)?, "replan_of": row.get::<_, Option<String>>(24)?, "outcome": row.get::<_, String>(25)?, "budget_unavailable": row.get::<_, Option<String>>(26)?, "context_ledger_hash": row.get::<_, String>(27)?
        })))? { ledger_rows.push(row?); }
        write_jsonl(&staging.join("context_ledger/entries.jsonl"), &ledger_rows)?;
        let mut route_policy = Vec::new();
        route_policy.push(serde_json::json!({"request_id": record.request_id, "route_snapshot_hash": record.route_snapshot_hash, "policy_snapshot_hash": record.policy_snapshot_hash, "route_policy_hash_shared": record.route_policy_hash_shared, "payload_mode": record.payload_mode}));
        write_jsonl(
            &staging.join("request_snapshots/route_policy.jsonl"),
            &route_policy,
        )?;
        let mut responses = Vec::new();
        let mut response_statement = self.connection.prepare("SELECT response_id,status,output,output_hash,finish_reason,started_at,completed_at FROM model_responses WHERE request_id=?1 ORDER BY response_id")?;
        for row in response_statement.query_map([request_id], |row| {
            let output: Option<String> = row.get(2)?;
            let output_hash: Option<String> = row.get(3)?;
            Ok((serde_json::json!({"response_id":row.get::<_,String>(0)?,"status":row.get::<_,String>(1)?,"output_hash":output_hash,"finish_reason":row.get::<_,Option<String>>(4)?,"started_at":row.get::<_,i64>(5)?,"completed_at":row.get::<_,Option<i64>>(6)?,"payload_mode":if output.is_some() {"full"} else {"hash_only"},"output_block":output_hash.as_ref().map(|hash| serde_json::json!({"content_hash":hash,"byte_len":output.as_ref().map_or(0, String::len),"file_path":format!("model_responses/blocks/{hash}.bin")}))}), output, output_hash))
        })? {
            let (value, output, output_hash) = row?;
            if let (Some(output), Some(hash)) = (output, output_hash) { write_bytes(&staging.join(format!("model_responses/blocks/{hash}.bin")), output.as_bytes())?; }
            responses.push(value);
        }
        write_jsonl(&staging.join("model_responses/responses.jsonl"), &responses)?;
        let mut intents = Vec::new();
        let mut intent_statement = self.connection.prepare("SELECT intent_id,origin_request_id,origin_request_envelope_hash,response_id,ordinal,tool_name,tool_args_hash,state FROM tool_intents WHERE origin_request_id=?1 ORDER BY ordinal")?;
        for row in intent_statement.query_map([request_id], |row| Ok(serde_json::json!({"intent_id":row.get::<_,String>(0)?,"origin_request_id":row.get::<_,String>(1)?,"origin_request_envelope_hash":row.get::<_,String>(2)?,"response_id":row.get::<_,Option<String>>(3)?,"ordinal":row.get::<_,i64>(4)?,"tool_name":row.get::<_,String>(5)?,"arguments_hash":row.get::<_,String>(6)?,"state":row.get::<_,String>(7)?,"payload_mode":"hash_only"})))? { intents.push(row?); }
        write_jsonl(&staging.join("tool_intents/intents.jsonl"), &intents)?;
        let mut links = Vec::new();
        let mut link_statement = self.connection.prepare("SELECT l.intent_id,l.action_id,l.terminal_receipt_hash,l.linked_at FROM tool_intent_receipt_links l JOIN tool_intents i ON i.intent_id=l.intent_id WHERE i.origin_request_id=?1 ORDER BY l.linked_at,l.intent_id")?;
        for row in link_statement.query_map([request_id], |row| Ok(serde_json::json!({"intent_id":row.get::<_,String>(0)?,"action_id":row.get::<_,String>(1)?,"terminal_receipt_hash":row.get::<_,String>(2)?,"linked_at":row.get::<_,i64>(3)?})))? { links.push(row?); }
        write_jsonl(&staging.join("tool_intents/receipt_links.jsonl"), &links)?;
        let mut sources = Vec::new();
        let mut source_statement = self.connection.prepare("SELECT ordinal,source_ref_id,source_kind,source_id,source_version,source_hash FROM model_request_sources WHERE request_id=?1 ORDER BY ordinal")?;
        for row in source_statement.query_map([request_id], |row| Ok(serde_json::json!({"ordinal":row.get::<_,i64>(0)?,"source_ref_id":row.get::<_,String>(1)?,"source_kind":row.get::<_,String>(2)?,"source_id":row.get::<_,String>(3)?,"source_version":row.get::<_,Option<String>>(4)?,"source_hash":row.get::<_,Option<String>>(5)?})))? { sources.push(row?); }
        write_jsonl(&staging.join("context_evidence/sources.jsonl"), &sources)?;
        let mut receipts = Vec::new();
        let mut receipt_statement = self.connection.prepare("SELECT receipt_id,receipt_kind,receipt_hash,previous_receipt_hash,key_id,created_at_ms,canonical_payload,canonical_envelope FROM receipt_records WHERE request_id=?1 OR action_id=?1 ORDER BY rowid")?;
        for row in receipt_statement.query_map([request_id], |row| {
            let payload: Vec<u8> = row.get(6)?;
            let envelope: Vec<u8> = row.get(7)?;
            Ok(serde_json::json!({"receipt_id":row.get::<_,String>(0)?,"receipt_kind":row.get::<_,String>(1)?,"receipt_hash":row.get::<_,String>(2)?,"previous_receipt_hash":row.get::<_,Option<String>>(3)?,"key_id":row.get::<_,String>(4)?,"created_at_ms":row.get::<_,i64>(5)?,"canonical_payload":String::from_utf8_lossy(&payload),"canonical_envelope":String::from_utf8_lossy(&envelope)}))
        })? { receipts.push(row?); }
        write_jsonl(&staging.join("receipt_records/records.jsonl"), &receipts)?;
        let shadows = self
            .list_shadow_originals(request_id, 4096)?
            .into_iter()
            .map(serde_json::to_value)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        write_jsonl(
            &staging.join("context_shadowed_originals/records.jsonl"),
            &shadows,
        )?;
        let mut shadow_refs = Vec::new();
        let mut shadow_ref_statement = self.connection.prepare("SELECT shadow_id,request_id,source_ref_ordinal,source_ordinal FROM context_shadow_source_refs WHERE request_id=?1 ORDER BY shadow_id,source_ref_ordinal")?;
        for row in shadow_ref_statement.query_map([request_id], |row| Ok(serde_json::json!({"shadow_id":row.get::<_,String>(0)?,"request_id":row.get::<_,String>(1)?,"source_ref_ordinal":row.get::<_,i64>(2)?,"source_ordinal":row.get::<_,i64>(3)?})))? { shadow_refs.push(row?); }
        write_jsonl(
            &staging.join("context_shadow_source_refs/refs.jsonl"),
            &shadow_refs,
        )?;
        let mut shadow_blocks = Vec::new();
        let mut shadow_block_statement = self.connection.prepare("SELECT b.content_hash,b.byte_len,b.bytes FROM context_shadow_blocks b JOIN context_shadowed_originals s ON s.original_content_hash=b.content_hash WHERE s.request_id=?1 GROUP BY b.content_hash ORDER BY b.content_hash")?;
        for row in shadow_block_statement.query_map([request_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<Vec<u8>>>(2)?,
            ))
        })? {
            let (hash, byte_len, bytes) = row?;
            if let Some(bytes) = bytes {
                write_bytes(
                    &staging.join(format!("context_shadow_blocks/blocks/{hash}.bin")),
                    &bytes,
                )?;
                shadow_blocks.push(serde_json::json!({"content_hash":hash,"byte_len":byte_len,"payload_mode":"full","file_path":format!("context_shadow_blocks/blocks/{hash}.bin")}));
            } else {
                shadow_blocks.push(serde_json::json!({"content_hash":hash,"byte_len":byte_len,"payload_mode":"metadata_hash_only"}));
            }
        }
        write_jsonl(
            &staging.join("context_shadow_blocks/blocks.jsonl"),
            &shadow_blocks,
        )?;
        let mut tombstones = Vec::new();
        let mut tombstone_statement = self.connection.prepare("SELECT tombstone_id,request_id,subject_kind,subject_ordinal,subject_id,state,source_disposition,marker_version,created_at FROM provenance_tombstones WHERE request_id=?1 ORDER BY created_at,tombstone_id")?;
        for row in tombstone_statement.query_map([request_id], |row| Ok(serde_json::json!({"tombstone_id":row.get::<_,String>(0)?,"request_id":row.get::<_,String>(1)?,"subject_kind":row.get::<_,String>(2)?,"subject_ordinal":row.get::<_,Option<i64>>(3)?,"subject_id":row.get::<_,Option<String>>(4)?,"state":row.get::<_,String>(5)?,"source_disposition":row.get::<_,String>(6)?,"marker_version":row.get::<_,i64>(7)?,"created_at":row.get::<_,i64>(8)?})))? { tombstones.push(row?); }
        write_jsonl(
            &staging.join("provenance_tombstones/tombstones.jsonl"),
            &tombstones,
        )?;
        write_bytes(
            &staging.join("key-history.jsonl"),
            &signer.key_history_jsonl()?,
        )?;
        write_bytes(
            &staging.join("checkpoints.jsonl"),
            &signer.checkpoints_jsonl()?,
        )?;
        let files = collect_files(&staging)?;
        let file_sizes = files
            .keys()
            .map(|path| {
                (
                    path.clone(),
                    serde_json::json!(fs::metadata(staging.join(path))
                        .map(|metadata| metadata.len())
                        .unwrap_or_default()),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let bundle_content_sha256 = bundle_digest(&files);
        let verification_state = match record.status.as_str() {
            "redacted" => "redacted",
            "retention_pruned" => "retention_pruned",
            _ if record.payload_mode == "full" => "valid",
            _ => "legacy_hash_only",
        };
        let manifest = serde_json::json!({"export_id":Uuid::now_v7().to_string(),"created_at":now_ms().unwrap_or_default(),"bundle_schema_version":1,"schema_versions":{"model_request":1,"storage":2},"selection":{"request_id":request_id},"request_count":1,"receipt_count":receipts.len(),"chain_roots":[],"chain_checkpoints":[],"request_states":[{"request_id":request_id,"payload_mode":record.payload_mode,"status":record.status,"verification_state":verification_state,"tombstone_ids":tombstones.iter().filter_map(|row| row.get("tombstone_id").cloned()).collect::<Vec<_>>(),"missing_or_pruned_subjects":[]}],"files":files,"file_sizes":file_sizes,"bundle_content_sha256":bundle_content_sha256,"signer":{"key_id":signer.key_id(),"algorithm":"Ed25519","public_key_hex":signer.public_key_hex(),"signature_path":"bundle.sig"}});
        let manifest_bytes =
            evohime_receipts::canonicalize_json(&serde_json::to_vec(&manifest)?)
                .map_err(|error| ModelProvenanceError::CommitFailed(error.to_string()))?;
        write_bytes(&staging.join("manifest.json"), &manifest_bytes)?;
        let digest = Sha256::digest(
            [
                b"evohime-provenance-manifest-v1\0".as_slice(),
                manifest_bytes.as_slice(),
            ]
            .concat(),
        );
        let signature = signer.sign_manifest_digest(&digest)?;
        write_bytes(&staging.join("bundle.sig"), &signature)?;
        fs::rename(&staging, destination)
            .map_err(|error| ModelProvenanceError::CommitFailed(error.to_string()))?;
        Ok(destination.to_path_buf())
    }

    pub fn verify_bundle(bundle: &Path) -> Result<BundleVerification> {
        let manifest_path = bundle.join("manifest.json");
        let manifest_bytes = fs::read(&manifest_path)?;
        let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
        let request_id = manifest["selection"]["request_id"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let mut errors = Vec::new();
        let files = manifest["files"]
            .as_object()
            .ok_or_else(|| ModelProvenanceError::CommitFailed("manifest files missing".into()))?;
        let mut map = BTreeMap::new();
        for (path, expected) in files {
            let safe = Path::new(path);
            if safe.is_absolute()
                || safe
                    .components()
                    .any(|component| matches!(component, std::path::Component::ParentDir))
            {
                errors.push("EXPORT_MANIFEST_MISMATCH".into());
                continue;
            }
            let bytes = fs::read(bundle.join(safe))?;
            let actual = hash_bytes_bytes(&bytes);
            if expected.as_str() != Some(actual.as_str()) {
                errors.push("EXPORT_MANIFEST_MISMATCH".into());
            }
            map.insert(path.clone(), actual);
        }
        if manifest["bundle_content_sha256"].as_str() != Some(bundle_digest(&map).as_str()) {
            errors.push("EXPORT_MANIFEST_MISMATCH".into());
        }
        let public_key = manifest["signer"]["public_key_hex"]
            .as_str()
            .and_then(|value| hex::decode(value).ok());
        let signature = fs::read(bundle.join("bundle.sig")).ok();
        let digest = Sha256::digest(
            [
                b"evohime-provenance-manifest-v1\0".as_slice(),
                manifest_bytes.as_slice(),
            ]
            .concat(),
        );
        if public_key.is_none()
            || signature.is_none()
            || evohime_receipts::verify_ed25519_digest(
                &digest,
                signature.as_deref().unwrap_or_default(),
                public_key.as_deref().unwrap_or_default(),
            )
            .is_err()
        {
            errors.push("EXPORT_SIGNATURE_INVALID".into());
        }
        let state = manifest["request_states"]
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item["verification_state"].as_str())
            .unwrap_or("damaged");
        Ok(BundleVerification {
            valid: errors.is_empty(),
            request_id,
            verification_state: state.into(),
            errors,
        })
    }
}

fn storage_envelope_bytes(envelope: &ModelRequestEnvelopeV1) -> Result<Vec<u8>> {
    let mut value = serde_json::to_value(envelope)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ModelProvenanceError::CommitFailed("envelope is not an object".into()))?;
    object.insert(
        "storage_mode".into(),
        Value::String("opaque_block_refs".into()),
    );
    object.insert(
        "system_prompt".into(),
        serde_json::json!({"block_ref_id": format!("{}-0", envelope.request_id)}),
    );
    object.insert(
        "messages".into(),
        Value::Array(
            envelope
                .messages
                .iter()
                .enumerate()
                .map(|(index, message)| {
                    serde_json::json!({
                        "role": message.role,
                        "block_ref_id": format!("{}-{}", envelope.request_id, index + 1),
                    })
                })
                .collect(),
        ),
    );
    let tool_offset = envelope.messages.len() + 1;
    object.insert(
        "tools".into(),
        Value::Array(
            envelope
                .tools
                .iter()
                .enumerate()
                .map(|(index, tool)| {
                    serde_json::json!({
                        "name": tool.name,
                        "block_ref_id": format!("{}-{}", envelope.request_id, tool_offset + index),
                    })
                })
                .collect(),
        ),
    );
    canonicalize_json(&serde_json::to_vec(&value)?)
        .map_err(|error| ModelProvenanceError::CommitFailed(error.to_string()))
}

fn commit_tx(
    tx: &Transaction<'_>,
    envelope: &ModelRequestEnvelopeV1,
    bytes: &[u8],
    envelope_hash: Option<&str>,
    mode: CommitMode,
) -> Result<ModelRequestRecord> {
    let existing: Option<ModelRequestRecord> = tx.query_row("SELECT request_id,logical_request_id,attempt,parent_request_id,previous_request_hash,request_kind,ledger_id,provider,model,envelope_version,payload_mode,envelope_hash,envelope_blob,context_projection_hash,route_snapshot_hash,policy_snapshot_hash,route_policy_hash_shared,status,dispatch_at,completed_at FROM model_requests WHERE request_id=?1", [&envelope.request_id], row_record).optional()?;
    if let Some(record) = existing {
        if record.envelope_blob == bytes && record.envelope_hash.as_deref() == envelope_hash {
            return Ok(record);
        }
        return Err(ModelProvenanceError::CommitFailed(
            "request id conflict".into(),
        ));
    }
    if envelope.attempt > 1 {
        let predecessor = envelope
            .parent_request_id
            .as_deref()
            .ok_or(ProvenanceError::LineageMismatch)?;
        let prior: Option<(String, String, i64, String)> = tx.query_row("SELECT logical_request_id,ledger_id,attempt,envelope_hash FROM model_requests WHERE request_id=?1", [predecessor], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).optional()?;
        let Some((logical, ledger, attempt, hash)) = prior else {
            return Err(ProvenanceError::LineageMismatch.into());
        };
        if logical != envelope.logical_request_id
            || ledger != envelope.ledger_id
            || attempt + 1 != i64::from(envelope.attempt)
            || Some(hash) != envelope.previous_request_hash.clone()
        {
            return Err(ProvenanceError::LineageMismatch.into());
        }
    }
    let payload_mode = match mode {
        CommitMode::FullForDispatch => "full",
        CommitMode::HashOnlyStorage => "hash_only",
    };
    tx.execute("INSERT INTO model_requests(request_id,logical_request_id,attempt,parent_request_id,previous_request_hash,request_kind,ledger_id,provider,model,envelope_version,payload_mode,envelope_hash,envelope_blob,context_projection_hash,route_snapshot_hash,policy_snapshot_hash,route_policy_hash_shared,status) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,'active')", params![envelope.request_id, envelope.logical_request_id, envelope.attempt, envelope.parent_request_id, envelope.previous_request_hash, serde_json::to_string(&envelope.request_kind)?.trim_matches('"'), envelope.ledger_id, envelope.provider, envelope.model, envelope.version, payload_mode, envelope_hash, bytes, envelope.context_projection.context_projection_hash, envelope.route_snapshot_hash, envelope.policy_snapshot_hash, envelope.route_policy_hash_shared])?;
    let mut source_ordinal = 0usize;
    for entry in &envelope.context_projection.entries {
        for source in &entry.source_refs {
            tx.execute("INSERT INTO model_request_sources(request_id,ordinal,source_ref_id,source_kind,source_id,source_version) VALUES (?1,?2,?3,?4,?5,?6)", params![envelope.request_id, source_ordinal as i64, source.source_ref_id, source.source_kind, source.source_id, source.source_version])?;
            source_ordinal += 1;
        }
    }
    for (ordinal, role, content) in
        std::iter::once((0usize, "system_prompt", envelope.system_prompt.as_str())).chain(
            envelope
                .messages
                .iter()
                .enumerate()
                .map(|(i, m)| (i + 1, "message", m.content.as_str())),
        )
    {
        let hash = hash_bytes(content);
        tx.execute("INSERT INTO model_request_blocks(content_hash,byte_len,bytes,refcount,last_referenced_at) VALUES (?1,?2,?3,0,?4) ON CONFLICT(content_hash) DO UPDATE SET last_referenced_at=excluded.last_referenced_at", params![hash, content.len() as i64, if matches!(mode, CommitMode::FullForDispatch) { Some(content.as_bytes()) } else { None::<&[u8]> }, now_ms()])?;
        tx.execute("INSERT INTO model_request_block_refs(request_id,ordinal,role,block_ref_id,content_hash) VALUES (?1,?2,?3,?4,?5)", params![envelope.request_id, ordinal as i64, role, format!("{}-{}", envelope.request_id, ordinal), hash])?;
        tx.execute("UPDATE model_request_blocks SET refcount=refcount+1,last_referenced_at=?2 WHERE content_hash=?1", params![hash, now_ms()])?;
    }
    let tool_offset = envelope.messages.len() + 1;
    for (index, tool) in envelope.tools.iter().enumerate() {
        let content = serde_json::to_vec(tool)?;
        let hash = hash_bytes_bytes(&content);
        tx.execute("INSERT INTO model_request_blocks(content_hash,byte_len,bytes,refcount,last_referenced_at) VALUES (?1,?2,?3,0,?4) ON CONFLICT(content_hash) DO UPDATE SET last_referenced_at=excluded.last_referenced_at", params![hash, content.len() as i64, if matches!(mode, CommitMode::FullForDispatch) { Some(content.as_slice()) } else { None::<&[u8]> }, now_ms()])?;
        tx.execute("INSERT INTO model_request_block_refs(request_id,ordinal,role,block_ref_id,content_hash) VALUES (?1,?2,?3,?4,?5)", params![envelope.request_id, (tool_offset + index) as i64, "tool_schema", format!("{}-{}", envelope.request_id, tool_offset + index), hash])?;
        tx.execute("UPDATE model_request_blocks SET refcount=refcount+1,last_referenced_at=?2 WHERE content_hash=?1", params![hash, now_ms()])?;
    }
    Ok(ModelRequestRecord {
        request_id: envelope.request_id.clone(),
        logical_request_id: envelope.logical_request_id.clone(),
        attempt: envelope.attempt,
        parent_request_id: envelope.parent_request_id.clone(),
        previous_request_hash: envelope.previous_request_hash.clone(),
        request_kind: serde_json::to_string(&envelope.request_kind)?
            .trim_matches('"')
            .into(),
        ledger_id: envelope.ledger_id.clone(),
        provider: envelope.provider.clone(),
        model: envelope.model.clone(),
        envelope_version: envelope.version,
        payload_mode: payload_mode.into(),
        envelope_hash: envelope_hash.map(str::to_owned),
        envelope_blob: bytes.to_vec(),
        context_projection_hash: envelope.context_projection.context_projection_hash.clone(),
        route_snapshot_hash: envelope.route_snapshot_hash.clone(),
        policy_snapshot_hash: envelope.policy_snapshot_hash.clone(),
        route_policy_hash_shared: envelope.route_policy_hash_shared,
        status: "active".into(),
        dispatch_at: None,
        completed_at: None,
    })
}

fn row_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelRequestRecord> {
    Ok(ModelRequestRecord {
        request_id: row.get(0)?,
        logical_request_id: row.get(1)?,
        attempt: row.get(2)?,
        parent_request_id: row.get(3)?,
        previous_request_hash: row.get(4)?,
        request_kind: row.get(5)?,
        ledger_id: row.get(6)?,
        provider: row.get(7)?,
        model: row.get(8)?,
        envelope_version: row.get(9)?,
        payload_mode: row.get(10)?,
        envelope_hash: row.get(11)?,
        envelope_blob: row.get(12)?,
        context_projection_hash: row.get(13)?,
        route_snapshot_hash: row.get(14)?,
        policy_snapshot_hash: row.get(15)?,
        route_policy_hash_shared: row.get::<_, i64>(16)? != 0,
        status: row.get(17)?,
        dispatch_at: row.get(18)?,
        completed_at: row.get(19)?,
    })
}

fn hash_bytes(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn hash_bytes_bytes(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn line(value: &[u8]) -> Vec<u8> {
    let mut bytes = value.to_vec();
    bytes.push(b'\n');
    bytes
}
fn write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
fn write_jsonl(path: &Path, values: &[Value]) -> std::io::Result<()> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend(line(&serde_json::to_vec(value).unwrap_or_default()));
    }
    write_bytes(path, &bytes)
}
fn collect_files(root: &Path) -> Result<BTreeMap<String, String>> {
    fn walk(
        root: &Path,
        current: &Path,
        map: &mut BTreeMap<String, String>,
    ) -> std::io::Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, map)?;
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if relative == "manifest.json" || relative == "bundle.sig" {
                    continue;
                }
                map.insert(relative, hash_bytes_bytes(&fs::read(path)?));
            }
        }
        Ok(())
    }
    let mut map = BTreeMap::new();
    walk(root, root, &mut map)
        .map_err(|error| ModelProvenanceError::CommitFailed(error.to_string()))?;
    Ok(map)
}
fn bundle_digest(files: &BTreeMap<String, String>) -> String {
    let mut bytes = b"evohime-provenance-bundle-v1\0".to_vec();
    for (path, hash) in files {
        bytes.extend(path.as_bytes());
        bytes.push(0);
        bytes.extend(hash.as_bytes());
        bytes.push(b'\n');
    }
    hash_bytes_bytes(&bytes)
}
fn now_ms() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_provenance::{
        ContextProjection, ModelMessage, ModelParameters, ProjectionEntry, RequestKind, ToolSchema,
    };

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE context_ledger(id TEXT PRIMARY KEY, context_ledger_hash TEXT NOT NULL)",
        )
        .unwrap();
        install_schema(&db).unwrap();
        db
    }
    fn envelope() -> ModelRequestEnvelopeV1 {
        let mut projection = ContextProjection {
            ledger_id: "l".into(),
            context_ledger_hash: "a".repeat(64),
            entries: vec![ProjectionEntry {
                projection_entry_id: "m".into(),
                operation: "include".into(),
                source_refs: vec![],
                block_ref_id: Some("b".into()),
                drop_reason: None,
            }],
            context_projection_hash: String::new(),
        };
        projection.context_projection_hash = projection.compute_hash().unwrap();
        ModelRequestEnvelopeV1 {
            version: 1,
            request_id: Uuid::now_v7().to_string(),
            logical_request_id: "logical".into(),
            attempt: 1,
            parent_request_id: None,
            ledger_id: "l".into(),
            request_kind: RequestKind::Agent,
            provider: "mock".into(),
            model: "m".into(),
            route_snapshot_hash: "b".repeat(64),
            policy_snapshot_hash: "c".repeat(64),
            route_policy_hash_shared: false,
            system_prompt: "system".into(),
            messages: vec![ModelMessage {
                role: "user".into(),
                content: "hello".into(),
            }],
            tools: vec![ToolSchema {
                name: "tool".into(),
                description: "tool".into(),
                input_schema: serde_json::json!({"type":"object"}),
            }],
            model_parameters: ModelParameters {
                temperature: None,
                top_p: None,
                max_output_tokens: Some(10),
                reasoning_mode: None,
                provider_options: Default::default(),
            },
            context_projection: projection,
            previous_request_hash: None,
        }
    }

    #[test]
    fn commit_is_idempotent_and_deduplicates_blocks() {
        let db = db();
        db.execute(
            "INSERT INTO context_ledger VALUES('l',?1)",
            ["a".repeat(64)],
        )
        .unwrap();
        let repo = ModelProvenanceRepository::new(&db);
        let one = envelope();
        let first = repo
            .commit_envelope(&one, CommitMode::FullForDispatch)
            .unwrap();
        let second = repo
            .commit_envelope(&one, CommitMode::FullForDispatch)
            .unwrap();
        assert_eq!(first, second);
        assert!(!String::from_utf8_lossy(&first.envelope_blob).contains("hello"));
        assert!(!String::from_utf8_lossy(&first.envelope_blob).contains("\\\"content\\\""));
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM model_request_blocks", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    #[test]
    fn failed_lineage_does_not_leave_rows() {
        let db = db();
        db.execute(
            "INSERT INTO context_ledger VALUES('l',?1)",
            ["a".repeat(64)],
        )
        .unwrap();
        let repo = ModelProvenanceRepository::new(&db);
        let mut one = envelope();
        one.attempt = 2;
        one.parent_request_id = Some("missing".into());
        one.previous_request_hash = Some("d".repeat(64));
        assert!(repo
            .commit_envelope(&one, CommitMode::FullForDispatch)
            .is_err());
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM model_requests", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
