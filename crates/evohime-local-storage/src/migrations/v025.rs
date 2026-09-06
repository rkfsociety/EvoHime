use rusqlite::Transaction;

pub(crate) fn apply(t: &Transaction<'_>, c: u32) -> rusqlite::Result<()> {
    if c < 25 {
        t.execute_batch(
            "CREATE TABLE IF NOT EXISTS ambient_episodes (
                episode_id TEXT PRIMARY KEY NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                utterance_count INTEGER NOT NULL,
                speech_ms INTEGER NOT NULL,
                engine_version TEXT NOT NULL,
                model_id TEXT NOT NULL,
                extraction_state TEXT NOT NULL CHECK(extraction_state IN
                    ('disabled','pending','done','failed')),
                expires_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS ambient_utterances (
                utterance_id TEXT PRIMARY KEY NOT NULL,
                episode_id TEXT NOT NULL
                    REFERENCES ambient_episodes(episode_id) ON DELETE CASCADE,
                sequence INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                text TEXT NOT NULL,
                text_hash TEXT NOT NULL,
                language TEXT NOT NULL,
                avg_logprob REAL NOT NULL,
                speaker TEXT NOT NULL,
                redacted INTEGER NOT NULL DEFAULT 0,
                expires_at TEXT NOT NULL,
                UNIQUE(episode_id, sequence)
            );
            CREATE TABLE IF NOT EXISTS ambient_tombstones (
                tombstone_id TEXT PRIMARY KEY NOT NULL,
                episode_id TEXT NOT NULL,
                removed_at TEXT NOT NULL,
                reason TEXT NOT NULL,
                utterance_count INTEGER NOT NULL,
                expires_at TEXT NOT NULL,
                UNIQUE(episode_id, removed_at)
            );
            CREATE INDEX IF NOT EXISTS idx_ambient_utterances_episode
                ON ambient_utterances(episode_id, sequence);
            CREATE INDEX IF NOT EXISTS idx_ambient_expiry
                ON ambient_utterances(expires_at);
            CREATE INDEX IF NOT EXISTS idx_ambient_episode_expiry
                ON ambient_episodes(expires_at);
            CREATE INDEX IF NOT EXISTS idx_ambient_tombstone_expiry
                ON ambient_tombstones(expires_at);
            PRAGMA user_version = 25;",
        )?;
    }
    Ok(())
}
