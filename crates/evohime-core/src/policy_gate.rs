//! Единственная Core-owned проверка перед внешним эффектом.
//!
//! Gate не принимает решений из renderer/model hints. Он сравнивает только
//! immutable snapshot, canonical call и текущий typed policy decision.

use evohime_receipts::capability::{CapabilitySnapshotV1, PolicyDecision, PolicyOutcome};
use evohime_receipts::runtime::canonical_call_hash;
use serde_json::Value;

pub fn default_snapshot(
    action_id: uuid::Uuid,
    task_id: uuid::Uuid,
    session_id: Option<uuid::Uuid>,
    tool_name: &str,
    scope: &str,
) -> Result<CapabilitySnapshotV1, String> {
    use evohime_receipts::capability::CapabilityLimits;
    CapabilitySnapshotV1 {
        snapshot_id: format!("snapshot:{action_id}"),
        run_id: format!("run:{task_id}"),
        session_id: session_id.map_or_else(|| "session:anonymous".into(), |id| id.to_string()),
        task_id: format!("task:{task_id}"),
        parent_snapshot_hash: None,
        policy_id: "policy:tool-v1".into(),
        policy_version: 1,
        policy_hash: evohime_receipts::sha256_hex(b"policy:tool-v1"),
        manifest_hash: evohime_receipts::sha256_hex(tool_name.as_bytes()),
        workspace_anchors: vec![format!("scope:{scope}")],
        operation_scopes: vec![scope.into()],
        permissions: vec!["permission-v1".into()],
        tool_identities: vec![tool_name.into()],
        network_routes: vec![],
        adapter_scopes: vec![],
        secret_refs: vec![],
        limits: CapabilityLimits {
            timeout_ms: 30_000,
            input_bytes: 256 * 1024,
            output_bytes: 512 * 1024,
            concurrency: 1,
            tool_calls: 1,
            token_budget: 0,
            cost_micros: 0,
        },
        snapshot_hash: String::new(),
    }
    .finalize()
    .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectBinding {
    pub action_id: String,
    pub tool_name: String,
    pub normalized_scope: String,
    pub input_hash: String,
    pub snapshot_hash: String,
    pub policy_version: u32,
}

pub const HOOK_CHAIN_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookMetadata {
    pub hook_chain_version: u32,
    pub action_id: String,
    pub tool_name: String,
    pub input_hash: String,
    pub snapshot_hash: String,
    pub outcome: Option<String>,
}

/// Bounded Core-owned hooks. They receive hashes and typed outcome metadata,
/// never raw input, preview text or secret values.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyHooks;

impl PolicyHooks {
    pub fn preflight(&self, binding: &EffectBinding) -> HookMetadata {
        HookMetadata {
            hook_chain_version: HOOK_CHAIN_VERSION,
            action_id: binding.action_id.clone(),
            tool_name: binding.tool_name.clone(),
            input_hash: binding.input_hash.clone(),
            snapshot_hash: binding.snapshot_hash.clone(),
            outcome: None,
        }
    }

    pub fn postflight(&self, binding: &EffectBinding, outcome: &str) -> HookMetadata {
        let mut metadata = self.preflight(binding);
        metadata.outcome = Some(outcome.to_owned());
        metadata
    }
}

#[derive(Debug, Clone)]
pub struct PolicyGate {
    snapshot: CapabilitySnapshotV1,
}

impl PolicyGate {
    pub fn new(snapshot: CapabilitySnapshotV1) -> Result<Self, PolicyDecision> {
        if snapshot.validate().is_err()
            || snapshot.compute_hash().ok().as_deref() != Some(&snapshot.snapshot_hash)
        {
            return Err(
                PolicyDecision::new(PolicyOutcome::PolicyError, "snapshot_invalid")
                    .expect("bounded reason"),
            );
        }
        Ok(Self { snapshot })
    }

    pub fn preflight(
        &self,
        action_id: &str,
        tool_name: &str,
        scope: &str,
        input: &Value,
        current: PolicyOutcome,
    ) -> Result<EffectBinding, PolicyDecision> {
        let input_hash = canonical_call_hash(tool_name, scope, input).map_err(|_| {
            PolicyDecision::new(PolicyOutcome::PolicyError, "input_invalid")
                .expect("bounded reason")
        })?;
        if !matches!(
            current,
            PolicyOutcome::Allowed | PolicyOutcome::ApprovalRequired
        ) {
            return Err(PolicyDecision::new(current, "current_policy").expect("bounded reason"));
        }
        Ok(EffectBinding {
            action_id: action_id.to_owned(),
            tool_name: tool_name.to_owned(),
            normalized_scope: scope.to_owned(),
            input_hash,
            snapshot_hash: self.snapshot.snapshot_hash.clone(),
            policy_version: self.snapshot.policy_version,
        })
    }

    pub fn recheck_before_effect(
        &self,
        binding: &EffectBinding,
        tool_name: &str,
        scope: &str,
        input: &Value,
        current: PolicyOutcome,
    ) -> Result<(), PolicyDecision> {
        let actual = canonical_call_hash(tool_name, scope, input).map_err(|_| {
            PolicyDecision::new(PolicyOutcome::PolicyError, "input_invalid")
                .expect("bounded reason")
        })?;
        if binding.tool_name != tool_name
            || binding.normalized_scope != scope
            || binding.input_hash != actual
            || binding.snapshot_hash != self.snapshot.snapshot_hash
            || binding.policy_version != self.snapshot.policy_version
        {
            return Err(
                PolicyDecision::new(PolicyOutcome::PolicyError, "binding_changed")
                    .expect("bounded reason"),
            );
        }
        if !matches!(
            current,
            PolicyOutcome::Allowed | PolicyOutcome::ApprovalRequired
        ) {
            return Err(PolicyDecision::new(current, "current_policy").expect("bounded reason"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_receipts::capability::{CapabilityLimits, SecretRefPurpose};

    fn snapshot() -> CapabilitySnapshotV1 {
        CapabilitySnapshotV1 {
            snapshot_id: "snapshot".into(),
            run_id: "run".into(),
            session_id: "session".into(),
            task_id: "task".into(),
            parent_snapshot_hash: None,
            policy_id: "policy".into(),
            policy_version: 1,
            policy_hash: "a".repeat(64),
            manifest_hash: "b".repeat(64),
            workspace_anchors: vec!["workspace".into()],
            operation_scopes: vec!["workspace".into()],
            permissions: vec!["shell_execute".into()],
            tool_identities: vec!["shell.execute".into()],
            network_routes: vec![],
            adapter_scopes: vec![],
            secret_refs: Vec::<SecretRefPurpose>::new(),
            limits: CapabilityLimits {
                timeout_ms: 1000,
                input_bytes: 1000,
                output_bytes: 1000,
                concurrency: 1,
                tool_calls: 1,
                token_budget: 1,
                cost_micros: 1,
            },
            snapshot_hash: String::new(),
        }
        .finalize()
        .unwrap()
    }

    #[test]
    fn drift_is_rejected_at_effect_boundary() {
        let gate = PolicyGate::new(snapshot()).unwrap();
        let input = serde_json::json!({"program":"echo","args":[]});
        let binding = gate
            .preflight(
                "action",
                "shell.execute",
                "workspace",
                &input,
                PolicyOutcome::Allowed,
            )
            .unwrap();
        assert!(gate
            .recheck_before_effect(
                &binding,
                "shell.execute",
                "workspace",
                &serde_json::json!({"program":"del","args":[]}),
                PolicyOutcome::Allowed
            )
            .is_err());
    }

    #[test]
    fn deny_is_not_retryable() {
        let gate = PolicyGate::new(snapshot()).unwrap();
        let decision = gate
            .preflight(
                "action",
                "shell.execute",
                "workspace",
                &serde_json::json!({}),
                PolicyOutcome::Denied,
            )
            .unwrap_err();
        assert_eq!(decision.outcome, PolicyOutcome::Denied);
        assert!(!decision.retryable);
    }

    #[test]
    fn hooks_expose_hashes_but_not_raw_input() {
        let gate = PolicyGate::new(snapshot()).unwrap();
        let input = serde_json::json!({"token":"must-not-leak"});
        let binding = gate
            .preflight(
                "action",
                "shell.execute",
                "workspace",
                &input,
                PolicyOutcome::Allowed,
            )
            .unwrap();
        let metadata = PolicyHooks.postflight(&binding, "succeeded");
        let serialized = serde_json::to_string(&metadata.input_hash).unwrap();
        assert!(!serialized.contains("must-not-leak"));
        assert_eq!(metadata.hook_chain_version, HOOK_CHAIN_VERSION);
    }
}
