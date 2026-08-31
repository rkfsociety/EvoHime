use evohime_core::agent_role_profiles::{
    effective_grants, AgentRoleProfile, AgentRoleProfilesRegistry, BudgetDefaults, ContractField,
    ExecutionMode,
};

fn profile() -> AgentRoleProfile {
    AgentRoleProfile {
        schema_version: 1,
        id: "tester".into(),
        revision: 1,
        objective: "Run bounded tests".into(),
        constraints: vec!["no_write".into()],
        skills: vec!["test".into()],
        tools: vec!["test.execute".into()],
        strategy: "run_then_report".into(),
        input_contract: vec![ContractField {
            name: "task_ref".into(),
            type_name: "string".into(),
            required: true,
        }],
        output_contract: vec![ContractField {
            name: "result".into(),
            type_name: "string".into(),
            required: true,
        }],
        budget_defaults: BudgetDefaults {
            timeout_ms: 5000,
            max_steps: 4,
            max_output_bytes: 4096,
        },
        execution_mode: ExecutionMode::Human,
    }
}

#[test]
fn run_pins_revision_and_hash() {
    let mut registry = AgentRoleProfilesRegistry::default();
    registry.create(profile(), "create").unwrap();
    let run = registry
        .start(
            "run-1".into(),
            "tester",
            1,
            vec!["test.execute".into()],
            &["test.execute".into()],
            &["test.execute".into()],
            &["test.execute".into()],
        )
        .unwrap();
    assert_eq!(run.snapshot.revision, 1);
    assert!(!run.snapshot.content_hash.is_empty());
}

#[test]
fn requested_grant_cannot_escape_intersection() {
    assert!(effective_grants(
        &["test.execute".into()],
        &["test.execute".into()],
        &["test.execute".into()],
        &["workspace.write".into()]
    )
    .is_err());
}
