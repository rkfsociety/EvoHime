//! Core-owned, metadata-only interceptors for collaboration delivery.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current schema version for collaboration intervention policies.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of hooks in one policy or recipients in one context.
pub const MAX_HOOKS: usize = 32;
/// Maximum number of metadata projection patches in one verdict.
pub const MAX_PATCHES: usize = 8;
/// Maximum length of bounded policy identifiers and metadata text.
pub const MAX_TEXT: usize = 128;

/// Delivery phase at which an intervention hook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPhase {
    /// Evaluate before a message is delivered to recipients.
    BeforeDelivery,
    /// Evaluate before recipient-specific context is constructed.
    BeforeRecipientContext,
}

/// Behavior used when an intervention hook cannot produce a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    /// Deny delivery when the hook fails.
    FailClosed,
    /// Allow delivery when the hook fails.
    FailOpen,
}

/// Classification of message data permitted by a hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityClass {
    /// Data intended for unrestricted disclosure.
    Public,
    /// Data internal to the workspace or team.
    Internal,
    /// Data requiring restricted recipients.
    Sensitive,
    /// Secret data that this metadata-only contract does not accept.
    Secret,
}

/// Decision a matching intervention hook may return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterventionAction {
    /// Permit delivery under the supplied metadata.
    Allow,
    /// Deny delivery.
    Block,
    /// Require a redacted recipient projection.
    Redact,
    /// Route through an alternate allowed destination.
    Redirect,
    /// Require human review before delivery.
    Escalate,
}

/// Ordered rule applied at selected collaboration delivery phases.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInterventionHook {
    /// Stable hook identifier.
    pub id: String,
    /// Monotonic hook revision.
    pub version: u64,
    /// Lower values run earlier; identifier breaks ties deterministically.
    pub priority: u16,
    /// Delivery phases where this hook is active.
    pub phases: Vec<HookPhase>,
    /// Decision returned when the hook matches.
    pub action: InterventionAction,
    /// Fail-open or fail-closed behavior for evaluator errors.
    pub failure_mode: FailureMode,
    /// Recipient routes allowed by this hook; empty means unrestricted by route.
    pub allowed_routes: Vec<String>,
    /// Sensitivity classes permitted by this hook.
    pub allowed_sensitivity: Vec<SensitivityClass>,
    /// Message kinds matched by this hook; empty means all kinds.
    pub message_kinds: Vec<String>,
}

/// Integrity-bound collection of collaboration message hooks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInterventionPolicy {
    /// Policy schema version.
    pub schema_version: u32,
    /// Stable policy identifier.
    pub id: String,
    /// Monotonic policy revision.
    pub version: u64,
    /// Hooks evaluated in deterministic priority order.
    pub hooks: Vec<MessageInterventionHook>,
    /// Digest of the policy with this field cleared.
    pub content_hash: String,
}

/// Metadata available to hooks without exposing raw message content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageInterventionContext {
    /// Team session containing the message.
    pub team_session_id: String,
    /// Stable sender identity.
    pub sender: String,
    /// Intended recipients or routes.
    pub recipients: Vec<String>,
    /// Message category used for hook matching.
    pub message_kind: String,
    /// Optional message contract reference.
    pub contract_ref: Option<String>,
    /// Bounded metadata projection of the message payload.
    pub payload_metadata: String,
    /// Sensitivity classification of the message.
    pub sensitivity: SensitivityClass,
    /// Current delivery hook phase.
    pub phase: HookPhase,
    /// Optional identifier of the causing message.
    pub causation_id: Option<String>,
    /// Digest of the routing state used for this delivery.
    pub routing_snapshot_hash: String,
    /// Stable key used to detect duplicate intervention evaluation.
    pub idempotency_key: String,
}

/// Metadata-only intervention decision for a collaboration message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterventionVerdict {
    /// Action the delivery pipeline must enforce.
    pub action: InterventionAction,
    /// Stable machine-readable reason code.
    pub reason_code: String,
    /// Hook responsible for the decision, if one matched.
    pub hook_id: Option<String>,
    /// Bounded changes applied to the recipient metadata projection.
    pub projection_patches: Vec<String>,
    /// Optional reference to a human escalation request.
    pub escalation_ref: Option<String>,
    /// Redaction handling summary without message content.
    pub redaction_status: String,
}

/// Invalid policy/context, duplicate delivery, or fail-closed evaluation error.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum InterventionError {
    /// Policy uses an unsupported schema version.
    #[error("unsupported intervention schema {0}")]
    UnsupportedVersion(u32),
    /// A policy or context field is malformed or unsupported.
    #[error("invalid intervention policy or context")]
    Invalid,
    /// Policy or context exceeds its documented bounds.
    #[error("intervention policy is too large")]
    TooLarge,
    /// Policy hash does not match its serialized content.
    #[error("intervention policy hash is invalid")]
    InvalidHash,
    /// The same message delivery was already evaluated.
    #[error("duplicate intervention delivery")]
    Duplicate,
    /// Hook evaluation failed under fail-closed behavior.
    #[error("intervention failed closed")]
    FailedClosed,
}

fn valid(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= MAX_TEXT
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-:/".contains(&b))
}

/// Computes the policy digest after clearing its stored hash field.
pub fn canonical_hash(policy: &MessageInterventionPolicy) -> Result<String, InterventionError> {
    let mut copy = policy.clone();
    copy.content_hash.clear();
    let bytes = serde_json::to_vec(&copy).map_err(|_| InterventionError::Invalid)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Checks policy identity, hook bounds, and its canonical digest.
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

/// Validates bounded routing metadata and rejects secret-class message context.
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

/// Selects the first matching hook and returns its metadata-only verdict.
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
