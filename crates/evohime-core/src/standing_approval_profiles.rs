//! Narrow, user-authored standing approvals. This module never grants capabilities.
use serde::{Deserialize, Serialize};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_RULES: usize = 32;
pub const MAX_PROFILES: usize = 128;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectScope {
    UserGlobal,
    Workspace,
    WorkspaceSet,
    Conversation,
    Goal,
    WorkflowDefinition,
    WorkflowRun,
    AgentRole,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRule {
    pub action_class: String,
    pub resource: String,
    pub max_risk: u8,
    pub foreground_only: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingApprovalProfile {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub name: String,
    pub enabled: bool,
    pub subject_scope: SubjectScope,
    pub subject_id: String,
    pub rules: Vec<ApprovalRule>,
    pub created_by: String,
    pub expires_at_ms: Option<i64>,
    pub max_uses: Option<u32>,
    pub uses: u32,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest<'a> {
    pub subject_id: &'a str,
    pub action_class: &'a str,
    pub resource: &'a str,
    pub risk: u8,
    pub foreground: bool,
    pub now_ms: i64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDecision {
    pub approved: bool,
    pub profile_id: Option<String>,
    pub reason: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("unsupported standing approval schema")]
    UnsupportedVersion,
    #[error("invalid standing approval profile")]
    Invalid,
    #[error("standing approval limits exceeded")]
    Bounds,
    #[error("critical risk cannot be auto-approved")]
    HardDeny,
}
fn valid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && !s.bytes().any(|b| b.is_ascii_control())
}
pub fn validate(p: &StandingApprovalProfile) -> Result<(), ProfileError> {
    if p.schema_version != SCHEMA_VERSION {
        return Err(ProfileError::UnsupportedVersion);
    }
    if !valid(&p.id)
        || !valid(&p.name)
        || !valid(&p.subject_id)
        || !valid(&p.created_by)
        || p.version == 0
        || p.rules.is_empty()
        || p.rules.len() > MAX_RULES
        || p.uses > p.max_uses.unwrap_or(u32::MAX)
    {
        return Err(ProfileError::Invalid);
    }
    for r in &p.rules {
        if !valid(&r.action_class) || !valid(&r.resource) || r.max_risk > 3 {
            return Err(ProfileError::Invalid);
        }
        if r.max_risk >= 3 {
            return Err(ProfileError::HardDeny);
        }
    }
    if p.expires_at_ms.is_some_and(|x| x <= 0) {
        return Err(ProfileError::Invalid);
    }
    Ok(())
}
pub fn match_request(
    p: &StandingApprovalProfile,
    r: &ApprovalRequest<'_>,
) -> Result<ApprovalDecision, ProfileError> {
    validate(p)?;
    if !p.enabled {
        return Ok(ApprovalDecision {
            approved: false,
            profile_id: None,
            reason: "disabled".into(),
        });
    }
    if p.expires_at_ms.is_some_and(|x| x <= r.now_ms) {
        return Ok(ApprovalDecision {
            approved: false,
            profile_id: None,
            reason: "expired".into(),
        });
    }
    if p.max_uses.is_some_and(|x| p.uses >= x) {
        return Ok(ApprovalDecision {
            approved: false,
            profile_id: None,
            reason: "uses_exhausted".into(),
        });
    }
    if r.risk >= 3 {
        return Err(ProfileError::HardDeny);
    }
    if p.subject_id != r.subject_id {
        return Ok(ApprovalDecision {
            approved: false,
            profile_id: None,
            reason: "scope_mismatch".into(),
        });
    }
    if p.rules.iter().any(|x| {
        x.action_class == r.action_class
            && x.resource == r.resource
            && r.risk <= x.max_risk
            && (!x.foreground_only || r.foreground)
    }) {
        return Ok(ApprovalDecision {
            approved: true,
            profile_id: Some(p.id.clone()),
            reason: "standing_profile_match_policy_still_required".into(),
        });
    }
    Ok(ApprovalDecision {
        approved: false,
        profile_id: None,
        reason: "rule_mismatch".into(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p() -> StandingApprovalProfile {
        StandingApprovalProfile {
            schema_version: 1,
            id: "p".into(),
            version: 1,
            name: "safe".into(),
            enabled: true,
            subject_scope: SubjectScope::Workspace,
            subject_id: "w".into(),
            rules: vec![ApprovalRule {
                action_class: "read".into(),
                resource: "src".into(),
                max_risk: 1,
                foreground_only: true,
            }],
            created_by: "user".into(),
            expires_at_ms: Some(100),
            max_uses: Some(2),
            uses: 0,
            content_hash: "h".into(),
        }
    }
    #[test]
    fn matches_bounded_rule() {
        let x = match_request(
            &p(),
            &ApprovalRequest {
                subject_id: "w",
                action_class: "read",
                resource: "src",
                risk: 1,
                foreground: true,
                now_ms: 1,
            },
        )
        .unwrap();
        assert!(x.approved)
    }
    #[test]
    fn critical_is_fail_closed() {
        let mut x = p();
        x.rules[0].max_risk = 3;
        assert_eq!(validate(&x), Err(ProfileError::HardDeny))
    }
}
