//! Core runtime boundary for the durable Goal contract (plan 25.2).
//!
//! Storage owns validation and atomic persistence. Core is the only caller
//! that can create authoritative criteria evidence or change a Goal state.
//! Startup recovery is read-only and never retries an uncertain effect.

pub use evohime_local_storage::goal::{
    GoalCommand, GoalCriterionEvidence, GoalCriterionKind, GoalCriterionStatus, GoalCriterionV1,
    GoalError, GoalMutationResult, GoalProvenance, GoalRecoveryProjection, GoalStatus, GoalStore,
    GoalV1, GOAL_MAX_CRITERIA, GOAL_MAX_ID_CHARS, GOAL_MAX_READ_LIMIT, GOAL_MAX_TEXT_CHARS,
    GOAL_SCHEMA_VERSION,
};

use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct GoalRuntime {
    journal: crate::EventJournal,
}

impl GoalRuntime {
    pub fn new(journal: crate::EventJournal) -> Self {
        Self { journal }
    }

    pub async fn get(
        &self,
        goal_id: &str,
    ) -> Result<Option<GoalV1>, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        GoalStore::new(database.connection()).get(goal_id)
    }

    pub async fn list(
        &self,
        workspace_id: &str,
        limit: usize,
    ) -> Result<Vec<GoalV1>, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        GoalStore::new(database.connection()).list(workspace_id, limit)
    }

    pub async fn recovery(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<GoalRecoveryProjection>, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        let store = GoalStore::new(database.connection());
        let mut projections = store.recovery(workspace_id)?;
        let goals = store.list(workspace_id, GOAL_MAX_READ_LIMIT)?;
        for projection in &mut projections {
            let Some(goal) = goals.iter().find(|goal| goal.id == projection.goal_id) else {
                continue;
            };
            let mut warnings = Vec::new();
            if !projection.warning.is_empty() {
                warnings.push(projection.warning.clone());
            }
            for reference_id in &goal.workflow_run_ids {
                if let Some(warning) = linked_workflow_warning(database.connection(), reference_id)?
                {
                    warnings.push(warning);
                }
            }
            for reference_id in &goal.child_run_ids {
                if let Some(warning) = linked_child_warning(database.connection(), reference_id)? {
                    warnings.push(warning);
                }
            }
            if let Some(reference_id) = &goal.checkpoint_id {
                if let Some(warning) =
                    linked_checkpoint_warning(database.connection(), reference_id)?
                {
                    warnings.push(warning);
                }
            }
            projection.warning = warnings
                .join(" ")
                .chars()
                .take(GOAL_MAX_TEXT_CHARS)
                .collect();
        }
        Ok(projections)
    }

    pub async fn create(
        &self,
        goal: &GoalV1,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        GoalStore::new(database.connection()).create(goal, command)
    }

    pub async fn transition(
        &self,
        goal_id: &str,
        expected_version: u64,
        status: GoalStatus,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        GoalStore::new(database.connection()).transition(goal_id, expected_version, status, command)
    }

    pub async fn update(
        &self,
        goal_id: &str,
        expected_version: u64,
        objective: Option<String>,
        criteria: Option<Vec<GoalCriterionV1>>,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        GoalStore::new(database.connection()).update(
            goal_id,
            expected_version,
            objective,
            criteria,
            command,
        )
    }

    pub async fn verify_criterion(
        &self,
        goal_id: &str,
        expected_version: u64,
        evidence: GoalCriterionEvidence<'_>,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        GoalStore::new(database.connection()).verify_criterion(
            goal_id,
            expected_version,
            evidence,
            command,
        )
    }

    pub async fn link_reference(
        &self,
        goal_id: &str,
        expected_version: u64,
        kind: &str,
        reference_id: &str,
        command: GoalCommand<'_>,
    ) -> Result<GoalMutationResult, evohime_local_storage::StorageError> {
        let database = self.journal.database().lock().await;
        if !reference_exists(database.connection(), kind, reference_id)? {
            return Err(evohime_local_storage::StorageError::Goal(
                GoalError::ReferenceNotFound {
                    kind: kind.to_owned(),
                    reference_id: reference_id.to_owned(),
                },
            ));
        }
        GoalStore::new(database.connection()).link_reference(
            goal_id,
            expected_version,
            kind,
            reference_id,
            command,
        )
    }
}

/// A client may request a link, but only Core can attest that the referenced
/// durable runtime object exists. Missing optional tables are treated as an
/// unavailable backend, never as an implicit successful link.
fn reference_exists(
    connection: &rusqlite::Connection,
    kind: &str,
    reference_id: &str,
) -> Result<bool, evohime_local_storage::StorageError> {
    let table_exists = |table: &str| -> Result<bool, rusqlite::Error> {
        Ok(connection
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .is_some())
    };
    let exists_in =
        |table: &str, column: &str| -> Result<bool, evohime_local_storage::StorageError> {
            if !table_exists(table)? {
                return Ok(false);
            }
            let query = format!("SELECT 1 FROM {table} WHERE {column} = ?1 LIMIT 1");
            Ok(connection
                .query_row(&query, [reference_id], |row| row.get::<_, i64>(0))
                .optional()?
                .is_some())
        };
    match kind {
        "workflow" => exists_in("workflow_runs", "run_id"),
        "child" => Ok(exists_in("child_task_requests", "child_task_id")?
            || exists_in("child_reports", "child_task_id")?
            || exists_in("coordinator_child_checkpoint", "child_task_id")?),
        "checkpoint" => exists_in("task_checkpoints", "id"),
        _ => Ok(false),
    }
}

