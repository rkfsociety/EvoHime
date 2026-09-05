use evohime_core::agent_role_profiles::{
    AgentRoleProfile, AgentRoleProfilesRegistry, BudgetDefaults, ContractField, ExecutionMode,
    RunState, StartRuntimeInput,
};

fn profile() -> AgentRoleProfile {
    AgentRoleProfile {
        schema_version: 1,
        id: "reviewer".into(),
        revision: 1,
        objective: "Review".into(),
        constraints: vec![],
        skills: vec!["review".into()],
        tools: vec!["review".into()],
        strategy: "inspect".into(),
        input_contract: vec![ContractField {
            name: "ref".into(),
            type_name: "string".into(),
            required: true,
        }],
        output_contract: vec![ContractField {
            name: "status".into(),
            type_name: "string".into(),
            required: true,
        }],
        budget_defaults: BudgetDefaults {
            timeout_ms: 1000,
            max_steps: 1,
            max_output_bytes: 1000,
        },
        execution_mode: ExecutionMode::Ai,
    }
}

#[test]
fn duplicate_start_and_cancel_are_typed() {
    let mut r = AgentRoleProfilesRegistry::default();
    r.create(profile(), "c").unwrap();
    let first = r
        .start(StartRuntimeInput {
            run_id: "run".into(),
            profile_id: "reviewer",
            revision: 1,
            grants: vec!["review".into()],
            parent: &["review".into()],
            policy: &["review".into()],
            registry: &["review".into()],
        })
        .unwrap();
    assert_eq!(first.state, RunState::Pinned);
    assert!(r
        .start(StartRuntimeInput {
            run_id: "run".into(),
            profile_id: "reviewer",
            revision: 1,
            grants: vec!["review".into()],
            parent: &["review".into()],
            policy: &["review".into()],
            registry: &["review".into()],
        })
        .is_err());
    assert_eq!(r.cancel("run").unwrap().state, RunState::Cancelling);
}
