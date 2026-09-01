//! Core-owned Incremental Change Protocol (plan 59), metadata-only vertical slice.

use evohime_local_storage::{
    incremental_change_protocol_store::{self, IncrementalChangeRunRecord},
    plan_artifact::PlanArtifactStore,
    workspace_state_checkpoint,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 32;
pub const MAX_TEXT: usize = 512;
pub const MAX_JSON: usize = incremental_change_protocol_store::MAX_JSON_BYTES;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementDelta {
    pub requirements: Vec<String>,
    pub non_goals: Vec<String>,
    pub evidence_refs: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactAnalysis {
    pub affected_plan_items: Vec<String>,
    pub risk: String,
    pub scope_fingerprint: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePlan {
    pub plan_artifact_id: String,
    pub plan_revision: u64,
    pub plan_content_hash: String,
    pub checkpoint_id: String,
    pub checkpoint_snapshot_hash: String,
    pub items: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Planned,
    Applying,
    Applied,
    Stale,
    Cancelled,
    UnknownReconciliationRequired,
}
impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Applying => "applying",
            Self::Applied => "applied",
            Self::Stale => "stale",
            Self::Cancelled => "cancelled",
            Self::UnknownReconciliationRequired => "unknown_reconciliation_required",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("incremental change input is invalid: {0}")]
    Invalid(String),
    #[error("incremental change run was not found")]
    NotFound,
    #[error("incremental change run is stale or has a version conflict")]
    Stale,
    #[error("incremental change run is already terminal")]
    Terminal,
    #[error("storage error: {0}")]
    Storage(String),
}

#[derive(Clone)]
pub struct Runtime {
    journal: crate::EventJournal,
}
impl Runtime {
    pub fn new(journal: crate::EventJournal) -> Self {
        Self { journal }
    }
    pub async fn create(
        &self,
        run_id: &str,
        idempotency_key: &str,
        delta: &RequirementDelta,
        impact: &ImpactAnalysis,
        plan: &ChangePlan,
        now_ms: i64,
    ) -> Result<IncrementalChangeRunRecord, Error> {
        validate(delta, impact, plan)?;
        let database = self.journal.database().lock().await;
        let artifact = PlanArtifactStore::new(database.connection())
            .get(&plan.plan_artifact_id)
            .map_err(|e| Error::Storage(e.to_string()))?
            .ok_or_else(|| Error::Invalid("plan artifact reference is missing".into()))?;
        if artifact.revision != plan.plan_revision
            || artifact.content_hash != plan.plan_content_hash
        {
            return Err(Error::Stale);
        }
        let checkpoint =
            workspace_state_checkpoint::get_checkpoint(database.connection(), &plan.checkpoint_id)
                .map_err(|e| Error::Storage(e.to_string()))?
                .ok_or_else(|| {
                    Error::Invalid("workspace checkpoint reference is missing".into())
                })?;
        if checkpoint.snapshot_hash != plan.checkpoint_snapshot_hash {
            return Err(Error::Stale);
        }
        let record = IncrementalChangeRunRecord {
            run_id: run_id.into(),
            version: 1,
            state: State::Planned.as_str().into(),
            plan_artifact_id: plan.plan_artifact_id.clone(),
            plan_revision: plan.plan_revision,
            plan_content_hash: plan.plan_content_hash.clone(),
            checkpoint_id: plan.checkpoint_id.clone(),
            checkpoint_snapshot_hash: plan.checkpoint_snapshot_hash.clone(),
            baseline_fingerprint: impact.scope_fingerprint.clone(),
            impact_json: bounded_json(delta)?,
            change_plan_json: bounded_json(plan)?,
            evidence_json: bounded_json(
                &serde_json::json!({"schema_version": SCHEMA_VERSION, "redacted": true}),
            )?,
            idempotency_key: idempotency_key.into(),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        if let Some(existing) = incremental_change_protocol_store::get_by_idempotency(
            database.connection(),
            idempotency_key,
        )
        .map_err(|e| Error::Storage(e.to_string()))?
        {
            if existing.run_id != run_id {
                return Err(Error::Invalid(
                    "idempotency key belongs to another run".into(),
                ));
            }
            return Ok(existing);
        }
        incremental_change_protocol_store::create(database.connection(), &record)
            .map_err(|e| Error::Storage(e.to_string()))?;
        database
            .append_event(
                run_id,
                "incremental_change.created",
                &serde_json::to_vec(&projection(&record)).unwrap(),
            )
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(
            incremental_change_protocol_store::get(database.connection(), run_id)
                .map_err(|e| Error::Storage(e.to_string()))?
                .unwrap_or(record),
        )
    }
    pub async fn transition(
        &self,
        run_id: &str,
        expected_version: u64,
        next: State,
        observed_fingerprint: &str,
        now_ms: i64,
    ) -> Result<IncrementalChangeRunRecord, Error> {
        let database = self.journal.database().lock().await;
        let current = incremental_change_protocol_store::get(database.connection(), run_id)
            .map_err(|e| Error::Storage(e.to_string()))?
            .ok_or(Error::NotFound)?;
        if matches!(
            current.state.as_str(),
            "applied" | "cancelled" | "unknown_reconciliation_required"
        ) {
            return Err(Error::Terminal);
        }
        if next == State::Applied && current.baseline_fingerprint != observed_fingerprint {
            let _ = incremental_change_protocol_store::transition(
                database.connection(),
                run_id,
                expected_version,
                State::Stale.as_str(),
                observed_fingerprint,
                br#"{"error_code":"scope_drift","redacted":true}"#,
                now_ms,
            );
            return Err(Error::Stale);
        }
        let evidence = serde_json::to_vec(&serde_json::json!({"schema_version": SCHEMA_VERSION, "state": next.as_str(), "redacted": true})).unwrap();
        if !incremental_change_protocol_store::transition(
            database.connection(),
            run_id,
            expected_version,
            next.as_str(),
            observed_fingerprint,
            &evidence,
            now_ms,
        )
        .map_err(|e| Error::Storage(e.to_string()))?
        {
            return Err(Error::Stale);
        }
        let updated = incremental_change_protocol_store::get(database.connection(), run_id)
            .map_err(|e| Error::Storage(e.to_string()))?
            .ok_or(Error::NotFound)?;
        database
            .append_event(
                run_id,
                "incremental_change.transitioned",
                &serde_json::to_vec(&projection(&updated)).unwrap(),
            )
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(updated)
    }
}
fn bounded_json<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let bytes = serde_json::to_vec(value).map_err(|e| Error::Invalid(e.to_string()))?;
    if bytes.len() > MAX_JSON {
        return Err(Error::Invalid("bounded JSON limit exceeded".into()));
    }
    Ok(bytes)
}
fn validate(
    delta: &RequirementDelta,
    impact: &ImpactAnalysis,
    plan: &ChangePlan,
) -> Result<(), Error> {
    if delta.requirements.is_empty()
        || delta.requirements.len() > MAX_ITEMS
        || delta.non_goals.len() > MAX_ITEMS
        || delta.evidence_refs.len() > MAX_ITEMS
        || impact.affected_plan_items.len() > MAX_ITEMS
        || plan.items.is_empty()
        || plan.items.len() > MAX_ITEMS
    {
        return Err(Error::Invalid("item limit exceeded".into()));
    }
    for value in delta
        .requirements
        .iter()
        .chain(delta.non_goals.iter())
        .chain(delta.evidence_refs.iter())
        .chain(impact.affected_plan_items.iter())
        .chain(plan.items.iter())
    {
        if value.is_empty()
            || value.len() > MAX_TEXT
            || value.contains('\0')
            || value.contains("../")
        {
            return Err(Error::Invalid("invalid bounded text".into()));
        }
    }
    if impact.scope_fingerprint.is_empty()
        || impact.scope_fingerprint.len() > MAX_TEXT
        || plan.plan_artifact_id.is_empty()
        || plan.checkpoint_id.is_empty()
        || plan.plan_content_hash.len() != 64
        || plan.checkpoint_snapshot_hash.len() != 64
        || !plan
            .plan_content_hash
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        || !plan
            .checkpoint_snapshot_hash
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        || impact.risk.len() > MAX_TEXT
    {
        return Err(Error::Invalid("invalid reference or hash".into()));
    }
    Ok(())
}
fn projection(record: &IncrementalChangeRunRecord) -> serde_json::Value {
    serde_json::json!({"schema_version": SCHEMA_VERSION, "run_id": record.run_id, "version": record.version, "state": record.state, "plan_artifact_id": record.plan_artifact_id, "plan_revision": record.plan_revision, "plan_content_hash": record.plan_content_hash, "checkpoint_id": record.checkpoint_id, "checkpoint_snapshot_hash": record.checkpoint_snapshot_hash, "baseline_fingerprint": record.baseline_fingerprint, "redacted": true})
}
pub fn fingerprint<T: Serialize>(value: &T) -> Result<String, Error> {
    Ok(hex::encode(Sha256::digest(bounded_json(value)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ChangePlan {
        ChangePlan {
            plan_artifact_id: "artifact".into(),
            plan_revision: 1,
            plan_content_hash: "a".repeat(64),
            checkpoint_id: "checkpoint".into(),
            checkpoint_snapshot_hash: "b".repeat(64),
            items: vec!["item-1".into()],
        }
    }
    #[test]
    fn fingerprint_is_deterministic_and_bounded() {
        let value = RequirementDelta {
            requirements: vec!["keep".into()],
            non_goals: vec![],
            evidence_refs: vec![],
        };
        assert_eq!(fingerprint(&value).unwrap(), fingerprint(&value).unwrap());
        assert!(validate(
            &value,
            &ImpactAnalysis {
                affected_plan_items: vec!["item-1".into()],
                risk: "low".into(),
                scope_fingerprint: "scope".into()
            },
            &plan()
        )
        .is_ok());
    }
    #[test]
    fn traversal_and_oversized_payloads_are_rejected() {
        let value = RequirementDelta {
            requirements: vec!["../escape".into()],
            non_goals: vec![],
            evidence_refs: vec![],
        };
        assert!(validate(
            &value,
            &ImpactAnalysis {
                affected_plan_items: vec![],
                risk: "low".into(),
                scope_fingerprint: "scope".into()
            },
            &plan()
        )
        .is_err());
    }
}
