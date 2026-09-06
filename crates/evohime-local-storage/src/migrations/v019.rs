use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, current: u32) -> rusqlite::Result<()> {
    if current >= 19 {
        return Ok(());
    }
    t.execute_batch(
        "CREATE TABLE IF NOT EXISTS workspace_index_runs (run_id TEXT PRIMARY KEY NOT NULL, workspace_key TEXT NOT NULL, generation INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('running','published','superseded','cancelled','failed')), started_at INTEGER NOT NULL, finished_at INTEGER, published_at INTEGER, scanner_version TEXT NOT NULL, chunker_version TEXT NOT NULL, tokenizer_version TEXT NOT NULL, file_count INTEGER NOT NULL DEFAULT 0, chunk_count INTEGER NOT NULL DEFAULT 0, excluded_count INTEGER NOT NULL DEFAULT 0, error_count INTEGER NOT NULL DEFAULT 0, error_summary TEXT NOT NULL DEFAULT '[]', dirty INTEGER NOT NULL DEFAULT 1, UNIQUE(workspace_key, generation));
         CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_index_published ON workspace_index_runs(workspace_key) WHERE status = 'published';
         CREATE INDEX IF NOT EXISTS idx_workspace_index_runs_state ON workspace_index_runs(workspace_key, status, generation);
         CREATE TABLE IF NOT EXISTS workspace_documents (document_id TEXT PRIMARY KEY NOT NULL, workspace_key TEXT NOT NULL, path TEXT NOT NULL, generation INTEGER NOT NULL, language TEXT NOT NULL, mime TEXT NOT NULL, file_hash TEXT NOT NULL, size_bytes INTEGER NOT NULL, encoding TEXT NOT NULL, decode_status TEXT NOT NULL, last_modified INTEGER NOT NULL, indexed_at INTEGER NOT NULL, status TEXT NOT NULL CHECK(status IN ('active','unstable','deleted')), redaction_status TEXT NOT NULL CHECK(redaction_status IN ('none','partial','full')), is_secret_path INTEGER NOT NULL DEFAULT 0, UNIQUE(workspace_key, generation, path));
         CREATE INDEX IF NOT EXISTS idx_workspace_documents_scope ON workspace_documents(workspace_key, generation, status, path);
         CREATE INDEX IF NOT EXISTS idx_workspace_documents_language ON workspace_documents(workspace_key, generation, language);
         CREATE TABLE IF NOT EXISTS document_chunks (chunk_id TEXT PRIMARY KEY NOT NULL, document_id TEXT NOT NULL, workspace_key TEXT NOT NULL, generation INTEGER NOT NULL, ordinal INTEGER NOT NULL, chunk_hash TEXT NOT NULL, byte_start INTEGER NOT NULL, byte_end INTEGER NOT NULL, line_start INTEGER, line_end INTEGER, parent_context TEXT NOT NULL, chunk_text TEXT NOT NULL, symbol TEXT, symbol_normalized TEXT NOT NULL DEFAULT '', token_count INTEGER NOT NULL, byte_count INTEGER NOT NULL, strategy_version TEXT NOT NULL, FOREIGN KEY(document_id) REFERENCES workspace_documents(document_id) ON DELETE CASCADE, UNIQUE(document_id, generation, ordinal));
         CREATE INDEX IF NOT EXISTS idx_document_chunks_active ON document_chunks(workspace_key, generation, document_id, ordinal);
         CREATE INDEX IF NOT EXISTS idx_document_chunks_hash ON document_chunks(workspace_key, chunk_hash);
         CREATE VIRTUAL TABLE IF NOT EXISTS workspace_chunks_fts USING fts5(chunk_text, symbol_normalized, path, parent_context, chunk_id UNINDEXED, workspace_key UNINDEXED, generation UNINDEXED, tokenize='trigram');
         CREATE TABLE IF NOT EXISTS workspace_vector_indexes (index_id TEXT PRIMARY KEY NOT NULL, workspace_key TEXT NOT NULL, source_generation INTEGER NOT NULL, embedding_model_id TEXT NOT NULL, embedding_model_version TEXT NOT NULL, vector_dimension INTEGER NOT NULL, distance_metric TEXT NOT NULL, normalization TEXT NOT NULL, chunker_version TEXT NOT NULL, build_status TEXT NOT NULL CHECK(build_status IN ('building','ready','published','deprecated','failed','cancelled')), created_at INTEGER NOT NULL, published_at INTEGER, vector_count INTEGER NOT NULL DEFAULT 0);
         CREATE UNIQUE INDEX IF NOT EXISTS idx_workspace_vector_published ON workspace_vector_indexes(workspace_key) WHERE build_status = 'published';
         CREATE INDEX IF NOT EXISTS idx_workspace_vector_state ON workspace_vector_indexes(workspace_key, source_generation, build_status);
         CREATE TABLE IF NOT EXISTS workspace_chunk_vectors (index_id TEXT NOT NULL, chunk_id TEXT NOT NULL, vector BLOB NOT NULL, PRIMARY KEY(index_id, chunk_id), FOREIGN KEY(index_id) REFERENCES workspace_vector_indexes(index_id) ON DELETE CASCADE, FOREIGN KEY(chunk_id) REFERENCES document_chunks(chunk_id) ON DELETE CASCADE);
         CREATE TABLE IF NOT EXISTS rag_context_ledger (ledger_id TEXT NOT NULL, query_id TEXT NOT NULL, block_id TEXT NOT NULL, rank INTEGER NOT NULL, retrieval_score REAL NOT NULL, checker_confidence REAL NOT NULL, chunk_hash TEXT NOT NULL, snippet_hash TEXT NOT NULL, path TEXT NOT NULL, line_start INTEGER, line_end INTEGER, citation_status TEXT NOT NULL, selection_reason TEXT NOT NULL, reread_result TEXT NOT NULL, error_code TEXT, created_at INTEGER NOT NULL, PRIMARY KEY(ledger_id, block_id));
         CREATE INDEX IF NOT EXISTS idx_rag_context_ledger_query ON rag_context_ledger(query_id, rank);
         PRAGMA user_version = 19;",
    )
}
