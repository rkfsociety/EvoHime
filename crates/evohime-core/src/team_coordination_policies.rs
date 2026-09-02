//! Core-owned, bounded routing policies for an existing child/workflow run.
//! This module selects ownership only; it never grants capabilities or starts effects.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_MEMBERS: usize = 32;
pub const MAX_RULES: usize = 64;
pub const MAX_TEXT: usize = 256;
pub const MAX_TURNS: u64 = 100_000;
pub const STRATEGY_CONTRACT_VERSION: u32 = 1;

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

/// Versioned strategy contract.  The strategy is a selector only: it cannot
/// create participants, grant capabilities, or execute an effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TeamCoordinationStrategyKind {
    RoundRobin,
    RuleSelector,
    ModelSelector,
    HandoffSwarm {
        routes: BTreeMap<String, Vec<String>>,
    },
    GraphDirected {
        edges: BTreeMap<String, Vec<String>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamCoordinationStrategy {
    pub schema_version: u32,
    pub strategy_id: String,
    pub revision: u64,
    pub session_id: String,
    pub protocol_id: String,
    pub protocol_hash: String,
    pub eligible_roles: Vec<String>,
    pub kind: TeamCoordinationStrategyKind,
    pub fallback_role: Option<String>,
    pub max_consecutive_turns_per_member: u32,
    pub max_team_turns: u64,
    pub repeated_selection_limit: u32,
}

/// The only value a model selector may propose.  No rationale, prompt,
/// provider identity, tool name, or executable identity crosses this boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParticipantIdentity {
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategySessionState {
    pub session_id: String,
    pub strategy_id: String,
    pub strategy_revision: u64,
    pub protocol_hash: String,
    pub coordination: TeamCoordinationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrategySelectionDecision {
    pub participant: ParticipantIdentity,
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
    InvalidStrategy,
    ProtocolRouteDenied,
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
            Self::InvalidStrategy => f.write_str("invalid_strategy"),
            Self::ProtocolRouteDenied => f.write_str("protocol_route_denied"),
        }
    }
}
impl std::error::Error for CoordinationError {}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn validate_strategy(strategy: &TeamCoordinationStrategy) -> Result<(), CoordinationError> {
    if strategy.schema_version != STRATEGY_CONTRACT_VERSION {
        return Err(CoordinationError::UnsupportedVersion(
            strategy.schema_version,
        ));
    }
    if !valid_text(&strategy.strategy_id)
        || strategy.revision == 0
        || !valid_text(&strategy.session_id)
        || !valid_text(&strategy.protocol_id)
        || !valid_hash(&strategy.protocol_hash)
        || strategy.eligible_roles.is_empty()
        || strategy.eligible_roles.len() > MAX_MEMBERS
        || strategy.max_consecutive_turns_per_member == 0
        || strategy.max_team_turns == 0
        || strategy.max_team_turns > MAX_TURNS
        || u64::from(strategy.max_consecutive_turns_per_member) > strategy.max_team_turns
        || strategy.repeated_selection_limit == 0
    {
        return Err(CoordinationError::InvalidStrategy);
    }
    let roles = strategy
        .eligible_roles
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if roles.len() != strategy.eligible_roles.len()
        || strategy.eligible_roles.iter().any(|role| !valid_text(role))
        || strategy
            .fallback_role
            .as_deref()
            .is_some_and(|role| !roles.contains(role))
    {
        return Err(CoordinationError::InvalidStrategy);
    }
    let routes_valid = |routes: &BTreeMap<String, Vec<String>>| {
        routes.len() <= MAX_RULES
            && routes.iter().all(|(from, targets)| {
                roles.contains(from.as_str())
                    && !targets.is_empty()
                    && targets.len() <= MAX_MEMBERS
                    && targets.iter().all(|target| roles.contains(target.as_str()))
            })
    };
    match &strategy.kind {
        TeamCoordinationStrategyKind::HandoffSwarm { routes }
        | TeamCoordinationStrategyKind::GraphDirected { edges: routes } => {
            if !routes_valid(routes) {
                return Err(CoordinationError::InvalidStrategy);
            }
        }
        TeamCoordinationStrategyKind::RoundRobin
        | TeamCoordinationStrategyKind::RuleSelector
        | TeamCoordinationStrategyKind::ModelSelector => {}
    }
    Ok(())
}

