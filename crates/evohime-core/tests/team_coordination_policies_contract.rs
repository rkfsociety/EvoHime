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
