use crate::runtime_contract::RuntimeError;
use rusqlite::{params, Connection, OptionalExtension};

/// Installs or upgrades the receipt runtime tables and their integrity constraints.
pub fn install_schema(connection: &Connection) -> Result<(), RuntimeError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS receipt_records (
           schema_version INTEGER NOT NULL DEFAULT 1,
           receipt_id TEXT PRIMARY KEY NOT NULL,
           action_id TEXT,
           request_id TEXT,
           receipt_kind TEXT NOT NULL CHECK(receipt_kind IN ('pre_action','post_action','refusal','request_commit')),
           action_status TEXT NOT NULL,
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           key_id TEXT NOT NULL,
           canonical_payload BLOB NOT NULL,
           canonical_envelope BLOB NOT NULL,
           receipt_hash TEXT NOT NULL UNIQUE,
           previous_receipt_hash TEXT,
           created_at_ms INTEGER NOT NULL,
           source TEXT NOT NULL DEFAULT 'signed'
         );
         CREATE INDEX IF NOT EXISTS idx_receipt_records_action ON receipt_records(action_id);
         CREATE TABLE IF NOT EXISTS receipt_actions (
           schema_version INTEGER NOT NULL DEFAULT 1,
           action_id TEXT PRIMARY KEY NOT NULL,
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           tool_name TEXT NOT NULL,
           normalized_scope TEXT NOT NULL DEFAULT '',
           fingerprint_input_version INTEGER NOT NULL DEFAULT 1,
           tool_args_hash TEXT NOT NULL,
           policy_id TEXT NOT NULL,
           policy_decision TEXT NOT NULL CHECK(policy_decision IN ('allow','deny','approval_required')),
           state TEXT NOT NULL CHECK(state IN ('awaiting_approval','prepared','refused','succeeded','failed','cancelled','pending_recovery','quarantined')),
           dispatch_state TEXT NOT NULL CHECK(dispatch_state IN ('not_started','started','returned')),
           approval_id TEXT,
           approval_call_hash TEXT,
           parent_approval_ref TEXT,
           legacy_approval_ref TEXT,
           pre_receipt_hash TEXT,
           terminal_receipt_hash TEXT,
           recovery_code TEXT,
           result_hash TEXT,
           result_marker BLOB,
           reconciliation_action_id TEXT,
           reconciles_action_id TEXT,
           completion_source TEXT NOT NULL DEFAULT 'execution' CHECK(completion_source IN ('execution','reconciliation')),
           tool_started_at_ms INTEGER,
           UNIQUE(action_id)
         );
         CREATE TABLE IF NOT EXISTS receipt_approval_intents (
           schema_version INTEGER NOT NULL DEFAULT 1,
           approval_id TEXT PRIMARY KEY NOT NULL,
           action_id TEXT NOT NULL UNIQUE REFERENCES receipt_actions(action_id),
           task_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           tool_name TEXT NOT NULL,
           normalized_scope TEXT NOT NULL,
           call_hash TEXT NOT NULL,
           preview TEXT NOT NULL,
           state TEXT NOT NULL CHECK(state IN ('pending','granted','denied','expired','claimed','lost')),
           created_wall_at_ms INTEGER NOT NULL,
           expires_at_ms INTEGER NOT NULL,
           clock_boot_id TEXT NOT NULL DEFAULT 'runtime-boot-v1',
           created_monotonic_ms INTEGER NOT NULL,
           deadline_monotonic_ms INTEGER NOT NULL,
           legacy_approval_ref TEXT
         );
         CREATE TABLE IF NOT EXISTS receipt_capability_snapshots (
           snapshot_hash TEXT PRIMARY KEY NOT NULL,
           snapshot_id TEXT NOT NULL,
           run_id TEXT NOT NULL,
           session_id TEXT NOT NULL,
           task_id TEXT NOT NULL,
           policy_id TEXT NOT NULL,
           policy_version INTEGER NOT NULL,
           canonical_snapshot BLOB NOT NULL,
           redacted_summary BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_policy_decisions (
           action_id TEXT PRIMARY KEY NOT NULL REFERENCES receipt_actions(action_id),
           snapshot_hash TEXT,
           outcome TEXT NOT NULL CHECK(outcome IN ('allowed','approval_required','denied','unavailable','expired','cancelled','policy_error','unknown_outcome')),
           reason_code TEXT NOT NULL,
           retryable INTEGER NOT NULL CHECK(retryable IN (0,1)),
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_chain_heads (
           key_id TEXT PRIMARY KEY NOT NULL,
           receipt_hash TEXT NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_guard (
           id INTEGER PRIMARY KEY CHECK(id=1),
           phase TEXT NOT NULL CHECK(phase IN ('recovery_in_progress','ready','read_only_recovery')),
           generation INTEGER NOT NULL,
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_protected_actions (
           action_id TEXT PRIMARY KEY NOT NULL REFERENCES receipt_actions(action_id),
           key_id TEXT NOT NULL,
           envelope BLOB NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_config (
           id INTEGER PRIMARY KEY CHECK(id=1),
           audit_sampling_rate INTEGER NOT NULL CHECK(audit_sampling_rate BETWEEN 0 AND 100),
           sampling_policy_version INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_audit_markers (
           action_id TEXT PRIMARY KEY NOT NULL,
           tool_name TEXT NOT NULL,
           call_hash TEXT NOT NULL,
           sampled INTEGER NOT NULL CHECK(sampled=0),
           sampling_policy_version INTEGER NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_sampling_changes (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           old_rate INTEGER NOT NULL,
           new_rate INTEGER NOT NULL,
           sampling_policy_version INTEGER NOT NULL,
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_metrics (
           metric TEXT PRIMARY KEY NOT NULL,
           value INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_runtime_diagnostics (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           code TEXT NOT NULL,
           action_id TEXT NOT NULL,
           detail_code TEXT NOT NULL,
           created_at_ms INTEGER NOT NULL,
           UNIQUE(code,action_id,detail_code)
         );
         CREATE TABLE IF NOT EXISTS receipt_storage_rotation (
           id INTEGER PRIMARY KEY CHECK(id=1),
           job_id TEXT NOT NULL,
           old_key_id TEXT NOT NULL,
           new_key_id TEXT NOT NULL,
           cursor TEXT NOT NULL DEFAULT '',
           generation INTEGER NOT NULL,
           state TEXT NOT NULL CHECK(state IN ('running','completed','failed')),
           updated_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_storage_rotation_audit (
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           job_id TEXT NOT NULL,
           cursor TEXT NOT NULL,
           old_key_id TEXT NOT NULL,
           new_key_id TEXT NOT NULL,
           processed_count INTEGER NOT NULL,
           outcome TEXT NOT NULL CHECK(outcome IN ('batch_committed','completed','failed')),
           created_at_ms INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS receipt_checkpoints (
           checkpoint_id TEXT PRIMARY KEY NOT NULL,
           key_id TEXT NOT NULL,
           cutoff_sequence INTEGER NOT NULL,
           first_retained_hash TEXT NOT NULL,
           prefix_last_hash TEXT NOT NULL,
           last_deleted_receipt_hash TEXT NOT NULL,
           head_receipt_hash TEXT NOT NULL,
           created_at TEXT NOT NULL,
           canonical_checkpoint BLOB NOT NULL,
           signed_by_key_id TEXT NOT NULL,
           signature TEXT NOT NULL,
           status TEXT NOT NULL CHECK(status IN ('active','superseded'))
         );
         CREATE INDEX IF NOT EXISTS idx_receipt_checkpoints_key ON receipt_checkpoints(key_id, cutoff_sequence);
         INSERT OR IGNORE INTO receipt_runtime_guard(id,phase,generation,updated_at_ms)
           VALUES(1,'ready',0,0);",
    )?;
    let marker_column: Option<String> = connection
        .query_row(
            "SELECT name FROM pragma_table_info('receipt_actions') WHERE name='result_marker'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if marker_column.is_none() {
        connection.execute(
            "ALTER TABLE receipt_actions ADD COLUMN result_marker BLOB",
            [],
        )?;
    }
    let boot_column: Option<String> = connection.query_row("SELECT name FROM pragma_table_info('receipt_approval_intents') WHERE name='clock_boot_id'", [], |row| row.get(0)).optional()?;
    if boot_column.is_none() {
        connection.execute("ALTER TABLE receipt_approval_intents ADD COLUMN clock_boot_id TEXT NOT NULL DEFAULT 'legacy'", [])?;
    }
    connection.execute("INSERT OR IGNORE INTO receipt_runtime_config(id,audit_sampling_rate,sampling_policy_version) VALUES(1,?1,?2)", params![crate::DEFAULT_READ_ONLY_SAMPLING_RATE as i64, crate::SAMPLING_POLICY_VERSION as i64])?;
    for (table, column, definition) in [
        (
            "receipt_records",
            "schema_version",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        ("receipt_records", "request_id", "TEXT"),
        (
            "receipt_actions",
            "schema_version",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        (
            "receipt_actions",
            "normalized_scope",
            "TEXT NOT NULL DEFAULT ''",
        ),
        (
            "receipt_actions",
            "fingerprint_input_version",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        (
            "receipt_approval_intents",
            "schema_version",
            "INTEGER NOT NULL DEFAULT 1",
        ),
        ("receipt_approval_intents", "legacy_approval_ref", "TEXT"),
        ("receipt_actions", "reconciliation_action_id", "TEXT"),
        ("receipt_actions", "reconciles_action_id", "TEXT"),
        (
            "receipt_actions",
            "completion_source",
            "TEXT NOT NULL DEFAULT 'execution'",
        ),
        ("receipt_actions", "parent_approval_ref", "TEXT"),
        ("receipt_actions", "legacy_approval_ref", "TEXT"),
        ("receipt_actions", "session_id", "TEXT"),
        ("receipt_actions", "snapshot_id", "TEXT"),
        ("receipt_actions", "snapshot_hash", "TEXT"),
        ("receipt_actions", "policy_version", "INTEGER"),
        ("receipt_actions", "hook_chain_version", "INTEGER"),
        ("receipt_approval_intents", "session_id", "TEXT"),
        ("receipt_approval_intents", "snapshot_hash", "TEXT"),
        ("receipt_approval_intents", "policy_version", "INTEGER"),
        ("receipt_approval_intents", "hook_chain_version", "INTEGER"),
        (
            "receipt_records",
            "source",
            "TEXT NOT NULL DEFAULT 'signed'",
        ),
    ] {
        let exists: Option<String> = connection
            .query_row(
                &format!("SELECT name FROM pragma_table_info('{table}') WHERE name='{column}'"),
                [],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            connection.execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    // SQLite cannot alter a CHECK constraint in place. Rebuild only the
    // legacy receipt table when it predates request_commit; all existing
    // signed action receipts are copied byte-for-byte.
    let receipt_sql: String = connection.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='receipt_records'",
        [],
        |row| row.get(0),
    )?;
    let action_id_not_null: i64 = connection
        .query_row(
            "SELECT COALESCE(\"notnull\",0) FROM pragma_table_info('receipt_records') WHERE name='action_id'",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default();
    if !receipt_sql.contains("request_commit") || action_id_not_null != 0 {
        let tx = connection.unchecked_transaction()?;
        tx.execute_batch(
            "ALTER TABLE receipt_records RENAME TO receipt_records_legacy;
             CREATE TABLE receipt_records (
               schema_version INTEGER NOT NULL DEFAULT 1,
               receipt_id TEXT PRIMARY KEY NOT NULL,
               action_id TEXT,
               request_id TEXT,
               receipt_kind TEXT NOT NULL CHECK(receipt_kind IN ('pre_action','post_action','refusal','request_commit')),
               action_status TEXT NOT NULL,
               task_id TEXT NOT NULL,
               run_id TEXT NOT NULL,
               key_id TEXT NOT NULL,
               canonical_payload BLOB NOT NULL,
               canonical_envelope BLOB NOT NULL,
               receipt_hash TEXT NOT NULL UNIQUE,
               previous_receipt_hash TEXT,
               created_at_ms INTEGER NOT NULL,
               source TEXT NOT NULL DEFAULT 'signed'
             );
             INSERT INTO receipt_records(schema_version,receipt_id,action_id,request_id,receipt_kind,action_status,task_id,run_id,key_id,canonical_payload,canonical_envelope,receipt_hash,previous_receipt_hash,created_at_ms,source)
               SELECT schema_version,receipt_id,action_id,request_id,receipt_kind,action_status,task_id,run_id,key_id,canonical_payload,canonical_envelope,receipt_hash,previous_receipt_hash,created_at_ms,COALESCE(source,'signed')
               FROM receipt_records_legacy;
             DROP TABLE receipt_records_legacy;
             CREATE INDEX IF NOT EXISTS idx_receipt_records_action ON receipt_records(action_id);",
        )?;
        tx.commit()?;
    }
    Ok(())
}
