//! Durable metadata boundary for Grounded Research Workspace (plan 120.1).
//!
//! Source bytes stay in ArtifactStore or the workspace-RAG generation.  This
//! store contains only bounded metadata and immutable lineage.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_METADATA_BYTES: usize = 32 * 1024;

pub type EvidenceRow = (String, Vec<u8>, String, String);
pub type ArtifactRow = (Vec<u8>, Vec<u8>, String, String);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ResearchRevisionRecord {
    pub revision_id: String,
    pub workspace_id: String,
    pub source_id: String,
    pub revision: i64,
    pub content_hash: String,
    pub origin_snapshot: String,
    pub parser_version: String,
    pub index_profile: String,
    pub status: String,
    pub trust: String,
    pub locator_root: String,
}

impl ResearchRevisionRecord {
    pub fn validate(&self) -> Result<(), &'static str> {
        for (name, value) in [
            ("revision_id", &self.revision_id),
            ("workspace_id", &self.workspace_id),
            ("source_id", &self.source_id),
            ("content_hash", &self.content_hash),
            ("origin_snapshot", &self.origin_snapshot),
            ("parser_version", &self.parser_version),
            ("index_profile", &self.index_profile),
            ("status", &self.status),
            ("trust", &self.trust),
            ("locator_root", &self.locator_root),
        ] {
            if value.trim().is_empty() {
                return Err(name);
            }
        }
        if self.revision <= 0 || self.origin_snapshot.len() > MAX_METADATA_BYTES {
            return Err("revision");
        }
        if !matches!(
            self.status.as_str(),
            "pending"
                | "acquiring"
                | "parsing"
                | "extracting"
                | "indexing"
                | "ready"
                | "partially_ready"
                | "failed"
                | "stale"
                | "unavailable"
                | "removing"
        ) || !matches!(
            self.trust.as_str(),
            "workspace" | "user_provided" | "acquired_external" | "derived" | "unverified"
        ) {
            return Err("revision state");
        }
        Ok(())
    }
}

pub struct GroundedResearchStore;

