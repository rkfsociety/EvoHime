//! Versioned Core-owned contract for repeatable automation triggers (plan 16.1).
//!
//! This contract deliberately does not contain an executor or a scheduler.  A
//! definition is immutable input; the runtime binds a run to its revision and
//! to immutable policy/approval snapshots before it can execute any effect.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const AUTOMATION_CONTRACT_VERSION: &str = "automation/v1";
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_GRAPH_ACTIVITIES: usize = 64;
pub const MAX_INPUT_BYTES: usize = 64 * 1024;
pub const MAX_HISTORY_EVENTS: usize = 256;
pub const MAX_PARALLELISM: u32 = 8;
pub const IDEMPOTENCY_RETENTION_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationDefinitionV1 {
    pub contract: String,
    pub definition_id: String,
    pub revision: u64,
    pub owner_scope: String,
    pub graph_ref: String,
    pub activities: Vec<ActivityRef>,
    pub trigger_policy: TriggerPolicy,
    pub concurrency: ConcurrencyPolicy,
    pub retry: RetryPolicy,
    pub capabilities: Vec<String>,
    pub approval_mode: ApprovalMode,
    pub input_schema: String,
    pub retention_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityRef {
    pub activity_id: String,
    pub block_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerPolicy {
    pub manual: bool,
    pub schedule: Option<String>,
    pub trigger_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcurrencyPolicy {
    pub max_concurrent: u32,
    pub queue_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub retryable_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Never,
    OnEffect,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomationValidationError {
    UnsupportedContract,
    Empty(&'static str),
    Limit(&'static str),
    Invalid(&'static str),
    UnknownMajor,
}

impl std::fmt::Display for AutomationValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for AutomationValidationError {}

impl AutomationDefinitionV1 {
    pub fn validate(&self) -> Result<(), AutomationValidationError> {
        if self.contract != AUTOMATION_CONTRACT_VERSION {
            return Err(AutomationValidationError::UnsupportedContract);
        }
        for (name, value) in [
            ("definition_id", &self.definition_id),
            ("owner_scope", &self.owner_scope),
            ("graph_ref", &self.graph_ref),
            ("input_schema", &self.input_schema),
        ] {
            if value.is_empty() {
                return Err(AutomationValidationError::Empty(name));
            }
            if value.len() > MAX_INPUT_BYTES {
                return Err(AutomationValidationError::Limit(name));
            }
        }
        if self.revision == 0 {
            return Err(AutomationValidationError::Invalid("revision"));
        }
        if self.activities.is_empty() || self.activities.len() > MAX_GRAPH_ACTIVITIES {
            return Err(AutomationValidationError::Limit("activities"));
        }
        if self.activities.iter().any(|a| {
            a.activity_id.is_empty()
                || a.activity_id.len() > MAX_ID_BYTES
                || a.block_ref.is_empty()
                || a.block_ref.len() > MAX_ID_BYTES
        }) {
            return Err(AutomationValidationError::Invalid("activity"));
        }
        if self
            .capabilities
            .iter()
            .any(|v| v.is_empty() || v.len() > MAX_ID_BYTES)
        {
            return Err(AutomationValidationError::Invalid("capability"));
        }
        if self.concurrency.max_concurrent == 0
            || self.concurrency.max_concurrent > MAX_PARALLELISM
            || self.concurrency.queue_limit > 256
        {
            return Err(AutomationValidationError::Limit("concurrency"));
        }
        if self.retry.max_attempts > 2 {
            return Err(AutomationValidationError::Limit("retry"));
        }
        if self.retention_days == 0 || self.retention_days > 365 {
            return Err(AutomationValidationError::Limit("retention_days"));
        }
        if self
            .trigger_policy
            .trigger_keys
            .iter()
            .any(|v| v.is_empty() || v.len() > MAX_ID_BYTES)
        {
            return Err(AutomationValidationError::Invalid("trigger_key"));
        }
        Ok(())
    }
    pub fn canonical_hash(&self) -> Result<String, AutomationValidationError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| AutomationValidationError::Invalid("definition"))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerRequestV1 {
    pub owner_scope: String,
    pub definition_id: String,
    pub revision: u64,
    pub trigger_key: String,
    pub scheduled_slot: Option<String>,
    pub input_json: String,
    pub correlation_id: String,
    pub idempotency_key: String,
}

impl TriggerRequestV1 {
    pub fn validate(&self) -> Result<(), AutomationValidationError> {
        for (name, value) in [
            ("owner_scope", &self.owner_scope),
            ("definition_id", &self.definition_id),
            ("trigger_key", &self.trigger_key),
            ("correlation_id", &self.correlation_id),
            ("idempotency_key", &self.idempotency_key),
        ] {
            if value.is_empty() {
                return Err(AutomationValidationError::Empty(name));
            }
            if value.len() > MAX_ID_BYTES {
                return Err(AutomationValidationError::Limit(name));
            }
        }
        if self.input_json.len() > MAX_INPUT_BYTES {
            return Err(AutomationValidationError::Limit("input_json"));
        }
        if self.revision == 0 {
            return Err(AutomationValidationError::Invalid("revision"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunState {
    Admitted,
    Queued,
    Starting,
    Running,
    WaitingApproval,
    Paused,
    Retrying,
    Cancelling,
    Completed,
    Failed,
    Cancelled,
    DeadLetter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationRunV1 {
    pub run_id: String,
    pub request: TriggerRequestV1,
    pub definition_hash: String,
    pub permission_snapshot: String,
    pub approval_snapshot: String,
    pub generation: u64,
    pub state: AutomationRunState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEventV1 {
    pub event_id: String,
    pub run_id: String,
    pub generation: u64,
    pub sequence: u64,
    pub activity_id: String,
    pub attempt: u32,
    pub outcome: String,
    pub diagnostics: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationHealthV1 {
    pub definition_id: String,
    pub revision: u64,
    pub active_runs: u32,
    pub queued_runs: u32,
    pub last_terminal_state: Option<AutomationRunState>,
    pub last_error_code: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> AutomationDefinitionV1 {
        AutomationDefinitionV1 {
            contract: AUTOMATION_CONTRACT_VERSION.into(),
            definition_id: "daily.sync".into(),
            revision: 1,
            owner_scope: "owner".into(),
            graph_ref: "workflow:sync".into(),
            activities: vec![ActivityRef {
                activity_id: "sync".into(),
                block_ref: "workspace.read".into(),
            }],
            trigger_policy: TriggerPolicy {
                manual: true,
                schedule: None,
                trigger_keys: vec!["manual".into()],
            },
            concurrency: ConcurrencyPolicy {
                max_concurrent: 1,
                queue_limit: 8,
            },
            retry: RetryPolicy {
                max_attempts: 2,
                retryable_errors: vec!["provider_timeout".into()],
            },
            capabilities: vec!["workspace.read".into()],
            approval_mode: ApprovalMode::OnEffect,
            input_schema: "{}".into(),
            retention_days: 30,
        }
    }
    #[test]
    fn validates_and_hashes_stably() {
        let d = definition();
        assert!(d.validate().is_ok());
        assert_eq!(d.canonical_hash().unwrap(), d.canonical_hash().unwrap());
    }
    #[test]
    fn rejects_unsafe_retry_and_unknown_contract() {
        let mut d = definition();
        d.retry.max_attempts = 3;
        assert!(matches!(
            d.validate(),
            Err(AutomationValidationError::Limit("retry"))
        ));
        d.contract = "automation/v9".into();
        assert!(matches!(
            d.validate(),
            Err(AutomationValidationError::UnsupportedContract)
        ));
    }
    #[test]
    fn trigger_requires_bounded_identity_and_input() {
        let mut r = TriggerRequestV1 {
            owner_scope: "o".into(),
            definition_id: "d".into(),
            revision: 1,
            trigger_key: "k".into(),
            scheduled_slot: None,
            input_json: "{}".into(),
            correlation_id: "c".into(),
            idempotency_key: "i".into(),
        };
        assert!(r.validate().is_ok());
        r.input_json = "x".repeat(MAX_INPUT_BYTES + 1);
        assert!(matches!(
            r.validate(),
            Err(AutomationValidationError::Limit("input_json"))
        ));
    }
}
