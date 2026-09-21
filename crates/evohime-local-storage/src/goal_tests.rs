
use super::*;

fn goal() -> GoalV1 {
    GoalV1 {
        id: "goal-1".into(),
        version: 1,
        workspace_id: "workspace-1".into(),
        chat_id: Some("chat-1".into()),
        objective: "Собрать доказуемый результат".into(),
        success_criteria: vec![GoalCriterionV1::new(
            "criterion-1",
            GoalCriterionKind::Manual,
            "Проверить итог",
        )],
        status: GoalStatus::Active,
        progress_summary: "Ожидает проверки".into(),
        completed_criteria: Vec::new(),
        remaining_criteria: vec!["criterion-1".into()],
        blockers: Vec::new(),
        next_action: Some("Проверить результат".into()),
        workflow_run_ids: Vec::new(),
        child_run_ids: Vec::new(),
        checkpoint_id: None,
        token_budget: Some(100),
        cost_budget_micros: None,
        continuation_budget: Some(2),
        created_at_ms: 1,
        updated_at_ms: 1,
        created_by: "shell".into(),
        updated_by: "shell".into(),
        content_hash: String::new(),
    }
}

#[test]
fn canonical_hash_is_stable_and_model_text_is_not_authority() {
    let sealed = goal().seal().expect("goal seals");
    assert_eq!(sealed.compute_content_hash().unwrap(), sealed.content_hash);
    let mut altered = sealed.clone();
    altered.progress_summary.push_str(" позже");
    assert!(matches!(
        altered.validate(),
        Err(GoalError::ContentHashMismatch { .. })
    ));
    let mut completed = sealed;
    completed.status = GoalStatus::Completed;
    assert!(matches!(
        completed.seal(),
        Err(GoalError::CompletionEvidenceMissing)
    ));
    let mut sensitive = goal();
    sensitive.objective = "доставить token: секрет".into();
    assert!(matches!(
        sensitive.seal(),
        Err(GoalError::SensitiveText { field }) if field == "objective"
    ));
    let mut model_verified = goal();
    model_verified.success_criteria[0].status = GoalCriterionStatus::Verified;
    model_verified.success_criteria[0].evidence_ref = Some("test-run-1".into());
    model_verified.success_criteria[0].verifier_id = Some("model".into());
    model_verified.success_criteria[0].verifier_version = Some("1".into());
    model_verified.success_criteria[0].verified_at_ms = Some(2);
    assert!(matches!(
        model_verified.seal(),
        Err(GoalError::AuthorityViolation { .. })
    ));
}

#[test]
fn transitions_and_evidence_are_core_authoritative() {
    assert!(GoalStatus::Active.allows_transition_to(GoalStatus::Paused));
    assert!(!GoalStatus::Completed.allows_transition_to(GoalStatus::Active));
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = GoalStore::new(&connection);
    let created = store
        .create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-1", &"1".repeat(64)),
        )
        .unwrap();
    let verified = store
        .verify_criterion(
            "goal-1",
            created.goal.version,
            GoalCriterionEvidence::new("criterion-1", "test-run-1", "core:tests", "v1"),
            GoalCommand::new("core", "request-2", &"2".repeat(64)),
        )
        .unwrap();
    assert_eq!(verified.goal.status, GoalStatus::Completed);
    assert_eq!(store.get("goal-1").unwrap().unwrap().version, 2);
}

#[test]
fn storage_is_transactional_idempotent_and_rejects_stale_writes() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = GoalStore::new(&connection);
    let first = store
        .create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-1", &"1".repeat(64)),
        )
        .unwrap();
    let replay = store
        .create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-1", &"1".repeat(64)),
        )
        .unwrap();
    assert!(replay.deduplicated);
    assert!(matches!(
        store.transition(
            "goal-1",
            first.goal.version - 1,
            GoalStatus::Paused,
            GoalCommand::new("shell", "request-2", &"2".repeat(64))
        ),
        Err(StorageError::VersionConflict { .. })
    ));
    assert!(matches!(
        store.create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-3", &"3".repeat(64))
        ),
        Err(StorageError::Goal(GoalError::AlreadyExists(_)))
    ));
}

