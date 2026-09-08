use super::*;

impl EventJournal {
    pub async fn recover_grounded_research_sessions(&self) -> Result<usize, String> {
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::mark_active_sessions_interrupted(
            database.connection(),
        )
        .map_err(|error| error.to_string())
    }

    /// Persists only immutable research revision metadata.  Source bytes stay
    /// in ArtifactStore or the workspace-RAG generation.
    pub async fn save_grounded_research_revision(
        &self,
        record: &evohime_local_storage::grounded_research_store::ResearchRevisionRecord,
    ) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_revision(
            database.connection(),
            record,
        )
        .map_err(str::to_owned)
    }

    pub async fn get_grounded_research_revision(
        &self,
        revision_id: &str,
    ) -> Result<
        Option<evohime_local_storage::grounded_research_store::ResearchRevisionRecord>,
        String,
    > {
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::get_revision(
            database.connection(),
            revision_id,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn save_grounded_research_session(
        &self,
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
    ) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_session(
            database.connection(),
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
        )
        .map_err(str::to_owned)
    }

    pub async fn save_grounded_research_evidence_item(
        &self,
        evidence: &crate::research::EvidenceItem,
    ) -> Result<bool, String> {
        let locator_json =
            serde_json::to_vec(&evidence.locator).map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_evidence_item(
            database.connection(),
            &evidence.evidence_id,
            &evidence.revision_id,
            &locator_json,
            &evidence.content_hash,
            &serde_json::to_string(&evidence.trust).map_err(|error| error.to_string())?,
        )
        .map_err(str::to_owned)
    }

    pub async fn get_grounded_research_artifact(
        &self,
        artifact_id: &str,
        revision: i64,
    ) -> Result<Option<(Vec<u8>, Vec<u8>, String, String)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::get_artifact(
            database.connection(),
            artifact_id,
            revision,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn save_grounded_research_delta(
        &self,
        delta: &crate::research::ResearchDelta,
    ) -> Result<bool, String> {
        let added =
            serde_json::to_vec(&delta.added_evidence_ids).map_err(|error| error.to_string())?;
        let stale =
            serde_json::to_vec(&delta.stale_evidence_ids).map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::insert_delta(
            database.connection(),
            &delta.delta_id,
            &delta.previous_artifact_id,
            &delta.current_artifact_id,
            &added,
            &stale,
        )
        .map_err(str::to_owned)
    }

    pub async fn transition_grounded_research_session(
        &self,
        session_id: &str,
        expected_revision: i64,
        from: crate::research::ResearchSessionState,
        next: crate::research::ResearchSessionState,
    ) -> Result<bool, String> {
        crate::research::transition_research_session(from, next)
            .map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        evohime_local_storage::grounded_research_store::GroundedResearchStore::transition_session(
            database.connection(),
            session_id,
            expected_revision,
            &serde_json::to_string(&from).map_err(|error| error.to_string())?,
            &serde_json::to_string(&next).map_err(|error| error.to_string())?,
        )
        .map_err(str::to_owned)
    }

    pub async fn create_project(
        &self,
        id: &str,
        title: &str,
        workspace_path: &str,
        source_ref: Option<&str>,
    ) -> Result<evohime_local_storage::ProjectRecord, StorageError> {
        let database = self.database.lock().await;
        database.create_project(id, title, workspace_path, source_ref)
    }

    pub async fn get_project(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::ProjectRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_project(id)
    }

    pub async fn get_project_by_workspace_path(
        &self,
        workspace_path: &str,
    ) -> Result<Option<evohime_local_storage::ProjectRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_project_by_workspace_path(workspace_path)
    }

    /// Persists one redacted, bounded research evidence record against the
    /// real `research_evidence` table (SCHEMA_VERSION 8).
    pub async fn save_research_evidence(
        &self,
        record: &evohime_local_storage::research_store::ResearchEvidenceRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::research_store::ResearchEvidenceSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists research evidence records tied to a work item, oldest id first.
    pub async fn list_research_evidence(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<evohime_local_storage::research_store::ResearchEvidenceRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::research_store::ResearchEvidenceSql::list_by_provenance(
            database.connection(),
            work_item_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one bounded, redacted Memory v1 record against the real
    /// `memory_entries` table (SCHEMA_VERSION 8).
    pub async fn save_memory(
        &self,
        record: &evohime_local_storage::domains::memory::MemoryRecord,
    ) -> Result<(), String> {
        let mut governed = record.clone();
        if matches!(
            governed.extraction.confirmation_state.as_str(),
            "candidate" | "pending_confirmation"
        ) && governed.extraction.authority == "user_asserted"
        {
            governed.extraction.authority = "model_proposed".to_owned();
        }
        crate::memory_governance::MemoryWriteGate::validate(&governed)
            .map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::insert(
            database.connection(),
            &governed,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists non-forgotten Memory v1 records for one exact scope.
    pub async fn list_memory(
        &self,
        scope: evohime_local_storage::domains::memory::MemoryScope,
        scope_id: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::memory::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::list(
            database.connection(),
            scope,
            scope_id,
            include_archived,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Lexical, deterministic search over Memory v1 records for one exact
    /// scope.
    pub async fn search_memory(
        &self,
        scope: evohime_local_storage::domains::memory::MemoryScope,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::memory::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::search(
            database.connection(),
            scope,
            scope_id,
            query,
            now,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Searches project-scoped memories for the current workspace so the
    /// agent can use user-created facts and decisions, not only automatic
    /// failure lessons.
    pub async fn search_workspace_memory(
        &self,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::memory::MemoryRecord>, String> {
        self.search_memory(
            evohime_local_storage::domains::memory::MemoryScope::Project,
            scope_id,
            query,
            now,
            limit,
        )
        .await
    }

    /// Archives a memory record. Returns `false` if no matching, non-forgotten
    /// record was found.
    pub async fn archive_memory(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::archive(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Forgets (erases title/content of) a memory record. Returns `false` if
    /// no matching row was found.
    pub async fn forget_memory(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::forget(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Reads one memory record by id, including body. Privacy redaction is
    /// applied by the caller, not here.
    pub async fn get_memory(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::domains::memory::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::get_by_id(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Records in one `confirmation_state` for one exact scope: the pending
    /// queue and the rejected/superseded history use the same path.
    pub async fn list_memory_by_state(
        &self,
        scope: evohime_local_storage::domains::memory::MemoryScope,
        scope_id: &str,
        state: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::memory::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::list_by_state(
            database.connection(),
            scope,
            scope_id,
            state,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Per-state counters for OperationsPanel; never exposes any body.
    pub async fn count_memory_by_state(
        &self,
        scope: evohime_local_storage::domains::memory::MemoryScope,
        scope_id: &str,
    ) -> Result<Vec<(String, i64)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::count_by_state(
            database.connection(),
            scope,
            scope_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Active records of one kind in one scope: the input for deterministic
    /// conflict detection in `memory_extraction::detect_conflict`.
    pub async fn memory_conflict_candidates(
        &self,
        scope: evohime_local_storage::domains::memory::MemoryScope,
        scope_id: &str,
        kind: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::domains::memory::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::conflict_candidates(
            database.connection(),
            scope,
            scope_id,
            kind,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Idempotent state transition. Repeated confirm/reject is safe and
    /// returns the actual current state.
    pub async fn transition_memory_state(&self, id: &str, target: &str) -> Result<String, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::transition_state(
            database.connection(),
            id,
            target,
        )
        .map_err(|error| error.to_string())
    }

    /// Replaces a pending candidate's statement with one the user wrote.
    pub async fn revise_pending_memory(&self, id: &str, statement: &str) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::revise_pending_statement(
            database.connection(),
            id,
            statement,
        )
        .map_err(|error| error.to_string())
    }

    /// Applies an explicit user choice: `old_id` is superseded by `new_id`.
    pub async fn supersede_memory(
        &self,
        old_id: &str,
        new_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::supersede(
            database.connection(),
            old_id,
            new_id,
            reason,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn memory_supersession_chain(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::supersession_chain(
            database.connection(),
            id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Marks due records `expired` so they leave retrieval without any
    /// hidden action on stale content.
    pub async fn expire_due_memory(&self, now: &str) -> Result<usize, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::expire_due(
            database.connection(),
            now,
        )
        .map_err(|error| error.to_string())
    }

    /// Logical deletion plus a tombstone that carries only metadata and a
    /// digest — never the original text.
    pub async fn forget_memory_with_tombstone(
        &self,
        id: &str,
        tombstone_id: &str,
        reason_class: &str,
        forgotten_at: &str,
    ) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::forget_with_tombstone(
            database.connection(),
            id,
            tombstone_id,
            reason_class,
            forgotten_at,
        )
        .map_err(|error| error.to_string())
    }

    /// Registered aliases for the scope, feeding
    /// `memory_extraction::AliasTable`. Model inference can never add one.
    pub async fn list_memory_aliases(
        &self,
        scope: evohime_local_storage::domains::memory::MemoryScope,
        scope_id: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::list_aliases(
            database.connection(),
            scope,
            scope_id,
        )
        .map_err(|error| error.to_string())
    }

    /// "Only for this session": a session-scoped row with automatic expiry
    /// that never becomes persistent memory.
    pub async fn save_memory_session_note(
        &self,
        note: SessionMemoryNote<'_>,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::insert_session_note(
            database.connection(),
            evohime_local_storage::domains::memory::InsertSessionNoteInput {
                id: note.id,
                session_id: note.session_id,
                scope: note.scope,
                scope_id: note.scope_id,
                kind: note.kind,
                statement: note.statement,
                created_at: note.created_at,
                expires_at: note.expires_at,
            },
        )
        .map_err(|error| error.to_string())
    }

    pub async fn list_memory_session_notes(
        &self,
        session_id: &str,
        now: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::list_session_notes(
            database.connection(),
            session_id,
            now,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn purge_expired_memory_session_notes(&self, now: &str) -> Result<usize, String> {
        let database = self.database.lock().await;
        evohime_local_storage::domains::memory::MemoryStoreSql::purge_expired_session_notes(
            database.connection(),
            now,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one bounded, redacted feedback record against the real
    /// `feedback_entries` table (SCHEMA_VERSION 14). Feedback never leaves
    /// this local table; see `evohime_local_storage::feedback_store::external_telemetry_allowed`.
    pub async fn save_feedback(
        &self,
        record: &evohime_local_storage::feedback_store::FeedbackRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::feedback_store::FeedbackStoreSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists feedback tied to one run, newest first.
    pub async fn list_feedback(
        &self,
        run_id: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::feedback_store::FeedbackRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::feedback_store::FeedbackStoreSql::list_by_run(
            database.connection(),
            run_id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Local aggregation: signal counts plus top rejection reasons/outcomes
    /// by frequency. No data leaves the local store as part of this call.
    pub async fn aggregate_feedback(
        &self,
        reason_limit: u32,
        outcome_limit: u32,
    ) -> Result<evohime_local_storage::feedback_store::FeedbackAggregate, String> {
        let database = self.database.lock().await;
        evohime_local_storage::feedback_store::FeedbackStoreSql::aggregate(
            database.connection(),
            reason_limit,
            outcome_limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Installs (inserts) or updates (replaces by id) one bounded capability
    /// manifest against the real `capability_manifests` table.
    pub async fn save_capability_manifest(
        &self,
        record: &evohime_local_storage::capability_store::CapabilityManifestRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists installed capability manifests, newest-first.
    pub async fn list_capability_manifests(
        &self,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::capability_store::CapabilityManifestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::list(
            database.connection(),
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches one installed capability manifest by id (manifest name).
    pub async fn get_capability_manifest(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::capability_store::CapabilityManifestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::get_by_id(
            database.connection(),
            id,
        )
        .map_err(|error| error.to_string())
    }

    /// Removes one installed capability manifest by id. Returns `false` if
    /// no matching row was found.
    pub async fn remove_capability_manifest(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::delete_by_id(
            database.connection(),
            id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists (upserts by task_id) the reconciled capability-selection
    /// state for a task, so the pin/replace/auto choice survives reconnect.
    pub async fn save_capability_selection(
        &self,
        record: &evohime_local_storage::capability_selection_store::CapabilitySelectionRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_selection_store::CapabilitySelectionStoreSql::upsert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches the persisted capability-selection state for a task, if any.
    pub async fn get_capability_selection(
        &self,
        task_id: &str,
    ) -> Result<
        Option<evohime_local_storage::capability_selection_store::CapabilitySelectionRecord>,
        String,
    > {
        let database = self.database.lock().await;
        evohime_local_storage::capability_selection_store::CapabilitySelectionStoreSql::get_by_task_id(
            database.connection(),
            task_id,
        )
        .map_err(|error| error.to_string())
    }
}
