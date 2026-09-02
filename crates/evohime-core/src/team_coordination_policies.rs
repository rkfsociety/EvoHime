//! Core-owned, bounded routing policies for an existing child/workflow run.
//! This module selects ownership only; it never grants capabilities or starts effects.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_MEMBERS: usize = 32;
pub const MAX_RULES: usize = 64;
pub const MAX_TEXT: usize = 256;
pub const MAX_TURNS: u64 = 100_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamMemberSpec {
    pub role: String,
    pub agent_profile: String,
    pub allowed_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoordinationPolicy {
    RoundRobin,
    Selector,
    DirectedHandoff,
    RoleRouter { rules: BTreeMap<String, String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSpec {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub members: Vec<TeamMemberSpec>,
    pub coordination: CoordinationPolicy,
    pub max_consecutive_turns_per_member: u32,
    pub max_team_turns: u64,
    pub repeated_selection_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamCoordinationState {
    pub team_id: String,
    pub policy_revision: u64,
    pub current_owner: Option<String>,
    pub previous_owner: Option<String>,
    pub turn_index: u64,
    pub per_member_turns: BTreeMap<String, u64>,
    pub recent_selection_history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionDecision {
    pub selected_role: String,
    pub reason_code: String,
    pub input_event_ids: Vec<String>,
    pub fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinationError {
    Invalid(&'static str),
    UnsupportedVersion(u32),
    UnknownRole,
    LimitReached,
    InvalidSelector,
    NoAvailableMember,
}

pub fn canonical_hash(spec: &TeamSpec) -> Result<String, CoordinationError> {
    validate_team(spec)?;
    let bytes =
        serde_json::to_vec(spec).map_err(|_| CoordinationError::Invalid("serialization"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

impl std::fmt::Display for CoordinationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(value) => f.write_str(value),
            Self::UnsupportedVersion(_) => f.write_str("unsupported_version"),
            Self::UnknownRole => f.write_str("unknown_role"),
            Self::LimitReached => f.write_str("limit_reached"),
            Self::InvalidSelector => f.write_str("invalid_selector"),
            Self::NoAvailableMember => f.write_str("no_available_member"),
        }
    }
}
impl std::error::Error for CoordinationError {}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT && !value.chars().any(char::is_control)
}

pub fn validate_team(spec: &TeamSpec) -> Result<(), CoordinationError> {
    if spec.schema_version != CONTRACT_VERSION {
        return Err(CoordinationError::UnsupportedVersion(spec.schema_version));
    }
    if !valid_text(&spec.id)
        || spec.revision == 0
        || spec.members.is_empty()
        || spec.members.len() > MAX_MEMBERS
        || spec.max_consecutive_turns_per_member == 0
        || u64::from(spec.max_consecutive_turns_per_member) > spec.max_team_turns
        || spec.max_team_turns == 0
        || spec.max_team_turns > MAX_TURNS
        || spec.repeated_selection_limit == 0
    {
        return Err(CoordinationError::Invalid("team"));
    }
    let mut roles = std::collections::BTreeSet::new();
    for member in &spec.members {
        if !valid_text(&member.role)
            || !valid_text(&member.agent_profile)
            || member.allowed_capabilities.len() > MAX_MEMBERS
            || member
                .allowed_capabilities
                .iter()
                .any(|value| !valid_text(value))
            || !roles.insert(member.role.as_str())
        {
            return Err(CoordinationError::Invalid("member"));
        }
    }
    if let CoordinationPolicy::RoleRouter { rules } = &spec.coordination {
        if rules.len() > MAX_RULES
            || rules
                .iter()
                .any(|(event, role)| !valid_text(event) || !roles.contains(role.as_str()))
        {
            return Err(CoordinationError::Invalid("router"));
        }
    }
    Ok(())
}

pub fn initial_state(spec: &TeamSpec) -> Result<TeamCoordinationState, CoordinationError> {
    validate_team(spec)?;
    Ok(TeamCoordinationState {
        team_id: spec.id.clone(),
        policy_revision: spec.revision,
        current_owner: None,
        previous_owner: None,
        turn_index: 0,
        per_member_turns: BTreeMap::new(),
        recent_selection_history: Vec::new(),
    })
}

pub fn select_next(
    spec: &TeamSpec,
    state: &TeamCoordinationState,
    handoff_from: Option<&str>,
    selector_role: Option<&str>,
    event_type: Option<&str>,
    event_ids: &[String],
) -> Result<(TeamCoordinationState, SelectionDecision), CoordinationError> {
    validate_team(spec)?;
    if state.team_id != spec.id || state.policy_revision != spec.revision {
        return Err(CoordinationError::Invalid("state_snapshot"));
    }
    if state.turn_index >= spec.max_team_turns {
        return Err(CoordinationError::LimitReached);
    }
    let roles: Vec<&str> = spec
        .members
        .iter()
        .map(|member| member.role.as_str())
        .collect();
    let selected = match &spec.coordination {
        CoordinationPolicy::RoundRobin => roles[state.turn_index as usize % roles.len()],
        CoordinationPolicy::Selector => {
            let role = selector_role.ok_or(CoordinationError::InvalidSelector)?;
            if !roles.contains(&role) {
                return Err(CoordinationError::InvalidSelector);
            }
            role
        }
        CoordinationPolicy::DirectedHandoff => {
            if state.current_owner.as_deref() != handoff_from {
                return Err(CoordinationError::InvalidSelector);
            }
            let role = selector_role.ok_or(CoordinationError::InvalidSelector)?;
            if !roles.contains(&role) {
                return Err(CoordinationError::InvalidSelector);
            }
            role
        }
        CoordinationPolicy::RoleRouter { rules } => {
            let event = event_type.ok_or(CoordinationError::NoAvailableMember)?;
            rules
                .get(event)
                .map(String::as_str)
                .ok_or(CoordinationError::NoAvailableMember)?
        }
    };
    let previous_count = state.per_member_turns.get(selected).copied().unwrap_or(0);
    if state.current_owner.as_deref() == Some(selected)
        && previous_count >= u64::from(spec.max_consecutive_turns_per_member)
    {
        return Err(CoordinationError::LimitReached);
    }
    let repeated = state
        .recent_selection_history
        .iter()
        .rev()
        .take(spec.repeated_selection_limit as usize)
        .all(|role| role == selected);
    if repeated && state.recent_selection_history.len() >= spec.repeated_selection_limit as usize {
        return Err(CoordinationError::LimitReached);
    }
    let mut next = state.clone();
    next.previous_owner = next.current_owner.take();
    next.current_owner = Some(selected.to_owned());
    next.turn_index += 1;
    *next
        .per_member_turns
        .entry(selected.to_owned())
        .or_default() += 1;
    next.recent_selection_history.push(selected.to_owned());
    next.recent_selection_history
        .truncate(spec.repeated_selection_limit as usize);
    Ok((
        next,
        SelectionDecision {
            selected_role: selected.to_owned(),
            reason_code: match &spec.coordination {
                CoordinationPolicy::RoundRobin => "round_robin",
                CoordinationPolicy::Selector => "selector",
                CoordinationPolicy::DirectedHandoff => "directed_handoff",
                CoordinationPolicy::RoleRouter { .. } => "role_router",
            }
            .into(),
            input_event_ids: event_ids.iter().take(MAX_MEMBERS).cloned().collect(),
            fallback: false,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn team(policy: CoordinationPolicy) -> TeamSpec {
        TeamSpec {
            schema_version: 1,
            id: "team".into(),
            revision: 1,
            members: vec![
                TeamMemberSpec {
                    role: "coder".into(),
                    agent_profile: "coder".into(),
                    allowed_capabilities: vec!["read".into()],
                },
                TeamMemberSpec {
                    role: "reviewer".into(),
                    agent_profile: "reviewer".into(),
                    allowed_capabilities: vec!["read".into()],
                },
            ],
            coordination: policy,
            max_consecutive_turns_per_member: 2,
            max_team_turns: 8,
            repeated_selection_limit: 3,
        }
    }
    #[test]
    fn round_robin_is_deterministic_and_bounded() {
        let spec = team(CoordinationPolicy::RoundRobin);
        let state = initial_state(&spec).unwrap();
        let (state, first) = select_next(&spec, &state, None, None, None, &[]).unwrap();
        let (_, second) = select_next(&spec, &state, None, None, None, &[]).unwrap();
        assert_eq!(first.selected_role, "coder");
        assert_eq!(second.selected_role, "reviewer");
    }
    #[test]
    fn selector_cannot_invent_role() {
        let spec = team(CoordinationPolicy::Selector);
        let state = initial_state(&spec).unwrap();
        assert_eq!(
            select_next(&spec, &state, None, Some("root"), None, &[]),
            Err(CoordinationError::InvalidSelector)
        );
    }
    #[test]
    fn router_validates_target() {
        let spec = team(CoordinationPolicy::RoleRouter {
            rules: [("test_failure".into(), "reviewer".into())].into(),
        });
        let state = initial_state(&spec).unwrap();
        assert_eq!(
            select_next(&spec, &state, None, None, Some("test_failure"), &[])
                .unwrap()
                .1
                .selected_role,
            "reviewer"
        );
    }
}
