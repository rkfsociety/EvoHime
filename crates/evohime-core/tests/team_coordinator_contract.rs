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
