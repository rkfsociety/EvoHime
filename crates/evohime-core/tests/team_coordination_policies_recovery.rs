use evohime_core::team_coordination_policies::*;

#[test]
fn repeated_selection_guard_stops_without_blind_retry() {
    let spec = TeamSpec {
        schema_version: 1,
        id: "team".into(),
        revision: 1,
        members: vec![TeamMemberSpec {
            role: "coder".into(),
            agent_profile: "coder".into(),
            allowed_capabilities: vec![],
        }],
        coordination: CoordinationPolicy::Selector,
        max_consecutive_turns_per_member: 1,
        max_team_turns: 4,
        repeated_selection_limit: 1,
    };
    let state = initial_state(&spec).unwrap();
    let (state, _) = select_next(&spec, &state, None, Some("coder"), None, &[]).unwrap();
    assert_eq!(
        select_next(&spec, &state, None, Some("coder"), None, &[]),
        Err(CoordinationError::LimitReached)
    );
}
