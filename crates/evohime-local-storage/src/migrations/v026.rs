use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 26 {
        t.execute_batch(
            "CREATE TABLE IF NOT EXISTS ambient_proposals (
                proposal_id TEXT PRIMARY KEY NOT NULL,
                proposal_key TEXT NOT NULL UNIQUE,
                mute_key TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('suggestion','reminder')),
                subject_key TEXT NOT NULL,
                subject TEXT NOT NULL,
                title TEXT NOT NULL,
                source_episode_id TEXT REFERENCES ambient_episodes(episode_id) ON DELETE SET NULL,
                source_deleted_at TEXT,
                source_deleted_reason TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                occurrences INTEGER NOT NULL DEFAULT 1,
                state TEXT NOT NULL CHECK(state IN ('proposed','accepted','declined','muted','expired')),
                accepted_task_id TEXT,
                idempotency_key TEXT,
                CHECK((source_deleted_at IS NULL AND source_deleted_reason IS NULL) OR (source_deleted_at IS NOT NULL AND source_deleted_reason IS NOT NULL))
            );
            CREATE TABLE IF NOT EXISTS ambient_proposal_mutes (
                mute_key TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('suggestion','reminder')),
                subject_key TEXT NOT NULL,
                muted_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ambient_proactivity_counters (
                profile_id TEXT PRIMARY KEY NOT NULL,
                hour_started_at_ms INTEGER NOT NULL,
                hour_count INTEGER NOT NULL,
                day_started_at_ms INTEGER NOT NULL,
                day_count INTEGER NOT NULL,
                last_proposed_at_ms INTEGER
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_ambient_proposal_idempotency ON ambient_proposals(idempotency_key) WHERE idempotency_key IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_ambient_proposal_state ON ambient_proposals(state, expires_at);
            CREATE INDEX IF NOT EXISTS idx_ambient_proposal_source ON ambient_proposals(source_episode_id);
            CREATE INDEX IF NOT EXISTS idx_ambient_proposal_mute ON ambient_proposals(mute_key);
            PRAGMA user_version = 26;",
        )?;
    }
    Ok(())
}