impl GroundedResearchStore {
    pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS research_source_revisions (
                revision_id TEXT PRIMARY KEY NOT NULL,
                workspace_id TEXT NOT NULL,
                source_id TEXT NOT NULL,
                revision INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                origin_snapshot BLOB NOT NULL,
                parser_version TEXT NOT NULL,
                index_profile TEXT NOT NULL,
                status TEXT NOT NULL,
                trust TEXT NOT NULL,
                locator_root TEXT NOT NULL,
                UNIQUE(source_id, revision),
                UNIQUE(source_id, revision, content_hash)
            );
            CREATE TABLE IF NOT EXISTS research_evidence_items (
                evidence_id TEXT PRIMARY KEY NOT NULL,
                revision_id TEXT NOT NULL REFERENCES research_source_revisions(revision_id),
                locator_json BLOB NOT NULL,
                content_hash TEXT NOT NULL,
                trust TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_sessions (
                session_id TEXT PRIMARY KEY NOT NULL,
                workspace_id TEXT NOT NULL,
                collection_id TEXT NOT NULL REFERENCES knowledge_collections(collection_id),
                session_revision INTEGER NOT NULL,
                mode TEXT NOT NULL,
                source_policy TEXT NOT NULL,
                pinned_revision_ids_json BLOB NOT NULL,
                tool_policy_snapshot BLOB NOT NULL,
                model_policy_snapshot BLOB NOT NULL,
                budget_json BLOB NOT NULL,
                state TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_subtasks (
                subtask_id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES research_sessions(session_id),
                objective_hash TEXT NOT NULL,
                state TEXT NOT NULL,
                evidence_ids_json BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_claims (
                claim_id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES research_sessions(session_id),
                text_hash TEXT NOT NULL,
                inference INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_citations (
                citation_id TEXT PRIMARY KEY NOT NULL,
                claim_id TEXT NOT NULL REFERENCES research_claims(claim_id),
                evidence_id TEXT NOT NULL REFERENCES research_evidence_items(evidence_id),
                kind TEXT NOT NULL,
                validation TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_conflicts (
                conflict_id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL REFERENCES research_sessions(session_id),
                evidence_ids_json BLOB NOT NULL,
                description_hash TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS research_artifacts (
                artifact_id TEXT NOT NULL,
                revision INTEGER NOT NULL,
                session_id TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                coverage TEXT NOT NULL,
                claims_json BLOB NOT NULL,
                citations_json BLOB NOT NULL,
                created_at_ms INTEGER NOT NULL,
                PRIMARY KEY(artifact_id, revision)
            );
            CREATE TABLE IF NOT EXISTS research_deltas (
                delta_id TEXT PRIMARY KEY NOT NULL,
                previous_artifact_id TEXT NOT NULL,
                current_artifact_id TEXT NOT NULL,
                added_evidence_ids_json BLOB NOT NULL,
                stale_evidence_ids_json BLOB NOT NULL
            );
            CREATE TRIGGER IF NOT EXISTS research_artifacts_immutable_update
            BEFORE UPDATE ON research_artifacts
            BEGIN SELECT RAISE(ABORT, 'research artifacts are immutable'); END;
            CREATE TRIGGER IF NOT EXISTS research_artifacts_immutable_delete
            BEFORE DELETE ON research_artifacts
            BEGIN SELECT RAISE(ABORT, 'research artifacts are immutable'); END;
            CREATE INDEX IF NOT EXISTS idx_research_revisions_workspace
                ON research_source_revisions(workspace_id, source_id, revision);
            CREATE INDEX IF NOT EXISTS idx_research_evidence_revision
                ON research_evidence_items(revision_id);
            ",
        )
    }

    pub fn insert_revision(
        connection: &Connection,
        record: &ResearchRevisionRecord,
    ) -> Result<bool, &'static str> {
        record.validate()?;
        if let Some(existing) = connection
            .query_row(
                "SELECT content_hash, origin_snapshot, parser_version, index_profile,
                        status, trust, locator_root
                 FROM research_source_revisions
                 WHERE source_id = ?1 AND revision = ?2",
                params![record.source_id, record.revision],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| "sqlite")?
        {
            if existing
                != (
                    record.content_hash.clone(),
                    record.origin_snapshot.clone(),
                    record.parser_version.clone(),
                    record.index_profile.clone(),
                    record.status.clone(),
                    record.trust.clone(),
                    record.locator_root.clone(),
                )
            {
                return Err("immutable revision conflict");
            }
            return Ok(false);
        }
        let inserted = connection
            .execute(
                "INSERT OR IGNORE INTO research_source_revisions
                 (revision_id, workspace_id, source_id, revision, content_hash,
                  origin_snapshot, parser_version, index_profile, status, trust, locator_root)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    record.revision_id,
                    record.workspace_id,
                    record.source_id,
                    record.revision,
                    record.content_hash,
                    record.origin_snapshot,
                    record.parser_version,
                    record.index_profile,
                    record.status,
                    record.trust,
                    record.locator_root,
                ],
            )
            .map_err(|_| "sqlite")?;
        Ok(inserted == 1)
    }

    pub fn get_revision(
        connection: &Connection,
        revision_id: &str,
    ) -> rusqlite::Result<Option<ResearchRevisionRecord>> {
        connection
            .query_row(
                "SELECT revision_id, workspace_id, source_id, revision, content_hash,
                        origin_snapshot, parser_version, index_profile, status, trust, locator_root
                 FROM research_source_revisions WHERE revision_id = ?1",
                params![revision_id],
                |row| {
                    Ok(ResearchRevisionRecord {
                        revision_id: row.get(0)?,
                        workspace_id: row.get(1)?,
                        source_id: row.get(2)?,
                        revision: row.get(3)?,
                        content_hash: row.get(4)?,
                        origin_snapshot: row.get(5)?,
                        parser_version: row.get(6)?,
                        index_profile: row.get(7)?,
                        status: row.get(8)?,
                        trust: row.get(9)?,
                        locator_root: row.get(10)?,
                    })
                },
            )
            .optional()
    }

    pub fn insert_evidence_item(
        connection: &Connection,
        evidence_id: &str,
        revision_id: &str,
        locator_json: &[u8],
        content_hash: &str,
        trust: &str,
    ) -> Result<bool, &'static str> {
        validate_bounded("evidence_id", evidence_id, 128)?;
        validate_bounded("revision_id", revision_id, 128)?;
        validate_bounded("content_hash", content_hash, 128)?;
        validate_bounded("trust", trust, 64)?;
        if locator_json.is_empty() || locator_json.len() > MAX_METADATA_BYTES {
            return Err("locator_json");
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO research_evidence_items
                 (evidence_id, revision_id, locator_json, content_hash, trust)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![evidence_id, revision_id, locator_json, content_hash, trust],
            )
            .map(|count| count == 1)
            .map_err(|_| "sqlite")
    }

    pub fn get_evidence(
        connection: &Connection,
        evidence_id: &str,
    ) -> rusqlite::Result<Option<EvidenceRow>> {
        connection
            .query_row(
                "SELECT revision_id, locator_json, content_hash, trust
                 FROM research_evidence_items WHERE evidence_id = ?1",
                params![evidence_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_session(
        connection: &Connection,
        session_id: &str,
        workspace_id: &str,
        collection_id: &str,
        revision: i64,
        mode: &str,
        source_policy: &str,
        pinned_revision_ids_json: &[u8],
        tool_policy_snapshot: &[u8],
        model_policy_snapshot: &[u8],
        budget_json: &[u8],
        state: &str,
    ) -> Result<bool, &'static str> {
        for (field, value, max) in [
            ("session_id", session_id, 128),
            ("workspace_id", workspace_id, 256),
            ("collection_id", collection_id, 128),
            ("mode", mode, 64),
            ("source_policy", source_policy, 64),
            ("state", state, 64),
        ] {
            validate_bounded(field, value, max)?;
        }
        if revision <= 0 || pinned_revision_ids_json.len() > MAX_METADATA_BYTES {
            return Err("session revision");
        }
        if !matches!(
            mode,
            "quick_research"
                | "deep_research"
                | "comparison"
                | "literature_review"
                | "technical_investigation"
                | "learning_notes"
        ) || !matches!(
            source_policy,
            "selected_only"
                | "selected_plus_workspace"
                | "selected_plus_web"
                | "open_research_within_policy"
        ) || !matches!(
            state,
            "queued"
                | "running"
                | "cancelling"
                | "completed"
                | "partial"
                | "failed"
                | "interrupted"
        ) {
            return Err("session state");
        }
        for (field, value) in [
            ("pinned_revision_ids", pinned_revision_ids_json),
            ("tool_policy_snapshot", tool_policy_snapshot),
            ("model_policy_snapshot", model_policy_snapshot),
            ("budget", budget_json),
        ] {
            if value.is_empty() || value.len() > MAX_METADATA_BYTES {
                return Err(field);
            }
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO research_sessions
                 (session_id, workspace_id, collection_id, session_revision, mode,
                  source_policy, pinned_revision_ids_json, tool_policy_snapshot,
                  model_policy_snapshot, budget_json, state)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    session_id,
                    workspace_id,
                    collection_id,
                    revision,
                    mode,
                    source_policy,
                    pinned_revision_ids_json,
                    tool_policy_snapshot,
                    model_policy_snapshot,
                    budget_json,
                    state,
                ],
            )
            .map(|count| count == 1)
            .map_err(|_| "sqlite")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_artifact(
        connection: &Connection,
        artifact_id: &str,
        revision: i64,
        session_id: &str,
        content_hash: &str,
        coverage: &str,
        claims_json: &[u8],
        citations_json: &[u8],
        created_at_ms: i64,
    ) -> Result<bool, &'static str> {
        for (field, value, max) in [
            ("artifact_id", artifact_id, 128),
            ("session_id", session_id, 128),
            ("content_hash", content_hash, 128),
            ("coverage", coverage, 64),
        ] {
            validate_bounded(field, value, max)?;
        }
        if !matches!(
            coverage,
            "complete" | "partial" | "budget_limited" | "source_limited" | "failed"
        ) {
            return Err("artifact coverage");
        }
        if revision <= 0
            || created_at_ms <= 0
            || claims_json.is_empty()
            || citations_json.is_empty()
            || claims_json.len() > MAX_METADATA_BYTES
            || citations_json.len() > MAX_METADATA_BYTES
        {
            return Err("artifact");
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO research_artifacts
                 (artifact_id, revision, session_id, content_hash, coverage,
                  claims_json, citations_json, created_at_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    artifact_id,
                    revision,
                    session_id,
                    content_hash,
                    coverage,
                    claims_json,
                    citations_json,
                    created_at_ms,
                ],
            )
            .map(|count| count == 1)
            .map_err(|_| "sqlite")
    }

    pub fn get_artifact(
        connection: &Connection,
        artifact_id: &str,
        revision: i64,
    ) -> rusqlite::Result<Option<ArtifactRow>> {
        connection
            .query_row(
                "SELECT claims_json, citations_json, content_hash, coverage
                 FROM research_artifacts
                 WHERE artifact_id = ?1 AND revision = ?2",
                params![artifact_id, revision],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
    }

    pub fn insert_delta(
        connection: &Connection,
        delta_id: &str,
        previous_artifact_id: &str,
        current_artifact_id: &str,
        added_evidence_ids_json: &[u8],
        stale_evidence_ids_json: &[u8],
    ) -> Result<bool, &'static str> {
        for (field, value) in [
            ("delta_id", delta_id),
            ("previous_artifact_id", previous_artifact_id),
            ("current_artifact_id", current_artifact_id),
        ] {
            validate_bounded(field, value, 128)?;
        }
        for (field, value) in [
            ("added_evidence_ids", added_evidence_ids_json),
            ("stale_evidence_ids", stale_evidence_ids_json),
        ] {
            if value.is_empty() || value.len() > MAX_METADATA_BYTES {
                return Err(field);
            }
        }
        connection
            .execute(
                "INSERT OR IGNORE INTO research_deltas
                 (delta_id, previous_artifact_id, current_artifact_id,
                  added_evidence_ids_json, stale_evidence_ids_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    delta_id,
                    previous_artifact_id,
                    current_artifact_id,
                    added_evidence_ids_json,
                    stale_evidence_ids_json,
                ],
            )
            .map(|count| count == 1)
            .map_err(|_| "sqlite")
    }

    pub fn transition_session(
        connection: &Connection,
        session_id: &str,
        expected_revision: i64,
        from_state: &str,
        next_state: &str,
    ) -> Result<bool, &'static str> {
        validate_bounded("session_id", session_id, 128)?;
        for (field, value) in [("from_state", from_state), ("next_state", next_state)] {
            validate_bounded(field, value, 64)?;
        }
        if expected_revision <= 0 {
            return Err("session revision");
        }
        let changed = connection
            .execute(
                "UPDATE research_sessions
                 SET state = ?4, session_revision = session_revision + 1
                 WHERE session_id = ?1 AND session_revision = ?2 AND state = ?3",
                params![session_id, expected_revision, from_state, next_state],
            )
            .map_err(|_| "sqlite")?;
        Ok(changed == 1)
    }

    /// Converts sessions that were active when Core stopped into an explicit
    /// restart-recoverable state. No session is silently resumed or marked
    /// successful; a later caller must use the normal CAS transition.
    pub fn mark_active_sessions_interrupted(connection: &Connection) -> rusqlite::Result<usize> {
        connection.execute(
            "UPDATE research_sessions
             SET state = 'interrupted', session_revision = session_revision + 1
             WHERE state IN ('queued', 'running', 'cancelling')",
            [],
        )
    }
}

fn validate_bounded(field: &'static str, value: &str, max: usize) -> Result<(), &'static str> {
    if value.trim().is_empty() || value.chars().count() > max {
        Err(field)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_insert_is_idempotent_and_artifact_is_immutable() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE knowledge_collections (collection_id TEXT PRIMARY KEY);")
            .unwrap();
        GroundedResearchStore::install_schema(&connection).unwrap();
        let record = ResearchRevisionRecord {
            revision_id: "revision-1".into(),
            workspace_id: "workspace-1".into(),
            source_id: "source-1".into(),
            revision: 1,
            content_hash: "a".repeat(64),
            origin_snapshot: "{}".into(),
            parser_version: "parser/v1".into(),
            index_profile: "index/v1".into(),
            status: "ready".into(),
            trust: "workspace".into(),
            locator_root: "file:README.md".into(),
        };
        assert!(GroundedResearchStore::insert_revision(&connection, &record).unwrap());
        assert!(!GroundedResearchStore::insert_revision(&connection, &record).unwrap());
        connection
            .execute(
                "INSERT INTO research_artifacts
                 (artifact_id,revision,session_id,content_hash,coverage,claims_json,citations_json,created_at_ms)
                 VALUES ('a',1,'s',?1,'partial','[]','[]',1)",
                ["b".repeat(64)],
            )
            .unwrap();
        assert!(connection
            .execute("UPDATE research_artifacts SET coverage='complete'", [])
            .is_err());
    }
}
