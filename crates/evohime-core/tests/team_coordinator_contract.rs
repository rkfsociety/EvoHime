use evohime_core::team_coordinator::*;

fn item() -> TeamWorkItem {
    TeamWorkItem {
        schema_version: SCHEMA_VERSION,
        id: "w".into(),
        objective: "objective".into(),
        required_output_contract: "report-v1".into(),
        required_capabilities: vec!["repo.read".into()],
        preferred_role_tags: vec![],
        dependencies: vec![],
        priority: 1,
        estimated_cost_class: Some("small".into()),
        status: WorkItemStatus::Unassigned,
        assigned_instance_id: None,
        attempt: 0,
        max_attempts: 4,
        created_by: "core".into(),
        evidence_refs: vec![],
        revision: 1,
    }
}

#[test]
fn transition_is_revision_fenced_and_managerial_accept_needs_both_gates() {
    let mut work = item();
    assert_eq!(
        transition(&mut work, WorkItemStatus::Assigned, 0),
        Err(CoordinatorError::StaleRevision)
    );
    transition(&mut work, WorkItemStatus::Assigned, 1).unwrap();
    assert_eq!(work.revision, 2);
    let review = CoordinationReview {
        schema_version: SCHEMA_VERSION,
        work_item_id: "w".into(),
        verdict: ManagerialVerdict::Accept,
        findings: vec![],
        evidence_refs: vec![],
        required_changes: vec![],
        security_gate_passed: true,
        acceptance_gate_passed: false,
    };
    assert_eq!(
        validate_review(&review),
        Err(CoordinatorError::GateRequired)
    );
}

#[test]
fn forbidden_authority_data_and_unknown_schema_are_rejected() {
    let mut work = item();
    work.schema_version = SCHEMA_VERSION + 1;
    assert_eq!(
        validate_work_item(&work),
        Err(CoordinatorError::UnsupportedVersion(SCHEMA_VERSION + 1))
    );
    let query = SpecialistQuery {
        schema_version: SCHEMA_VERSION,
        id: "q".into(),
        requester: "r".into(),
        specialist: "s".into(),
        question: "q".into(),
        context_refs: vec![],
        response_contract: "report-v1".into(),
        deadline_ms: None,
        budget_class: None,
    };
    assert!(validate_consultation(&query).is_ok());
}

#[test]
fn idle_candidate_wins_over_busy_compatible_candidate() {
    let work = item();
    let busy = ParticipantCandidate {
        instance_id: "busy".into(),
        role_profile_id: "role".into(),
        role_version: "1".into(),
        specialization_tags: vec![],
        effective_capability_summary: vec!["repo.read".into()],
        supported_output_contracts: vec!["report-v1".into()],
        current_load: 0,
        current_status: "busy".into(),
        remaining_budget_class: None,
    };
    let idle = ParticipantCandidate {
        instance_id: "idle".into(),
        role_profile_id: "role".into(),
        role_version: "1".into(),
        specialization_tags: vec![],
        effective_capability_summary: vec!["repo.read".into()],
        supported_output_contracts: vec!["report-v1".into()],
        current_load: 9,
        current_status: "idle".into(),
        remaining_budget_class: None,
    };
    assert_eq!(
        propose_assignment(&work, &[busy, idle])
            .unwrap()
            .target_instance_id,
        "idle"
    );
}

#[test]
fn policy_is_fail_closed_and_serialization_hash_is_deterministic() {
    let policy = default_policy();
    validate_policy(&policy).unwrap();
    let mut invalid = policy.clone();
    invalid.max_work_items = MAX_WORK_ITEMS + 1;
    assert_eq!(validate_policy(&invalid), Err(CoordinatorError::Bounds));
    assert_eq!(
        canonical_hash(&policy).unwrap(),
        canonical_hash(&policy).unwrap()
    );
}

#[test]
fn termination_gate_blocks_routing_before_candidate_selection() {
    let mut policy = evohime_core::composable_termination_conditions::TerminationPolicy {
        schema_version: 1,
        id: "stop-policy".into(),
        version: 1,
        expression:
            evohime_core::composable_termination_conditions::TerminationExpression::Condition {
                id: "stop".into(),
                kind: evohime_core::composable_termination_conditions::ConditionKind::StopEvent,
                threshold: 1,
                text: None,
            },
        hard_stop: true,
        content_hash: String::new(),
    };
    policy.content_hash =
        evohime_core::composable_termination_conditions::canonical_hash(&policy).unwrap();
    let state = evohime_core::composable_termination_conditions::TerminationState {
        schema_version: 1,
        policy_version: 1,
        event_cursor: String::new(),
        outcome: evohime_core::composable_termination_conditions::TerminalOutcome::Continue,
        triggered_condition_id: None,
        triggered_event_id: None,
        reason_code: None,
        evidence_refs: vec![],
        version: 1,
    };
    let event = evohime_core::composable_termination_conditions::TerminationEvent {
        event_id: "event-1".into(),
        kind: "stop".into(),
        source: "core".into(),
        messages: 0,
        turns: 0,
        tool_calls: 0,
        input_tokens: 0,
        output_tokens: 0,
        cost_micros: 0,
        elapsed_ms: 0,
        idle_ms: 0,
        goal_state: None,
        workflow_state: None,
        signal: None,
        handoff_reached: false,
    };
    assert_eq!(
        termination_allows_routing(Some(&policy), Some(&state), Some(&event)),
        Err(CoordinatorError::TerminationReached)
    );
}
