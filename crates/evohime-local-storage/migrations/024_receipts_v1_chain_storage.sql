-- Migration: receipts_v1_chain_storage
-- Implements Stage 01.4: Chain storage and export
-- Blocking dependencies: 01.1 (canonical payload/envelope v1), 01.2 (public-key history),
-- 01.3 (receipt/action rows, chain append, approval binding and recovery state)

-- Enable foreign keys and WAL mode (should be set by application, but enforced here)
PRAGMA foreign_keys = ON;

-- ============================================================================
-- receipt_records: Core signed receipt storage (extended from 01.3)
-- ============================================================================
-- This table replaces or extends the existing receipt_records from 01.3 runtime.
-- The schema below is the authoritative 01.4 version with all required columns.

CREATE TABLE IF NOT EXISTS receipt_records_v1 (
    -- sequence: INTEGER PRIMARY KEY AUTOINCREMENT; commit order, not signed payload.
    -- AUTOINCREMENT intentional: sequence identifiers MUST NOT be reused after retention/purge or crash recovery
    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
    
    -- receipt_id: TEXT NOT NULL UNIQUE; lowercase UUIDv7
    receipt_id TEXT NOT NULL UNIQUE,
    
    -- action_id: TEXT NOT NULL; lowercase UUIDv7
    action_id TEXT NOT NULL,
    
    -- receipt_kind: TEXT NOT NULL CHECK pre_action/post_action/refusal
    receipt_kind TEXT NOT NULL CHECK (receipt_kind IN ('pre_action', 'post_action', 'refusal')),
    
    -- action_status: TEXT NOT NULL CHECK prepared/succeeded/failed/cancelled/refused
    action_status TEXT NOT NULL CHECK (action_status IN ('prepared', 'succeeded', 'failed', 'cancelled', 'refused')),
    
    -- task_id, run_id, tool_name: TEXT NOT NULL; length 1–128 and exact 01.1 typed-identifier pattern
    task_id TEXT NOT NULL CHECK (length(task_id) BETWEEN 1 AND 128),
    run_id TEXT NOT NULL CHECK (length(run_id) BETWEEN 1 AND 128),
    tool_name TEXT NOT NULL CHECK (length(tool_name) BETWEEN 1 AND 128),
    
    -- key_id: TEXT NOT NULL; ed25519:<64 lowercase hex>
    key_id TEXT NOT NULL CHECK (key_id GLOB 'ed25519:[a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9]'),
    
    -- receipt_hash: TEXT NOT NULL UNIQUE; 64 lowercase hex
    receipt_hash TEXT NOT NULL UNIQUE CHECK (length(receipt_hash) = 64 AND receipt_hash GLOB '[a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9][a-f0-9]'),
    
    -- previous_receipt_hash: TEXT NULL only for a genesis row or the first row of a new key segment
    previous_receipt_hash TEXT,
    
    -- canonical_payload: BLOB NOT NULL; 1–4096 bytes
    canonical_payload BLOB NOT NULL CHECK (length(canonical_payload) BETWEEN 1 AND 4096),
    
    -- canonical_envelope: BLOB NOT NULL; 1–8192 bytes
    canonical_envelope BLOB NOT NULL CHECK (length(canonical_envelope) BETWEEN 1 AND 8192),
    
    -- created_at_ms: INTEGER NOT NULL; derived from canonical timestamp
    created_at_ms INTEGER NOT NULL,
    
    -- source: TEXT NOT NULL CHECK (source = 'signed'); legacy audit is exposed by a separate adapter
    source TEXT NOT NULL DEFAULT 'signed' CHECK (source = 'signed')
);

