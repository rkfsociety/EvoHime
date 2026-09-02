//! Core-owned granular approval policy; it is evaluated before, never instead of, execution policy.
use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RULES: usize = 32;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyScope {
    Conversation,
    Workspace,
    WorkflowRun,
    User,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyRule {
    pub action_class: String,
    pub risk: u8,
    pub resource_prefix: String,
    pub require_prompt: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPolicyProfile {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub scope: PolicyScope,
    pub scope_id: String,
    pub enabled: bool,
    pub rules: Vec<PolicyRule>,
    pub expires_at_ms: Option<i64>,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyDecision {
    pub require_prompt: bool,
    pub profile_id: Option<String>,
    pub reason: String,
    pub hard_requirement: bool,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PolicyError {
    #[error("unsupported approval policy schema")]
    UnsupportedVersion,
    #[error("invalid approval policy profile")]
    Invalid,
    #[error("approval policy bounds exceeded")]
    Bounds,
    #[error("hard approval requirement cannot be removed")]
    HardRequirement,
}
fn valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.bytes().any(|b| b.is_ascii_control())
}
pub fn validate(p: &ApprovalPolicyProfile) -> Result<(), PolicyError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(PolicyError::UnsupportedVersion);
    }
    if !valid(&p.id)
        || !valid(&p.scope_id)
        || p.version == 0
        || p.rules.is_empty()
        || p.rules.len() > MAX_RULES
    {
        return Err(PolicyError::Invalid);
    }
    for r in &p.rules {
        if !valid(&r.action_class) || !valid(&r.resource_prefix) || r.risk > 3 {
            return Err(PolicyError::Invalid);
        }
        if r.risk >= 3 && !r.require_prompt {
            return Err(PolicyError::HardRequirement);
        }
    }
    if p.expires_at_ms.is_some_and(|x| x <= 0) {
        return Err(PolicyError::Invalid);
    }
    Ok(())
}
pub fn decide(
    p: &ApprovalPolicyProfile,
    scope_id: &str,
    action: &str,
    resource: &str,
    risk: u8,
    now: i64,
) -> Result<PolicyDecision, PolicyError> {
    validate(p)?;
    if risk >= 3 {
        return Ok(PolicyDecision {
            require_prompt: true,
            profile_id: None,
            reason: "hard_requirement".into(),
            hard_requirement: true,
        });
    }
    if !p.enabled || p.scope_id != scope_id || p.expires_at_ms.is_some_and(|x| x <= now) {
        return Ok(PolicyDecision {
            require_prompt: true,
            profile_id: None,
            reason: "default_prompt".into(),
            hard_requirement: false,
        });
    }
    if let Some(rule) = p.rules.iter().find(|r| {
        r.action_class == action && resource.starts_with(&r.resource_prefix) && risk <= r.risk
    }) {
        return Ok(PolicyDecision {
            require_prompt: rule.require_prompt,
            profile_id: Some(p.id.clone()),
            reason: if rule.require_prompt {
                "profile_requires_prompt".into()
            } else {
                "profile_match".into()
            },
            hard_requirement: false,
        });
    }
    Ok(PolicyDecision {
        require_prompt: true,
        profile_id: None,
        reason: "no_rule_match".into(),
        hard_requirement: false,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> ApprovalPolicyProfile {
        ApprovalPolicyProfile {
            schema_version: 1,
            id: "p".into(),
            version: 1,
            scope: PolicyScope::Workspace,
            scope_id: "w".into(),
            enabled: true,
            rules: vec![PolicyRule {
                action_class: "read".into(),
                risk: 1,
                resource_prefix: "src/".into(),
                require_prompt: false,
            }],
            expires_at_ms: Some(100),
            content_hash: "h".into(),
        }
    }
    #[test]
    fn bounded_match() {
        assert!(
            !decide(&p(), "w", "read", "src/lib.rs", 1, 1)
                .unwrap()
                .require_prompt
        )
    }
    #[test]
    fn hard_requirement_wins() {
        assert!(
            decide(&p(), "w", "read", "src/lib.rs", 3, 1)
                .unwrap()
                .hard_requirement
        )
    }
}