fn linked_workflow_warning(
    connection: &rusqlite::Connection,
    reference_id: &str,
) -> Result<Option<String>, evohime_local_storage::StorageError> {
    if !reference_exists(connection, "workflow", reference_id)? {
        return Ok(Some(format!(
            "Workflow {reference_id} отсутствует; автоматическое продолжение запрещено."
        )));
    }
    let run_state: Option<String> = connection
        .query_row(
            "SELECT state FROM workflow_runs WHERE run_id = ?1",
            [reference_id],
            |row| row.get(0),
        )
        .optional()?;
    if run_state.as_deref() == Some("interrupted") {
        return Ok(Some(format!(
            "Workflow {reference_id} прерван; требуется reconciliation, blind retry запрещён."
        )));
    }
    let unknown_outcome: Option<String> = connection
        .query_row(
            "SELECT node_id FROM workflow_run_nodes
             WHERE run_id = ?1 AND state = 'unknown_outcome' LIMIT 1",
            [reference_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(unknown_outcome.map(|node_id| {
        format!(
            "Workflow {reference_id} содержит unknown outcome узла {node_id}; blind retry запрещён."
        )
    }))
}

fn linked_child_warning(
    connection: &rusqlite::Connection,
    reference_id: &str,
) -> Result<Option<String>, evohime_local_storage::StorageError> {
    if !reference_exists(connection, "child", reference_id)? {
        return Ok(Some(format!(
            "Child run {reference_id} отсутствует; автоматическое продолжение запрещено."
        )));
    }
    if !table_exists(connection, "coordinator_child_checkpoint")? {
        return Ok(None);
    }
    let checkpoint: Option<(String, i64)> = connection
        .query_row(
            "SELECT state, dead_letter FROM coordinator_child_checkpoint
             WHERE child_task_id = ?1 ORDER BY revision DESC LIMIT 1",
            [reference_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(checkpoint.and_then(|(state, dead_letter)| {
        (dead_letter != 0 || matches!(state.as_str(), "failed" | "timed_out"))
            .then(|| format!("Child run {reference_id} требует reconciliation: {state}."))
    }))
}

fn linked_checkpoint_warning(
    connection: &rusqlite::Connection,
    reference_id: &str,
) -> Result<Option<String>, evohime_local_storage::StorageError> {
    if !reference_exists(connection, "checkpoint", reference_id)? {
        return Ok(Some(format!(
            "Checkpoint {reference_id} отсутствует; автоматическое продолжение запрещено."
        )));
    }
    Ok(None)
}

fn table_exists(
    connection: &rusqlite::Connection,
    table: &str,
) -> Result<bool, evohime_local_storage::StorageError> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some())
}

/// Stable workspace scope used by the Goal contract. The path itself remains
/// a shell concern and is never persisted in a Goal or sent in a projection.
pub fn workspace_id_from_path(path: &str) -> String {
    let normalized = path.trim().replace('\\', "/").to_ascii_lowercase();
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_scope_is_stable_without_exposing_the_path() {
        let first = workspace_id_from_path(r"C:\Repo\EvoHime");
        assert_eq!(first, workspace_id_from_path("c:/repo/evohime"));
        assert_eq!(first.len(), 64);
        assert!(!first.contains("repo"));
    }

    #[tokio::test]
    async fn recovery_reports_missing_link_without_retrying_any_effect() {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = crate::EventJournal::open(directory.path().join("goal-recovery.db"))
            .expect("journal opens");
        let runtime = GoalRuntime::new(journal);
        let now = now_ms();
        let goal = GoalV1 {
            id: "goal-recovery".into(),
            version: 1,
            workspace_id: workspace_id_from_path("C:/workspace"),
            chat_id: None,
            objective: "Восстановить цель".into(),
            success_criteria: vec![GoalCriterionV1::new(
                "criterion-1",
                GoalCriterionKind::Manual,
                "Подтвердить результат",
            )],
            status: GoalStatus::Active,
            progress_summary: "Ожидает проверки".into(),
            completed_criteria: Vec::new(),
            remaining_criteria: Vec::new(),
            blockers: Vec::new(),
            next_action: Some("Проверить".into()),
            workflow_run_ids: Vec::new(),
            child_run_ids: Vec::new(),
            checkpoint_id: Some("missing-checkpoint".into()),
            token_budget: None,
            cost_budget_micros: None,
            continuation_budget: None,
            created_at_ms: now,
            updated_at_ms: now,
            created_by: "core".into(),
            updated_by: "core".into(),
            content_hash: String::new(),
        };
        runtime
            .create(
                &goal,
                GoalCommand::new("core", "goal-recovery-create", &"a".repeat(64)),
            )
            .await
            .expect("goal persists");

        let recovery = runtime
            .recovery(&goal.workspace_id)
            .await
            .expect("recovery reads");
        assert_eq!(recovery.len(), 1);
        assert!(recovery[0]
            .warning
            .contains("Checkpoint missing-checkpoint"));
        assert!(recovery[0].warning.contains("запрещено"));
    }
}
