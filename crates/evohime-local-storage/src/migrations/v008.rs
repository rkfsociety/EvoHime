use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 8 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS research_evidence (id TEXT PRIMARY KEY NOT NULL, source_kind TEXT NOT NULL, source_ref TEXT NOT NULL, redacted_excerpt TEXT NOT NULL, source_hash TEXT NOT NULL, fetched_at TEXT NOT NULL, ttl_seconds INTEGER NOT NULL, provenance_link TEXT); CREATE INDEX IF NOT EXISTS idx_research_evidence_provenance ON research_evidence(provenance_link); CREATE TABLE IF NOT EXISTS memory_entries (id TEXT PRIMARY KEY NOT NULL, scope_kind TEXT NOT NULL, scope_id TEXT NOT NULL, title TEXT NOT NULL, content TEXT NOT NULL, provenance TEXT NOT NULL, privacy TEXT NOT NULL, created_at TEXT NOT NULL, expires_at TEXT, archived INTEGER NOT NULL, forgotten INTEGER NOT NULL); CREATE INDEX IF NOT EXISTS idx_memory_entries_scope ON memory_entries(scope_kind, scope_id); PRAGMA user_version = 8;")?;
    }
    Ok(())
}