pub fn canonical_strategy_hash(
    strategy: &TeamCoordinationStrategy,
) -> Result<String, CoordinationError> {
    validate_strategy(strategy)?;
    let bytes =
        serde_json::to_vec(strategy).map_err(|_| CoordinationError::Invalid("serialization"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub fn strategy_from_team(
    team: &TeamSpec,
    session_id: &str,
    protocol_id: &str,
    protocol_hash: &str,
    kind: TeamCoordinationStrategyKind,
    fallback_role: Option<String>,
) -> Result<TeamCoordinationStrategy, CoordinationError> {
    validate_team(team)?;
    let strategy = TeamCoordinationStrategy {
        schema_version: STRATEGY_CONTRACT_VERSION,
        strategy_id: format!("{}-{}", team.id, team.revision),
        revision: team.revision,
        session_id: session_id.to_owned(),
        protocol_id: protocol_id.to_owned(),
        protocol_hash: protocol_hash.to_owned(),
        eligible_roles: team
            .members
            .iter()
            .map(|member| member.role.clone())
            .collect(),
        kind,
        fallback_role,
        max_consecutive_turns_per_member: team.max_consecutive_turns_per_member,
        max_team_turns: team.max_team_turns,
        repeated_selection_limit: team.repeated_selection_limit,
    };
    validate_strategy(&strategy)?;
    Ok(strategy)
}

pub fn initial_strategy_state(
    strategy: &TeamCoordinationStrategy,
) -> Result<StrategySessionState, CoordinationError> {
    validate_strategy(strategy)?;
    Ok(StrategySessionState {
        session_id: strategy.session_id.clone(),
        strategy_id: strategy.strategy_id.clone(),
        strategy_revision: strategy.revision,
        protocol_hash: strategy.protocol_hash.clone(),
        coordination: TeamCoordinationState {
            team_id: strategy.strategy_id.clone(),
            policy_revision: strategy.revision,
            current_owner: None,
            previous_owner: None,
            turn_index: 0,
            per_member_turns: BTreeMap::new(),
            recent_selection_history: Vec::new(),
        },
    })
}

fn strategy_candidate<'a>(
    strategy: &'a TeamCoordinationStrategy,
    state: &StrategySessionState,
    proposed: Option<&'a ParticipantIdentity>,
    handoff_from: Option<&str>,
) -> Result<(&'a str, bool), CoordinationError> {
    let roles = strategy
        .eligible_roles
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let proposed_role = proposed.map(|value| value.role.as_str());
    let candidate = match &strategy.kind {
        TeamCoordinationStrategyKind::RoundRobin => {
            roles[state.coordination.turn_index as usize % roles.len()]
        }
        TeamCoordinationStrategyKind::RuleSelector
        | TeamCoordinationStrategyKind::ModelSelector => {
            proposed_role.ok_or(CoordinationError::InvalidSelector)?
        }
        TeamCoordinationStrategyKind::HandoffSwarm { routes } => {
            if state.coordination.current_owner.as_deref() != handoff_from {
                return Err(CoordinationError::ProtocolRouteDenied);
            }
            let from = handoff_from.ok_or(CoordinationError::ProtocolRouteDenied)?;
            let target = proposed_role.ok_or(CoordinationError::InvalidSelector)?;
            if !routes
                .get(from)
                .is_some_and(|targets| targets.iter().any(|v| v == target))
            {
                return Err(CoordinationError::ProtocolRouteDenied);
            }
            target
        }
        TeamCoordinationStrategyKind::GraphDirected { edges } => {
            let from = state
                .coordination
                .current_owner
                .as_deref()
                .ok_or(CoordinationError::NoAvailableMember)?;
            let target = proposed_role.ok_or(CoordinationError::InvalidSelector)?;
            if handoff_from != Some(from)
                || !edges
                    .get(from)
                    .is_some_and(|targets| targets.iter().any(|v| v == target))
            {
                return Err(CoordinationError::ProtocolRouteDenied);
            }
            target
        }
    };
    if !roles.contains(&candidate) {
        return Err(CoordinationError::InvalidSelector);
    }
    Ok((candidate, false))
}

pub fn select_strategy(
    strategy: &TeamCoordinationStrategy,
    state: &StrategySessionState,
    proposed: Option<&ParticipantIdentity>,
    handoff_from: Option<&str>,
    event_ids: &[String],
) -> Result<(StrategySessionState, StrategySelectionDecision), CoordinationError> {
    validate_strategy(strategy)?;
    if state.session_id != strategy.session_id
        || state.strategy_id != strategy.strategy_id
        || state.strategy_revision != strategy.revision
        || state.protocol_hash != strategy.protocol_hash
        || state.coordination.policy_revision != strategy.revision
    {
        return Err(CoordinationError::Invalid("strategy_snapshot"));
    }
    if state.coordination.turn_index >= strategy.max_team_turns {
        return Err(CoordinationError::LimitReached);
    }
    let selected = match strategy_candidate(strategy, state, proposed, handoff_from) {
        Ok(selected) => selected,
        Err(CoordinationError::InvalidSelector | CoordinationError::NoAvailableMember) => strategy
            .fallback_role
            .as_deref()
            .map(|role| (role, true))
            .ok_or(CoordinationError::NoAvailableMember)?,
        Err(error) => return Err(error),
    };
    let role = selected.0;
    let count = state
        .coordination
        .per_member_turns
        .get(role)
        .copied()
        .unwrap_or(0);
    let repeated = state
        .coordination
        .recent_selection_history
        .iter()
        .rev()
        .take(strategy.repeated_selection_limit as usize)
        .all(|value| value == role);
    if (state.coordination.current_owner.as_deref() == Some(role)
        && count >= u64::from(strategy.max_consecutive_turns_per_member))
        || (repeated
            && state.coordination.recent_selection_history.len()
                >= strategy.repeated_selection_limit as usize)
    {
        return Err(CoordinationError::LimitReached);
    }
    let mut next = state.clone();
    next.coordination.previous_owner = next.coordination.current_owner.take();
    next.coordination.current_owner = Some(role.to_owned());
    next.coordination.turn_index += 1;
    *next
        .coordination
        .per_member_turns
        .entry(role.to_owned())
        .or_default() += 1;
    next.coordination
        .recent_selection_history
        .push(role.to_owned());
    next.coordination
        .recent_selection_history
        .truncate(strategy.repeated_selection_limit as usize);
    Ok((
        next,
        StrategySelectionDecision {
            participant: ParticipantIdentity {
                role: role.to_owned(),
            },
            reason_code: match &strategy.kind {
                TeamCoordinationStrategyKind::RoundRobin => "round_robin",
                TeamCoordinationStrategyKind::RuleSelector => "rule_selector",
                TeamCoordinationStrategyKind::ModelSelector => "model_selector",
                TeamCoordinationStrategyKind::HandoffSwarm { .. } => "handoff_swarm",
                TeamCoordinationStrategyKind::GraphDirected { .. } => "graph_directed",
            }
            .into(),
            input_event_ids: event_ids.iter().take(MAX_MEMBERS).cloned().collect(),
            fallback: selected.1,
        },
    ))
}

/// Validate a route against the immutable TeamProtocol snapshot before a
/// handoff/graph decision is dispatched.  The strategy's hash is checked by
/// the caller when binding the snapshot; this function validates the actual
/// declared producer/consumer route and never grants a capability.
pub fn validate_protocol_route(
    snapshot: &crate::team_sop_protocols::ProtocolSnapshot,
    from: &str,
    to: &str,
) -> Result<(), CoordinationError> {
    if snapshot.protocol_id.is_empty() || !valid_hash(&snapshot.content_hash) {
        return Err(CoordinationError::ProtocolRouteDenied);
    }
    let protocol: crate::team_sop_protocols::TeamProtocol =
        serde_json::from_slice(&snapshot.protocol_json)
            .map_err(|_| CoordinationError::ProtocolRouteDenied)?;
    crate::team_sop_protocols::validate_protocol(&protocol)
        .map_err(|_| CoordinationError::ProtocolRouteDenied)?;
    if protocol.content_hash != snapshot.content_hash
        || protocol.id != snapshot.protocol_id
        || !protocol.handoffs.iter().any(|handoff| {
            handoff.producer_slot == from && handoff.consumers.iter().any(|consumer| consumer == to)
        })
    {
        return Err(CoordinationError::ProtocolRouteDenied);
    }
    Ok(())
}

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