#[test]
fn stored_projection_rejects_sql_metadata_drift() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = GoalStore::new(&connection);
    store
        .create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-1", &"1".repeat(64)),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE goals SET objective = 'tampered' WHERE id = 'goal-1'",
            [],
        )
        .unwrap();
    assert!(matches!(
        store.get("goal-1"),
        Err(StorageError::Goal(GoalError::InvalidStored(_)))
    ));
}

#[test]
fn links_multiple_runtime_refs_and_persists_budget_recovery_state() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = GoalStore::new(&connection);
    let created = store
        .create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-1", &"1".repeat(64)),
        )
        .unwrap();
    let workflow = store
        .link_reference(
            "goal-1",
            created.goal.version,
            "workflow",
            "workflow-1",
            GoalCommand::new("core", "request-2", &"2".repeat(64)),
        )
        .unwrap();
    let child = store
        .link_reference(
            "goal-1",
            workflow.goal.version,
            "child",
            "child-1",
            GoalCommand::new("core", "request-3", &"3".repeat(64)),
        )
        .unwrap();
    let checkpoint = store
        .link_reference(
            "goal-1",
            child.goal.version,
            "checkpoint",
            "checkpoint-1",
            GoalCommand::new("core", "request-4", &"4".repeat(64)),
        )
        .unwrap();
    assert_eq!(checkpoint.goal.workflow_run_ids, vec!["workflow-1"]);
    assert_eq!(checkpoint.goal.child_run_ids, vec!["child-1"]);
    assert_eq!(
        checkpoint.goal.checkpoint_id.as_deref(),
        Some("checkpoint-1")
    );
    let budget_limited = store
        .transition(
            "goal-1",
            checkpoint.goal.version,
            GoalStatus::BudgetLimited,
            GoalCommand::new("core", "request-5", &"5".repeat(64)),
        )
        .unwrap();
    let recovery = store.recovery("workspace-1").unwrap();
    assert_eq!(budget_limited.goal.status, GoalStatus::BudgetLimited);
    assert!(recovery[0].warning.contains("лимитом бюджета"));
    let revisions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM goal_revisions WHERE goal_id = 'goal-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 5);
}

#[test]
fn objective_update_creates_an_immutable_revision() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = GoalStore::new(&connection);
    let created = store
        .create(
            &goal().seal().unwrap(),
            GoalCommand::new("shell", "request-1", &"1".repeat(64)),
        )
        .unwrap();
    let updated = store
        .update(
            "goal-1",
            created.goal.version,
            Some("Обновлённая цель".into()),
            None,
            GoalCommand::new("shell", "request-2", &"2".repeat(64)),
        )
        .unwrap();
    assert_eq!(updated.goal.version, 2);
    assert_eq!(updated.goal.objective, "Обновлённая цель");
    let old_objective: String = connection
        .query_row(
            "SELECT json_extract(canonical_json, '$.objective')
                 FROM goal_revisions WHERE goal_id = 'goal-1' AND version = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(old_objective, "Собрать доказуемый результат");
}

#[test]
fn durable_goal_survives_reopen_without_replaying_an_effect() {
    let path = std::env::temp_dir().join(format!("evohime-goal-reopen-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let connection = Connection::open(&path).unwrap();
        install_schema(&connection).unwrap();
        GoalStore::new(&connection)
            .create(
                &goal().seal().unwrap(),
                GoalCommand::new("shell", "request-1", &"1".repeat(64)),
            )
            .unwrap();
    }
    {
        let connection = Connection::open(&path).unwrap();
        install_schema(&connection).unwrap();
        let recovered = GoalStore::new(&connection).recovery("workspace-1").unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].status, GoalStatus::Active);
        assert!(recovered[0].warning.is_empty());
        assert_eq!(
            GoalStore::new(&connection)
                .get("goal-1")
                .unwrap()
                .unwrap()
                .version,
            1
        );
    }
    let _ = std::fs::remove_file(path);
}