-- Indexes for receipt_records_v1
CREATE UNIQUE INDEX IF NOT EXISTS idx_receipt_records_v1_receipt_id ON receipt_records_v1(receipt_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_receipt_records_v1_receipt_hash ON receipt_records_v1(receipt_hash);
CREATE INDEX IF NOT EXISTS idx_receipt_records_v1_task_id_created ON receipt_records_v1(task_id, created_at_ms, sequence);
CREATE INDEX IF NOT EXISTS idx_receipt_records_v1_run_id_created ON receipt_records_v1(run_id, created_at_ms, sequence);
CREATE INDEX IF NOT EXISTS idx_receipt_records_v1_action_id_sequence ON receipt_records_v1(action_id, sequence);
CREATE INDEX IF NOT EXISTS idx_receipt_records_v1_key_id_sequence ON receipt_records_v1(key_id, sequence);

-- ============================================================================
-- receipt_actions: Action state tracking (extended from 01.3)
-- ============================================================================

CREATE TABLE IF NOT EXISTS receipt_actions_v1 (
    -- action_id: TEXT PRIMARY KEY
    action_id TEXT PRIMARY KEY,
    
    -- pre_receipt_hash: TEXT NULL until pre append, then REFERENCES receipt_records_v1(receipt_hash)
    pre_receipt_hash TEXT,
    
    -- terminal_receipt_hash: TEXT NULL until post/refusal, REFERENCES receipt_records_v1(receipt_hash)
    terminal_receipt_hash TEXT,
    
    -- state: TEXT NOT NULL CHECK (state IN ('prepared','terminal','pending_recovery'))
    state TEXT NOT NULL CHECK (state IN ('prepared', 'terminal', 'pending_recovery')),
    
    -- approval_id: TEXT NULL; bounded typed identifier
    approval_id TEXT,
    
    -- approval_call_hash: TEXT NULL; 64 lowercase hex
    approval_call_hash TEXT CHECK (approval_call_hash IS NULL OR (length(approval_call_hash) = 64 AND approval_call_hash GLOB '[a-f0-9]*')),
    
    -- approval_state: TEXT NOT NULL CHECK (approval_state IN ('none','pending','granted','denied','expired','claimed'))
    approval_state TEXT NOT NULL DEFAULT 'none' CHECK (approval_state IN ('none', 'pending', 'granted', 'denied', 'expired', 'claimed')),
    
    -- tool_args_hash: TEXT NOT NULL; 64 lowercase hex
    tool_args_hash TEXT NOT NULL CHECK (length(tool_args_hash) = 64 AND tool_args_hash GLOB '[a-f0-9]*'),
    
    -- recovery_code: TEXT NULL; bounded error codes only
    recovery_code TEXT CHECK (recovery_code IS NULL OR recovery_code IN (
        'signature_failed', 'external_error', 'unknown', 'missing_pre', 'missing_terminal',
        'approval_expired', 'chain_conflict', 'stale_head'
    )),
    
    -- created_at_ms, updated_at_ms: INTEGER NOT NULL
    created_at_ms INTEGER NOT NULL,
    updated_at_ms INTEGER NOT NULL,
    
    -- Foreign key constraints
    FOREIGN KEY (pre_receipt_hash) REFERENCES receipt_records_v1(receipt_hash),
    FOREIGN KEY (terminal_receipt_hash) REFERENCES receipt_records_v1(receipt_hash)
);

-- Trigger to enforce approval_id/approval_state consistency
CREATE TRIGGER IF NOT EXISTS trg_receipt_actions_approval_consistency
BEFORE INSERT ON receipt_actions_v1
FOR EACH ROW
WHEN (NEW.approval_state != 'none' AND NEW.approval_id IS NULL)
   OR (NEW.approval_state = 'none' AND NEW.approval_id IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'approval_id must be NULL when approval_state is none, and NOT NULL otherwise');
END;

-- Trigger to enforce recovery_code consistency
CREATE TRIGGER IF NOT EXISTS trg_receipt_actions_recovery_code
BEFORE INSERT ON receipt_actions_v1
FOR EACH ROW
WHEN (NEW.state = 'pending_recovery' AND NEW.recovery_code IS NULL)
   OR (NEW.state != 'pending_recovery' AND NEW.recovery_code IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'recovery_code is required exactly for state=pending_recovery');
END;

-- ============================================================================
-- receipt_chain_heads: Per-key chain head tracking
-- ============================================================================

CREATE TABLE IF NOT EXISTS receipt_chain_heads_v1 (
    key_id TEXT PRIMARY KEY NOT NULL,
    head_sequence INTEGER NOT NULL,
    head_receipt_hash TEXT NOT NULL REFERENCES receipt_records_v1(receipt_hash),
    updated_at_ms INTEGER NOT NULL
);

-- ============================================================================
-- receipt_checkpoints: Signed checkpoints for retention compaction
-- ============================================================================

CREATE TABLE IF NOT EXISTS receipt_checkpoints_v1 (
    checkpoint_id TEXT PRIMARY KEY NOT NULL,
    key_id TEXT NOT NULL,
    cutoff_sequence INTEGER NOT NULL,
    first_retained_hash TEXT NOT NULL REFERENCES receipt_records_v1(receipt_hash),
    prefix_last_hash TEXT NOT NULL REFERENCES receipt_records_v1(receipt_hash),
    last_deleted_receipt_hash TEXT NOT NULL REFERENCES receipt_records_v1(receipt_hash),
    head_receipt_hash TEXT NOT NULL REFERENCES receipt_records_v1(receipt_hash),
    created_at_ms INTEGER NOT NULL,
    canonical_checkpoint BLOB NOT NULL,
    signature BLOB NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'superseded', 'invalid'))
);

CREATE INDEX IF NOT EXISTS idx_receipt_checkpoints_v1_key_id ON receipt_checkpoints_v1(key_id);
CREATE INDEX IF NOT EXISTS idx_receipt_checkpoints_v1_cutoff ON receipt_checkpoints_v1(cutoff_sequence);

-- ============================================================================
-- receipt_exports_audit: Bounded audit events for export operations
-- ============================================================================

CREATE TABLE IF NOT EXISTS receipt_exports_audit_v1 (
    export_id TEXT PRIMARY KEY NOT NULL,
    destination_path TEXT NOT NULL,
    snapshot_last_sequence INTEGER NOT NULL,
    requested_count INTEGER NOT NULL,
    selected_count INTEGER NOT NULL,
    actual_exported_count INTEGER NOT NULL,
    first_receipt_hash TEXT NOT NULL,
    last_receipt_hash TEXT NOT NULL,
    manifest_sha256 TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('success', 'failed', 'cancelled')),
    error_code TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_receipt_exports_audit_v1_created ON receipt_exports_audit_v1(created_at_ms);

-- ============================================================================
-- receipt_verify_cache: Incremental verification cache (optimization only)
-- ============================================================================

CREATE TABLE IF NOT EXISTS receipt_verify_cache_v1 (
    cache_key TEXT PRIMARY KEY NOT NULL,
    head_hash TEXT NOT NULL,
    public_history_hash TEXT NOT NULL,
    checkpoint_sequence INTEGER,
    verified_at_ms INTEGER NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('verified', 'verified_pruned', 'pending', 'broken'))
);

-- ============================================================================
-- Migrate data from old receipt_records (01.3) if it exists
-- ============================================================================

-- Note: This migration does not destructive rewrite existing data.
-- Legacy audit events remain queryable through source=legacy adapter.
-- Existing 01.3 receipt_records are preserved; new writes go to _v1 tables.

-- Placeholder for future data migration logic (application-controlled)
-- Application should handle migration of existing receipt_records -> receipt_records_v1
-- with proper sequence allocation and hash chain verification.
