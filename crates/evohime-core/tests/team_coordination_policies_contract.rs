use evohime_core::team_coordination_policies::*;
use std::collections::BTreeMap;

fn spec() -> TeamSpec {
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
        coordination: CoordinationPolicy::RoleRouter {
            rules: BTreeMap::from([(String::from("test_failure"), String::from("reviewer"))]),
        },
        max_consecutive_turns_per_member: 2,
        max_team_turns: 8,
        repeated_selection_limit: 3,
    }
}

#[test]
fn version_hash_and_routing_are_stable() {
    let value = spec();
    validate_team(&value).unwrap();
    assert_eq!(canonical_hash(&value).unwrap().len(), 64);
    let state = initial_state(&value).unwrap();
    let (_, decision) = select_next(
        &value,
        &state,
        None,
        None,
        Some("test_failure"),
        &["event-1".into()],
    )
    .unwrap();
    assert_eq!(decision.selected_role, "reviewer");
}

#[test]
fn duplicate_roles_and_unknown_versions_are_rejected() {
    let mut value = spec();
    value.schema_version = 2;
    assert!(matches!(
        validate_team(&value),
        Err(CoordinationError::UnsupportedVersion(2))
    ));
    let mut value = spec();
    value.members[1].role = "coder".into();
    assert!(matches!(
        validate_team(&value),
        Err(CoordinationError::Invalid("member"))
    ));
}

#[test]
fn versioned_strategies_keep_selection_core_owned_and_bounded() {
    let team = spec();
    let strategy = strategy_from_team(
        &team,
        "session-1",
        "protocol-1",
        &"a".repeat(64),
        TeamCoordinationStrategyKind::ModelSelector,
        Some("reviewer".into()),
    )
    .unwrap();
    assert_eq!(canonical_strategy_hash(&strategy).unwrap().len(), 64);
    let state = initial_strategy_state(&strategy).unwrap();
    let (next, decision) = select_strategy(
        &strategy,
        &state,
        Some(&ParticipantIdentity {
            role: "coder".into(),
        }),
        None,
        &[],
    )
    .unwrap();
    assert_eq!(decision.participant.role, "coder");
    assert!(!decision.fallback);
    assert_eq!(next.session_id, "session-1");
    assert!(select_strategy(
        &strategy,
        &next,
        Some(&ParticipantIdentity {
            role: "not-eligible".into()
        }),
        None,
        &[],
    )
    .is_ok());
}

#[test]
fn selector_failure_uses_only_explicit_fallback() {
    let team = spec();
    let strategy = strategy_from_team(
        &team,
        "session-1",
        "protocol-1",
        &"b".repeat(64),
        TeamCoordinationStrategyKind::RuleSelector,
        Some("reviewer".into()),
    )
    .unwrap();
    let state = initial_strategy_state(&strategy).unwrap();
    let (_, decision) = select_strategy(&strategy, &state, None, None, &[]).unwrap();
    assert_eq!(decision.participant.role, "reviewer");
    assert!(decision.fallback);
}

#[test]
fn handoff_swarm_and_graph_require_declared_edges() {
    let team = spec();
    let routes = BTreeMap::from([(String::from("coder"), vec![String::from("reviewer")])]);
    for kind in [
        TeamCoordinationStrategyKind::HandoffSwarm {
            routes: routes.clone(),
        },
        TeamCoordinationStrategyKind::GraphDirected {
            edges: routes.clone(),
        },
    ] {
        let strategy = strategy_from_team(
            &team,
            "session-1",
            "protocol-1",
            &"c".repeat(64),
            kind,
            None,
        )
        .unwrap();
        let mut state = initial_strategy_state(&strategy).unwrap();
        state.coordination.current_owner = Some("coder".into());
        assert!(select_strategy(
            &strategy,
            &state,
            Some(&ParticipantIdentity {
                role: "reviewer".into(),
            }),
            Some("coder"),
            &[],
        )
        .is_ok());
        assert!(matches!(
            select_strategy(
                &strategy,
                &state,
                Some(&ParticipantIdentity {
                    role: "coder".into(),
                }),
                Some("coder"),
                &[],
            ),
            Err(CoordinationError::ProtocolRouteDenied)
        ));
    }
}
