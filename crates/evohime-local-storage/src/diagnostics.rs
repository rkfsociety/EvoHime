use crate::{
    DiagnosticsEventCount, DiagnosticsSummary, DiagnosticsTableCount, LocalDatabase,
    RecoveryHealthSnapshot, StorageError, MAX_DIAGNOSTICS_EVENT_TYPES,
};

impl LocalDatabase {
    /// Returns a bounded, read-only health/retention summary.
    ///
    /// The table list is fixed to the schema owned by this crate, while event
    /// types are capped so a noisy database cannot produce an unbounded
    /// response. This method performs only SELECTs and does not affect
    /// recovery state or retention data.
    pub fn read_diagnostics_summary(
        &self,
        max_event_types: usize,
    ) -> Result<DiagnosticsSummary, StorageError> {
        const TABLES: [&str; 24] = [
            "events",
            "projects",
            "work_items",
            "work_item_edges",
            "provenance",
            "runs",
            "command_dedup",
            "snapshots",
            "run_checkpoints",
            "run_effects",
            "project_policies",
            "run_leases",
            "run_reconciliations",
            "run_recovery",
            "agent_run_effects",
            "agent_run_leases",
            "agent_run_reconciliations",
            "agent_run_recovery",
            "workspace_index_runs",
            "workspace_documents",
            "document_chunks",
            "workspace_vector_indexes",
            "workspace_chunk_vectors",
            "rag_context_ledger",
        ];

        let mut table_counts = Vec::with_capacity(TABLES.len());
        for table in TABLES {
            let sql = format!("SELECT COUNT(*) FROM {table}");
            let rows = self.connection.query_row(&sql, [], |row| row.get(0))?;
            table_counts.push(DiagnosticsTableCount {
                table: table.to_string(),
                rows,
            });
        }

        let total_events = self
            .connection
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        let limit = max_event_types.min(MAX_DIAGNOSTICS_EVENT_TYPES);
        let query_limit = limit.saturating_add(1).min(i64::MAX as usize) as i64;
        let mut statement = self.connection.prepare(
            "SELECT event_type, COUNT(*) AS rows
             FROM events GROUP BY event_type
             ORDER BY rows DESC, event_type ASC LIMIT ?1",
        )?;
        let rows = statement.query_map([query_limit], |row| {
            Ok(DiagnosticsEventCount {
                event_type: row.get(0)?,
                rows: row.get(1)?,
            })
        })?;
        let mut event_counts = rows.collect::<Result<Vec<_>, _>>()?;
        let event_types_truncated = event_counts.len() > limit;
        event_counts.truncate(limit);

        Ok(DiagnosticsSummary {
            table_counts,
            event_counts,
            total_events,
            event_types_truncated,
        })
    }

    /// Returns a bounded, read-only summary of recovery-relevant state.
    ///
    /// This performs only SELECTs; it does not transition run/effect state.
    /// Use `recover_unknown_effects` for the mutating recovery flow.
    pub fn read_recovery_health(&self) -> Result<RecoveryHealthSnapshot, StorageError> {
        let unknown_effects: i64 = self.connection.query_row(
            "SELECT
                 (SELECT COUNT(*) FROM run_effects WHERE state = 'unknown') +
                 (SELECT COUNT(*) FROM agent_run_effects WHERE state = 'unknown')",
            [],
            |row| row.get(0),
        )?;
        let lease_expired: bool = self.connection.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM run_leases
                 WHERE lease_expires_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             ) OR EXISTS(
                 SELECT 1 FROM agent_run_leases
                 WHERE lease_expires_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             )",
            [],
            |row| row.get(0),
        )?;
        let resumable_runs: i64 = self.connection.query_row(
            "SELECT
                 (SELECT COUNT(*) FROM runs WHERE status = 'blocked') +
                 (SELECT COUNT(*) FROM agent_run_recovery
                  WHERE state IN ('RESUMABLE', 'BLOCKED') AND id IN (
                      SELECT MAX(id) FROM agent_run_recovery GROUP BY run_id
                  ))",
            [],
            |row| row.get(0),
        )?;
        Ok(RecoveryHealthSnapshot {
            unknown_effects,
            lease_expired,
            resumable_runs,
        })
    }
}
