use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 9 {
        t.execute_batch("CREATE TABLE IF NOT EXISTS capability_manifests (id TEXT PRIMARY KEY NOT NULL, kind TEXT NOT NULL, version TEXT NOT NULL, risk_class TEXT NOT NULL, content_hash TEXT NOT NULL, manifest_json BLOB NOT NULL, installed_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))); CREATE INDEX IF NOT EXISTS idx_capability_manifests_kind ON capability_manifests(kind); PRAGMA user_version = 9;")?;
    }
    Ok(())
}
