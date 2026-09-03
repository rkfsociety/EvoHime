//! Core-owned, metadata-only interceptors for collaboration delivery.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_HOOKS: usize = 32;
pub const MAX_PATCHES: usize = 8;
pub const MAX_TEXT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    BeforeDelivery,
    BeforeRecipientContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    FailClosed,
    FailOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityClass {
    Public,
    Internal,
    Sensitive,
    Secret,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterventionAction {
    Allow,
    Block,
    Redact,
    Redirect,
    Escalate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInterventionHook {
    pub id: String,
    pub version: u64,
    pub priority: u16,
    pub phases: Vec<HookPhase>,
    pub action: InterventionAction,
    pub failure_mode: FailureMode,
    pub allowed_routes: Vec<String>,
    pub allowed_sensitivity: Vec<SensitivityClass>,
    pub message_kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInterventionPolicy {
    pub schema_version: u32,
    pub id: String,
    pub version: u64,
    pub hooks: Vec<MessageInterventionHook>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInterventionContext {
    pub team_session_id: String,
    pub sender: String,
    pub recipients: Vec<String>,
    pub message_kind: String,
    pub contract_ref: Option<String>,
    pub payload_metadata: String,
    pub sensitivity: SensitivityClass,
    pub phase: HookPhase,
    pub causation_id: Option<String>,
    pub routing_snapshot_hash: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionVerdict {
    pub action: InterventionAction,
    pub reason_code: String,
    pub hook_id: Option<String>,
    pub projection_patches: Vec<String>,
    pub escalation_ref: Option<String>,
    pub redaction_status: String,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum InterventionError {
    #[error("unsupported intervention schema {0}")]
    UnsupportedVersion(u32),
    #[error("invalid intervention policy or context")]
    Invalid,
    #[error("intervention policy is too large")]
    TooLarge,
    #[error("intervention policy hash is invalid")]
    InvalidHash,
    #[error("duplicate intervention delivery")]
    Duplicate,
    #[error("intervention failed closed")]
    FailedClosed,
}

fn valid(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= MAX_TEXT
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:/".contains(&b))
}

pub fn canonical_hash(policy: &MessageInterventionPolicy) -> Result<String, InterventionError> {
    let mut copy = policy.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| InterventionError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn validate_policy(policy: &MessageInterventionPolicy) -> Result<(), InterventionError> {
    if policy.schema_version != SCHEMA_VERSION {
        return Err(InterventionError::UnsupportedVersion(policy.schema_version));
    }
    if !valid(&policy.id)
        || policy.version == 0
        || policy.hooks.is_empty()
        || policy.hooks.len() > MAX_HOOKS
        || policy.content_hash.len() != 64
    {
        return Err(InterventionError::Invalid);
    }
    if canonical_hash(policy)? != policy.content_hash {
        return Err(InterventionError::InvalidHash);
    }
    for hook in &policy.hooks {
        if !valid(&hook.id)
            || hook.version == 0
            || hook.phases.is_empty()
            || hook.allowed_routes.iter().any(|v| !valid(v))
            || hook.message_kinds.iter().any(|v| !valid(v))
            || hook.allowed_sensitivity.is_empty()
        {
            return Err(InterventionError::Invalid);
        }
    }
    Ok(())
}

pub fn validate_context(context: &MessageInterventionContext) -> Result<(), InterventionError> {
    if !valid(&context.team_session_id)
        || !valid(&context.sender)
        || context.recipients.is_empty()
        || context.recipients.len() > MAX_HOOKS
        || context.recipients.iter().any(|v| !valid(v))
        || !valid(&context.message_kind)
        || context.payload_metadata.len() > MAX_TEXT
        || !valid(&context.routing_snapshot_hash)
        || !valid(&context.idempotency_key)
    {
        return Err(InterventionError::Invalid);
    }
    if context.sensitivity == SensitivityClass::Secret {
        return Err(InterventionError::Invalid);
    }
    Ok(())
}

pub fn evaluate(
    policy: &MessageInterventionPolicy,
    context: &MessageInterventionContext,
    seen: bool,
) -> Result<InterventionVerdict, InterventionError> {
    validate_policy(policy)?;
    validate_context(context)?;
    if seen {
        return Err(InterventionError::Duplicate);
    }
    let mut hooks = policy
        .hooks
        .iter()
        .filter(|h| h.phases.contains(&context.phase))
        .collect::<Vec<_>>();
    hooks.sort_by_key(|h| (h.priority, h.id.as_str()));
    for hook in hooks {
        if !hook.allowed_routes.is_empty()
            && context
                .recipients
                .iter()
                .any(|r| !hook.allowed_routes.contains(r))
        {
            return Ok(InterventionVerdict {
                action: InterventionAction::Block,
                reason_code: "route_denied".into(),
                hook_id: Some(hook.id.clone()),
                projection_patches: vec![],
                escalation_ref: None,
                redaction_status: "metadata_only".into(),
            });
        }
        if !hook.allowed_sensitivity.contains(&context.sensitivity) {
            return Ok(InterventionVerdict {
                action: InterventionAction::Block,
                reason_code: "sensitivity_denied".into(),
                hook_id: Some(hook.id.clone()),
                projection_patches: vec![],
                escalation_ref: None,
                redaction_status: "metadata_only".into(),
            });
        }
        if !hook.message_kinds.is_empty() && !hook.message_kinds.contains(&context.message_kind) {
            continue;
        }
        let (action, reason) = match hook.action {
            InterventionAction::Allow => (InterventionAction::Allow, "allowed"),
            InterventionAction::Redact => (InterventionAction::Redact, "redaction_required"),
            InterventionAction::Redirect => (InterventionAction::Redirect, "redirect_required"),
            InterventionAction::Escalate => {
                (InterventionAction::Escalate, "human_escalation_required")
            }
            InterventionAction::Block => (InterventionAction::Block, "policy_blocked"),
        };
        return Ok(InterventionVerdict {
            action,
            reason_code: reason.into(),
            hook_id: Some(hook.id.clone()),
            projection_patches: if action == InterventionAction::Redact {
                vec!["payload_metadata=redacted".into()]
            } else {
                vec![]
            },
            escalation_ref: (action == InterventionAction::Escalate)
                .then(|| format!("escalation:{}", context.idempotency_key)),
            redaction_status: "metadata_only".into(),
        });
    }
    Ok(InterventionVerdict {
        action: InterventionAction::Allow,
        reason_code: "no_matching_hook".into(),
        hook_id: None,
        projection_patches: vec![],
        escalation_ref: None,
        redaction_status: "metadata_only".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy(action: InterventionAction) -> MessageInterventionPolicy {
        let mut p = MessageInterventionPolicy {
            schema_version: 1,
            id: "p".into(),
            version: 1,
            hooks: vec![MessageInterventionHook {
                id: "h".into(),
                version: 1,
                priority: 1,
                phases: vec![HookPhase::BeforeDelivery],
                action,
                failure_mode: FailureMode::FailClosed,
                allowed_routes: vec!["recipient".into()],
                allowed_sensitivity: vec![SensitivityClass::Internal],
                message_kinds: vec!["notice".into()],
            }],
            content_hash: String::new(),
        };
        p.content_hash = canonical_hash(&p).unwrap();
        p
    }
    fn context() -> MessageInterventionContext {
        MessageInterventionContext {
            team_session_id: "s".into(),
            sender: "sender".into(),
            recipients: vec!["recipient".into()],
            message_kind: "notice".into(),
            contract_ref: None,
            payload_metadata: "size=2".into(),
            sensitivity: SensitivityClass::Internal,
            phase: HookPhase::BeforeDelivery,
            causation_id: None,
            routing_snapshot_hash: "snapshot".into(),
            idempotency_key: "key".into(),
        }
    }
    #[test]
    fn fixed_order_and_typed_patch() {
        let v = evaluate(&policy(InterventionAction::Redact), &context(), false).unwrap();
        assert_eq!(v.action, InterventionAction::Redact);
        assert_eq!(v.projection_patches, vec!["payload_metadata=redacted"]);
    }
    #[test]
    fn duplicate_and_route_fail_closed() {
        assert_eq!(
            evaluate(&policy(InterventionAction::Allow), &context(), true),
            Err(InterventionError::Duplicate)
        );
        let mut c = context();
        c.recipients = vec!["other".into()];
        assert_eq!(
            evaluate(&policy(InterventionAction::Allow), &c, false)
                .unwrap()
                .reason_code,
            "route_denied"
        );
    }
}
