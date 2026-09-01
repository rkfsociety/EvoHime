//! Core runtime facade for the durable Plan Artifact contract.
//! All mutations pass through the storage authority and publish bounded events.

pub use evohime_local_storage::plan_artifact::{
    AcceptanceCriterion, PlanArtifactError, PlanArtifactStatus, PlanArtifactV1,
    PlanExecutionSnapshot, PlanProvenance, PlanStep, MAX_ARTIFACT_BYTES, MAX_CRITERIA, MAX_REFS,
    MAX_STEPS, MAX_TEXT_CHARS, PLAN_ARTIFACT_SCHEMA_VERSION,
};

#[derive(Clone)]
pub struct PlanArtifactRuntime {
    journal: crate::EventJournal,
}

impl PlanArtifactRuntime {
    pub fn new(journal: crate::EventJournal) -> Self {
        Self { journal }
    }

    pub async fn get(
        &self,
        artifact_id: &str,
    ) -> Result<Option<PlanArtifactV1>, PlanArtifactError> {
        let database = self.journal.database().lock().await;
        evohime_local_storage::plan_artifact::PlanArtifactStore::new(database.connection())
            .get(artifact_id)
    }

    pub async fn create(
        &self,
        artifact: &PlanArtifactV1,
        idempotency_key: &str,
        now_ms: i64,
    ) -> Result<PlanArtifactV1, PlanArtifactError> {
        let database = self.journal.database().lock().await;
        let value =
            evohime_local_storage::plan_artifact::PlanArtifactStore::new(database.connection())
                .create(artifact, idempotency_key, now_ms)?;
        let payload = serde_json::to_vec(&serde_json::json!({"artifact_id":value.id,"revision":value.revision,"status":value.status.as_str(),"content_hash":value.content_hash})).map_err(|e|PlanArtifactError::Invalid(e.to_string()))?;
        database
            .append_event(&value.id, "plan_artifact.created", &payload)
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(value)
    }

    pub async fn transition(
        &self,
        artifact_id: &str,
        expected_version: u64,
        status: PlanArtifactStatus,
        idempotency_key: &str,
        now_ms: i64,
    ) -> Result<PlanArtifactV1, PlanArtifactError> {
        let database = self.journal.database().lock().await;
        let value =
            evohime_local_storage::plan_artifact::PlanArtifactStore::new(database.connection())
                .transition(
                    artifact_id,
                    expected_version,
                    status,
                    idempotency_key,
                    now_ms,
                )?;
        let payload = serde_json::to_vec(&serde_json::json!({"artifact_id":value.id,"revision":value.revision,"status":value.status.as_str(),"content_hash":value.content_hash})).map_err(|e|PlanArtifactError::Invalid(e.to_string()))?;
        database
            .append_event(&value.id, "plan_artifact.transitioned", &payload)
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(value)
    }

    pub async fn execute(
        &self,
        artifact_id: &str,
        expected_version: u64,
        policy_snapshot_hash: &str,
        task_id: Option<&str>,
        workflow_run_id: Option<&str>,
        correlation_id: &str,
        idempotency_key: &str,
        now_ms: i64,
    ) -> Result<PlanExecutionSnapshot, PlanArtifactError> {
        let database = self.journal.database().lock().await;
        let snapshot =
            evohime_local_storage::plan_artifact::PlanArtifactStore::new(database.connection())
                .create_execution_snapshot(
                    artifact_id,
                    expected_version,
                    policy_snapshot_hash,
                    task_id,
                    workflow_run_id,
                    correlation_id,
                    idempotency_key,
                    now_ms,
                )?;
        let payload = serde_json::to_vec(&serde_json::json!({"artifact_id":snapshot.artifact_id,"revision":snapshot.revision,"content_hash":snapshot.content_hash,"policy_snapshot_hash":snapshot.policy_snapshot_hash,"correlation_id":snapshot.correlation_id})).map_err(|e|PlanArtifactError::Invalid(e.to_string()))?;
        database
            .append_event(artifact_id, "plan_artifact.execution_snapshot", &payload)
            .map_err(|e| PlanArtifactError::Invalid(e.to_string()))?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn runtime_persists_and_pins_execution_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = PlanArtifactRuntime::new(
            crate::EventJournal::open(dir.path().join("plan.db")).unwrap(),
        );
        let artifact = PlanArtifactV1 {
            schema_version: 1,
            id: "runtime-plan".into(),
            revision: 1,
            version: 1,
            status: PlanArtifactStatus::Draft,
            title: "Plan".into(),
            objective: "Objective".into(),
            steps: vec![PlanStep {
                id: "step".into(),
                description: "bounded operation".into(),
                capability_ref: None,
                risk: "low".into(),
            }],
            assumptions: vec![],
            risks: vec![],
            acceptance_criteria: vec![AcceptanceCriterion {
                id: "criterion".into(),
                description: "test".into(),
                evidence_kind: "TestsPass".into(),
                required: true,
            }],
            references: vec![],
            provenance: PlanProvenance {
                actor: "core".into(),
                request_id: "request".into(),
                correlation_id: "correlation".into(),
            },
            content_hash: String::new(),
        };
        let accepted = runtime.create(&artifact, "create", 1).await.unwrap();
        let accepted = runtime
            .transition(&accepted.id, 1, PlanArtifactStatus::Accepted, "accept", 2)
            .await
            .unwrap();
        let snapshot = runtime
            .execute(
                &accepted.id,
                2,
                "policy-hash",
                Some("task"),
                None,
                "correlation",
                "execute",
                3,
            )
            .await
            .unwrap();
        assert_eq!(snapshot.revision, 3);
        assert_eq!(
            runtime.get("runtime-plan").await.unwrap().unwrap().status,
            PlanArtifactStatus::Executing
        );
    }
}
