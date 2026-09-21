use super::*;

#[test]
fn appends_and_replays_events_by_sequence() {
    let path = temp_database_path("events");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let first = database
        .append_event("task-1", "task.started", b"one")
        .expect("first event");
    let second = database
        .append_event("task-1", "task.completed", b"two")
        .expect("second event");
    let events = database.read_events_after(first, 10).expect("events read");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence_id, second);
    assert_eq!(events[0].payload, b"two");
    let task_events = database
        .read_task_events("task-1", 10)
        .expect("task events read");
    assert_eq!(task_events.len(), 2);
    assert_eq!(task_events[0].sequence_id, first);
    assert_eq!(task_events[1].sequence_id, second);
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn exports_events_as_jsonl() {
    let path = temp_database_path("export");
    let output = path.with_extension("jsonl");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&output);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .append_event("task-export", "task.started", br#"{"ok":true}"#)
        .expect("event writes");
    database
        .export_events_jsonl(&output)
        .expect("export writes");
    let content = std::fs::read_to_string(&output).expect("export reads");
    let record: serde_json::Value = serde_json::from_str(content.trim()).expect("valid JSON");
    assert_eq!(record["task_id"], "task-export");
    assert_eq!(record["payload"]["ok"], true);
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(output);
}

#[test]
fn diagnostics_summary_is_bounded_read_only_and_counts_tables_and_events() {
    let path = temp_database_path("diagnostics-summary");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("diagnostics-project", "Diagnostics", "C:\\workspace", None)
        .expect("project creates");
    database
        .append_event("task-1", "task.started", b"one")
        .expect("first event writes");
    database
        .append_event("task-1", "task.started", b"two")
        .expect("second event writes");
    database
        .append_event("task-1", "task.completed", b"three")
        .expect("third event writes");

    let before_version = database.schema_version().expect("schema version reads");
    let summary: DiagnosticsSummary = database
        .read_diagnostics_summary(1)
        .expect("diagnostics summary reads");

    assert_eq!(summary.total_events, 3);
    assert_eq!(summary.event_counts.len(), 1);
    assert_eq!(summary.event_counts[0].event_type, "task.started");
    assert_eq!(summary.event_counts[0].rows, 2);
    assert!(summary.event_types_truncated);
    assert_eq!(summary.table_counts.len(), 24);
    assert_eq!(
        summary
            .table_counts
            .iter()
            .find(|count| count.table == "projects")
            .expect("projects count exists")
            .rows,
        1
    );
    assert_eq!(
        summary
            .table_counts
            .iter()
            .find(|count| count.table == "events")
            .expect("events count exists")
            .rows,
        3
    );
    assert_eq!(
        database.schema_version().expect("schema version reads"),
        before_version
    );
    assert_eq!(
        database
            .read_events_after(0, 10)
            .expect("events remain readable")
            .len(),
        3
    );

    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn creates_and_updates_task_with_optimistic_version() {
    let path = temp_database_path("tasks");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-1", "Demo", "C:\\Projects\\demo", None)
        .expect("project creates");
    let item = WorkItemRecord {
        id: "work-1".into(),
        project_id: "project-1".into(),
        parent_id: None,
        title: "First task".into(),
        description: "desc".into(),
        source_ref: Some("prd:1".into()),
        acceptance_criteria: "tests pass".into(),
        non_goals: "no UI".into(),
        status: "backlog".into(),
        priority: 10,
        estimate: Some(2),
        complexity: Some("small".into()),
        attempt_count: 0,
        version: 1,
    };
    let created = database.create_work_item(&item).expect("task creates");
    let updated = database
        .update_work_item_status(&created.id, 1, "ready")
        .expect("task updates");
    assert_eq!(updated.status, "ready");
    assert_eq!(updated.version, 2);
    assert!(matches!(
        database.update_work_item_status(&created.id, 1, "done"),
        Err(StorageError::VersionConflict { .. })
    ));
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn lists_graph_rejects_cycles_and_selects_next_ready_deterministically() {
    let path = temp_database_path("task-graph");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-graph", "Graph", "C:\\Projects\\graph", None)
        .expect("project creates");
    for (id, title, status, priority) in [
        ("task-a", "A", "ready", 1),
        ("task-b", "B", "ready", 10),
        ("task-c", "C", "done", 100),
    ] {
        database
            .create_work_item(&WorkItemRecord {
                id: id.into(),
                project_id: "project-graph".into(),
                parent_id: None,
                title: title.into(),
                description: String::new(),
                source_ref: None,
                acceptance_criteria: String::new(),
                non_goals: String::new(),
                status: status.into(),
                priority,
                estimate: None,
                complexity: None,
                attempt_count: 0,
                version: 1,
            })
            .expect("task creates");
    }
    database
        .add_dependency("task-a", "task-c", "blocks")
        .expect("dependency creates");
    assert!(matches!(
        database.add_dependency("task-c", "task-a", "blocks"),
        Err(StorageError::DependencyCycle { .. })
    ));
    assert_eq!(database.list_work_items("project-graph").unwrap().len(), 3);
    assert_eq!(
        database.list_dependencies("project-graph").unwrap().len(),
        1
    );
    assert_eq!(
        database.next_ready("project-graph").unwrap().unwrap().id,
        "task-b"
    );
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn imports_prd_atomically_and_preserves_provenance() {
    let path = temp_database_path("prd-import");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-prd", "PRD", "C:\\Projects\\prd", None)
        .expect("project creates");
    let source = "# Plan\n\n## Imported\nDescription\n- [ ] Verify\n";
    let tasks = [ImportedTask {
        id: "import-task-1".into(),
        title: "Imported".into(),
        description: "Description".into(),
        source_ref: "prd.md#L3".into(),
        acceptance_criteria: "Verify".into(),
    }];
    let imported = database
        .import_prd("import-1", "project-prd", "prd.md", "v7", source, &tasks)
        .expect("PRD imports");
    assert_eq!(imported[0].status, "backlog");
    let provenance = database
        .get_provenance("import-1")
        .expect("provenance reads")
        .expect("provenance exists");
    assert_eq!(provenance.kind, "prd_import");
    assert_eq!(provenance.source, "prd.md");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&provenance.payload).unwrap()["version"],
        "v7"
    );
    assert!(database
        .import_prd("import-1", "project-prd", "prd.md", "v7", source, &tasks)
        .is_err());
    assert_eq!(database.list_work_items("project-prd").unwrap().len(), 1);
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn persists_run_linked_snapshot_payload_immutably() {
    let path = temp_database_path("snapshots");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let saved = database
        .save_snapshot("snapshot-1", "run-1", "workspace-hash", br#"{"files":[]}"#)
        .expect("snapshot saves");
    assert_eq!(saved.run_id, "run-1");
    assert_eq!(database.get_snapshot("snapshot-1").unwrap(), Some(saved));
    assert!(database
        .save_snapshot("snapshot-1", "run-2", "other", b"changed")
        .is_err());
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn persists_project_policy_with_optimistic_versioning() {
    let path = temp_database_path("project-policy");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-policy", "Policy", ".", None)
        .expect("project creates");
    let first = database
        .upsert_project_policy("project-policy", br#"{"timeout_ms":30000}"#, None)
        .expect("policy creates");
    assert_eq!(first.version, 1);
    let second = database
        .upsert_project_policy("project-policy", br#"{"timeout_ms":15000}"#, Some(1))
        .expect("policy updates");
    assert_eq!(second.version, 2);
    assert!(matches!(
        database.upsert_project_policy("project-policy", b"{}", Some(1)),
        Err(StorageError::VersionConflict {
            entity: "project_policy",
            ..
        })
    ));
    assert_eq!(
        database
            .get_project_policy("project-policy")
            .unwrap()
            .unwrap(),
        second
    );
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn checkpoints_and_unknown_effects_recover_without_retry() {
    let path = temp_database_path("recovery");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-recovery", "Recovery", ".", None)
        .expect("project creates");
    let task = WorkItemRecord {
        id: "task-recovery".into(),
        project_id: "project-recovery".into(),
        parent_id: None,
        title: "recover me".into(),
        description: String::new(),
        source_ref: None,
        acceptance_criteria: String::new(),
        non_goals: String::new(),
        status: "in_progress".into(),
        priority: 0,
        estimate: None,
        complexity: None,
        attempt_count: 0,
        version: 1,
    };
    database.create_work_item(&task).expect("task creates");
    let run = RunRecord {
        id: "run-recovery".into(),
        work_item_id: task.id.clone(),
        status: "running".into(),
        policy_snapshot: vec![],
        role_snapshot: vec![],
        skill_snapshot: vec![],
        model_route_snapshot: vec![],
    };
    let checkpoint = RunCheckpointRecord {
        run_id: run.id.clone(),
        checkpoint_id: "checkpoint-1".into(),
        stage: "build".into(),
        node_id: "node-1".into(),
        attempt: 1,
        input_hash: "input-hash".into(),
        state_json: br#"{"stage":"build"}"#.to_vec(),
        pending_effects_json: br#"["effect-1"]"#.to_vec(),
        committed_at: "2026-01-01T00:00:00Z".into(),
    };
    let effect = RunEffectRecord {
        effect_id: "effect-1".into(),
        run_id: run.id.clone(),
        node_id: "node-1".into(),
        kind: "bounded_build".into(),
        idempotency_key: "run-recovery:build".into(),
        immutable_intent_hash: "intent-hash".into(),
        state: "prepared".into(),
        started_at: None,
        completed_at: None,
        result_hash: None,
    };
    database
        .prepare_run_effect(&run, &checkpoint, &effect)
        .expect("effect prepares");
    database
        .mark_effect_executing("effect-1")
        .expect("effect starts");
    drop(database);
    let database = LocalDatabase::open(&path).expect("database reopens after restart");
    let recovered = database.recover_unknown_effects().expect("recovery runs");
    assert_eq!(recovered.len(), 1);
    assert_eq!(
        database.get_run("run-recovery").unwrap().unwrap().status,
        "blocked"
    );
    assert_eq!(
        database
            .latest_checkpoint("run-recovery")
            .unwrap()
            .unwrap()
            .checkpoint_id,
        "checkpoint-1"
    );
    assert_eq!(
        database.read_task_events(&task.id, 10).unwrap()[0].event_type,
        "run.recovery.blocked"
    );
    assert!(
        database.recover_unknown_effects().unwrap().is_empty(),
        "recovery is idempotent"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn run_lease_is_single_owner_and_effect_can_be_reconciled() {
    let path = temp_database_path("leases");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-lease", "Lease", ".", None)
        .expect("project creates");
    database
        .create_work_item(&WorkItemRecord {
            id: "task-lease".into(),
            project_id: "project-lease".into(),
            parent_id: None,
            title: "lease".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "in_progress".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        })
        .expect("task creates");
    let run = RunRecord {
        id: "run-lease".into(),
        work_item_id: "task-lease".into(),
        status: "running".into(),
        policy_snapshot: vec![],
        role_snapshot: vec![],
        skill_snapshot: vec![],
        model_route_snapshot: vec![],
    };
    let checkpoint = RunCheckpointRecord {
        run_id: run.id.clone(),
        checkpoint_id: "checkpoint-lease".into(),
        stage: "build".into(),
        node_id: "bounded-build".into(),
        attempt: 1,
        input_hash: "intent".into(),
        state_json: b"{}".to_vec(),
        pending_effects_json: br#"["effect-lease"]"#.to_vec(),
        committed_at: String::new(),
    };
    let effect = RunEffectRecord {
        effect_id: "effect-lease".into(),
        run_id: run.id.clone(),
        node_id: "bounded-build".into(),
        kind: "bounded_build".into(),
        idempotency_key: "lease-key".into(),
        immutable_intent_hash: "intent".into(),
        state: "prepared".into(),
        started_at: None,
        completed_at: None,
        result_hash: None,
    };
    database
        .prepare_run_effect(&run, &checkpoint, &effect)
        .expect("effect prepares");
    database
        .acquire_run_lease("run-lease", "lease-1", "core-a", 1, 30)
        .expect("first owner claims");
    assert!(matches!(
        database.acquire_run_lease("run-lease", "lease-2", "core-b", 2, 30),
        Err(StorageError::InvalidRunEffect(_))
    ));
    database
        .heartbeat_run_lease("run-lease", "lease-1", "core-a", 1, 30)
        .expect("owner heartbeats");
    database
        .mark_effect_executing("effect-lease")
        .expect("effect executes");
    database
        .recover_unknown_effects()
        .expect("unknown effect recovers");
    let reconciliation = database
        .reconcile_run_effect(
            "effect-lease",
            true,
            "snapshot",
            br#"{"snapshot_id":"snapshot-1"}"#,
        )
        .expect("effect reconciles");
    assert_eq!(reconciliation.state, "reconciled_success");
    let retry = database
        .reconcile_run_effect(
            "effect-lease",
            false,
            "different-verifier",
            br#"{"changed":true}"#,
        )
        .expect("duplicate reconciliation is idempotent");
    assert_eq!(retry, reconciliation);
    assert_eq!(
        database
            .get_run_effect("effect-lease")
            .unwrap()
            .unwrap()
            .state,
        "completed_success"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn agent_run_effect_has_an_independent_lease_and_completes() {
    let path = temp_database_path("agent-run-lease");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let effect = RunEffectRecord {
        effect_id: "agent-effect-1".into(),
        run_id: "agent-run-1".into(),
        node_id: "agent-task".into(),
        kind: "agent_task".into(),
        idempotency_key: "agent-run-1:agent-task".into(),
        immutable_intent_hash: "intent-agent".into(),
        state: "prepared".into(),
        started_at: None,
        completed_at: None,
        result_hash: None,
    };
    database
        .prepare_agent_run_effect(&effect, "shell-task-1")
        .expect("agent effect prepares");
    database
        .acquire_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1, 30)
        .expect("agent lease claims");
    database
        .heartbeat_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1, 30)
        .expect("agent lease heartbeats");
    database
        .mark_agent_effect_executing("agent-effect-1")
        .expect("agent effect executes");
    let completed = database
        .complete_agent_run_effect("agent-effect-1", true, Some("result"))
        .expect("agent effect completes");
    assert_eq!(completed.state, "completed_success");
    database
        .release_agent_run_lease("agent-run-1", "agent-lease-1", "core", 1)
        .expect("agent lease releases");
    assert!(database
        .get_agent_run_lease("agent-run-1")
        .unwrap()
        .is_none());
    let _ = std::fs::remove_file(path);
}

#[test]
fn deduplicates_same_request_and_rejects_reused_request_id() {
    let path = temp_database_path("dedup");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    assert_eq!(
        database
            .record_deduplicated("client", "request", "hash", b"ok")
            .expect("first write"),
        None
    );
    assert_eq!(
        database
            .record_deduplicated("client", "request", "hash", b"different")
            .expect("replay"),
        Some(b"ok".to_vec())
    );
    assert!(matches!(
        database.record_deduplicated("client", "request", "other", b"bad"),
        Err(StorageError::DeduplicationConflict { .. })
    ));
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn persists_immutable_run_snapshots() {
    let path = temp_database_path("run-snapshots");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-run", "Run project", "C:\\Projects\\run", None)
        .expect("project creates");
    database
        .create_work_item(&WorkItemRecord {
            id: "task-run".into(),
            project_id: "project-run".into(),
            parent_id: None,
            title: "Run task".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "ready".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        })
        .expect("task creates");
    let run = RunRecord {
        id: "run-1".into(),
        work_item_id: "task-run".into(),
        status: "queued".into(),
        policy_snapshot: br#"{"max_iterations":1}"#.to_vec(),
        role_snapshot: br#"{"id":"planner","version":1}"#.to_vec(),
        skill_snapshot: br#"{"id":"native","version":1}"#.to_vec(),
        model_route_snapshot: br#"{"route":"local-first"}"#.to_vec(),
    };
    assert_eq!(database.create_run(&run).expect("run creates"), run);
    assert!(
        database.create_run(&run).is_err(),
        "run snapshot is immutable"
    );
    assert_eq!(database.get_run("run-1").expect("run reads"), Some(run));
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn round_trips_typed_snapshot_contracts() {
    let path = temp_database_path("typed-snapshots");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-typed", "Typed", "C:\\Projects\\typed", None)
        .expect("project creates");
    database
        .create_work_item(&WorkItemRecord {
            id: "task-typed".into(),
            project_id: "project-typed".into(),
            parent_id: None,
            title: "Typed task".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "ready".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        })
        .expect("task creates");
    let snapshots = RunSnapshots {
        role_ref: RoleRef {
            id: "planner".into(),
            version: "1".into(),
            hash: "role-hash".into(),
        },
        skill_ref: SkillRef {
            id: "native".into(),
            version: "2".into(),
            hash: "skill-hash".into(),
        },
        policy: PolicySnapshot {
            schema_version: 1,
            policy_version: 3,
            effective_permissions_hash: "permissions-hash".into(),
            canonical_json: br#"{"tools":["filesystem.read"]}"#.to_vec(),
        },
        model_route: ModelRouteSnapshot {
            requested_route: "local-first".into(),
            resolved_provider: "mock".into(),
            resolved_model: "test-model".into(),
            route_policy_version: 1,
            canonical_json: br#"{"route":"local-first"}"#.to_vec(),
        },
    };
    database
        .create_run_with_snapshots("run-typed", "task-typed", "queued", &snapshots)
        .expect("typed run creates");
    assert_eq!(
        database
            .get_run_snapshots("run-typed")
            .expect("typed run reads"),
        Some(snapshots)
    );
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn recovery_transitions_are_durable_and_audited_without_retry() {
    let path = temp_database_path("recovery-state-machine");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .create_project("project-state", "State", ".", None)
        .expect("project creates");
    database
        .create_work_item(&WorkItemRecord {
            id: "task-state".into(),
            project_id: "project-state".into(),
            parent_id: None,
            title: "State task".into(),
            description: String::new(),
            source_ref: None,
            acceptance_criteria: String::new(),
            non_goals: String::new(),
            status: "ready".into(),
            priority: 0,
            estimate: None,
            complexity: None,
            attempt_count: 0,
            version: 1,
        })
        .expect("task creates");
    database
        .create_run(&RunRecord {
            id: "run-state".into(),
            work_item_id: "task-state".into(),
            status: "running".into(),
            policy_snapshot: Vec::new(),
            role_snapshot: Vec::new(),
            skill_snapshot: Vec::new(),
            model_route_snapshot: Vec::new(),
        })
        .expect("run creates");

    database
        .transition_recovery(RecoveryTransitionInput {
            run_id: "run-state",
            next: RecoveryState::Recovering,
            effect_id: "effect-state",
            idempotency_key: "run-state:effect-state",
            verifier: "startup",
            evidence_json: br#"{"reason":"process_restart"}"#,
            decision: "recovery_started",
        })
        .expect("recovering transition");
    database
        .transition_recovery(RecoveryTransitionInput {
            run_id: "run-state",
            next: RecoveryState::Reconciling,
            effect_id: "effect-state",
            idempotency_key: "run-state:effect-state:reconciling",
            verifier: "file_hash",
            evidence_json: br#"{"path":"src/lib.rs"}"#,
            decision: "verifier_started",
        })
        .expect("reconciling transition");
    let blocked = database
        .transition_recovery(RecoveryTransitionInput {
            run_id: "run-state",
            next: RecoveryState::Blocked,
            effect_id: "effect-state",
            idempotency_key: "run-state:effect-state:blocked",
            verifier: "file_hash",
            evidence_json: br#"{"match":false}"#,
            decision: "outcome_unconfirmed",
        })
        .expect("blocked transition");
    assert_eq!(blocked.state, RecoveryState::Blocked);
    assert_eq!(
        database.latest_recovery("run-state").expect("latest reads"),
        Some(blocked)
    );
    let repeated = database
        .transition_recovery(RecoveryTransitionInput {
            run_id: "run-state",
            next: RecoveryState::Blocked,
            effect_id: "effect-state",
            idempotency_key: "run-state:effect-state:blocked",
            verifier: "file_hash",
            evidence_json: br#"{"match":false}"#,
            decision: "outcome_unconfirmed",
        })
        .expect("repeated decision is idempotent");
    assert_eq!(
        repeated.id,
        database
            .latest_recovery("run-state")
            .expect("latest reads")
            .expect("record exists")
            .id
    );
    assert!(matches!(
        database.transition_recovery(RecoveryTransitionInput {
            run_id: "run-state",
            next: RecoveryState::Resumable,
            effect_id: "effect-state",
            idempotency_key: "run-state:effect-state:blind-retry",
            verifier: "file_hash",
            evidence_json: br#"{}"#,
            decision: "blind_retry",
        }),
        Err(StorageError::InvalidRecovery(_))
    ));
    let events = database.read_events_after(0, 10).expect("events read");
    assert_eq!(
        events
            .iter()
            .filter(|event| event.event_type == "run.recovery.decision")
            .count(),
        3
    );
    drop(database);
    let _ = std::fs::remove_file(path);
}

#[test]
fn migration_12_is_idempotent_and_preserves_pre_existing_memory_rows() {
    // Reproduces the pre-wave-VI state: a v8 `memory_entries` table
    // (no `confirmations` / `lesson_key`) with one real row already in
    // it, then confirms the 11 -> 12 migration both preserves that row
    // and can be re-applied (guarded re-open) without altering the
    // already-migrated columns a second time.
    let path = temp_database_path("migration-12-idempotent");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL
                    );
                    CREATE INDEX IF NOT EXISTS idx_memory_entries_scope
                        ON memory_entries(scope_kind, scope_id);
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten)
                    VALUES
                        ('pre-existing', 'project', 'scope-a', 'Old title', 'Old content',
                         'task:pre-wave-vi', 'internal', '2026-01-01T00:00:00Z', NULL, 0, 0);
                    PRAGMA user_version = 11;",
            )
            .expect("v8-shaped legacy memory table seeds");
    }

    let database = LocalDatabase::open(&path).expect("database migrates 11 -> 12");
    assert_eq!(
        database.schema_version().expect("version reads"),
        SCHEMA_VERSION
    );
    let (confirmations, lesson_key): (i64, Option<String>) = database
        .connection()
        .query_row(
            "SELECT confirmations, lesson_key FROM memory_entries WHERE id = 'pre-existing'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("pre-existing row survives migration with new columns defaulted");
    assert_eq!(confirmations, 1, "DEFAULT 1 applied to pre-existing rows");
    assert_eq!(lesson_key, None);
    let title: String = database
        .connection()
        .query_row(
            "SELECT title FROM memory_entries WHERE id = 'pre-existing'",
            [],
            |row| row.get(0),
        )
        .expect("original content untouched by migration");
    assert_eq!(title, "Old title");
    drop(database);

    // Re-opening an already-migrated database must not re-run the
    // ALTER TABLE (which would error on a duplicate column) and must
    // not disturb existing data.
    let database = LocalDatabase::open(&path).expect("re-open is idempotent");
    assert_eq!(
        database.schema_version().expect("version reads"),
        SCHEMA_VERSION
    );
    let confirmations_after_reopen: i64 = database
        .connection()
        .query_row(
            "SELECT confirmations FROM memory_entries WHERE id = 'pre-existing'",
            [],
            |row| row.get(0),
        )
        .expect("row still present after idempotent re-open");
    assert_eq!(confirmations_after_reopen, 1);
    let row_count: i64 = database
        .connection()
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
        .expect("count reads");
    assert_eq!(row_count, 1, "no duplicate rows created by re-migration");

    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
}

#[test]
fn research_and_memory_stores_round_trip_against_shared_migrated_database() {
    use crate::memory_store::{
        MemoryPrivacy, MemoryRecord, MemoryRecordInput, MemoryScope, MemoryStoreSql,
    };
    use crate::research_store::{ResearchEvidenceRecord, ResearchEvidenceSql};

    let path = temp_database_path("bounded-stores");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    assert_eq!(
        database.schema_version().expect("version reads"),
        SCHEMA_VERSION
    );

    let evidence = ResearchEvidenceRecord {
        id: "evidence-1".into(),
        source_kind: "url".into(),
        source_ref: "https://example.test/source".into(),
        redacted_excerpt: "redacted result".into(),
        source_hash: "sha256:abc".into(),
        fetched_at: "2026-08-12T10:00:00Z".into(),
        ttl_seconds: 3600,
        provenance_link: Some("run:shared-db".into()),
    };
    ResearchEvidenceSql::insert(database.connection(), &evidence)
        .expect("evidence inserts against shared connection");
    assert_eq!(
        ResearchEvidenceSql::get_by_id(database.connection(), "evidence-1")
            .expect("evidence reads"),
        Some(evidence)
    );
    assert_eq!(
        ResearchEvidenceSql::list_by_provenance(database.connection(), "run:shared-db")
            .expect("evidence lists")
            .len(),
        1
    );

    let memory = MemoryRecord::new(MemoryRecordInput {
        id: "memory-1".into(),
        scope: MemoryScope::Project,
        scope_id: "project-shared-db".into(),
        title: "Decision".into(),
        content: "keep this fact".into(),
        provenance: "run:shared-db".into(),
        privacy: MemoryPrivacy::Internal,
        created_at: "2026-08-12T10:00:00Z".into(),
        expires_at: Some("2027-01-01T00:00:00Z".into()),
    })
    .expect("memory record builds");
    MemoryStoreSql::insert(database.connection(), &memory)
        .expect("memory inserts against shared connection");
    assert_eq!(
        MemoryStoreSql::get_by_id(database.connection(), "memory-1").expect("memory reads"),
        Some(memory)
    );
    let found = MemoryStoreSql::search(
        database.connection(),
        MemoryScope::Project,
        "project-shared-db",
        "fact",
        "2026-09-01T00:00:00Z",
        10,
    )
    .expect("memory search");
    assert_eq!(found.len(), 1);
    assert!(MemoryStoreSql::archive(database.connection(), "memory-1").expect("archive"));
    assert!(MemoryStoreSql::forget(database.connection(), "memory-1").expect("forget"));

    drop(database);
    let _ = std::fs::remove_file(&path);
}

fn sample_ledger_event(
    event_id: &str,
    run_id: &str,
    action_id: &str,
    state_after: crate::execution_ledger::ActionState,
) -> crate::execution_ledger::ExecutionEventV1 {
    crate::execution_ledger::ExecutionEventV1 {
        schema_version: 1,
        event_id: event_id.to_string(),
        sequence_id: None,
        run_scope: crate::execution_ledger::RunScope::Workflow,
        run_id: run_id.to_string(),
        session_id: Some("session-1".into()),
        task_id: "task-ledger".into(),
        created_at_ms: 1_700_000_000_000,
        state_after: Some(state_after),
        action_id: Some(action_id.to_string()),
        tool_call_id: None,
        observation_id: None,
        receipt_id: None,
        failure_id: None,
        workflow_run_id: Some(run_id.to_string()),
        node_id: Some("node-1".into()),
        attempt_id: None,
        effect_id: None,
        model_request_id: None,
        body: crate::execution_ledger::ExecutionEventBody::ToolCall {
            tool_name: "shell".into(),
            tool_call_hash: "hash".into(),
            manifest_hash: None,
        },
        redaction: crate::execution_ledger::RedactionMeta::default(),
    }
}

/// План 08-4 acceptance: legacy `events` rows (written before 08-1, or
/// still written by callers that never adopt the typed ledger) get a
/// deterministic `event_id` via `execution_ledger::legacy_event_id`,
/// without the original row — or its `sequence_id` — ever being
/// touched. Recomputing the mapping from the durably stored row must
/// reproduce the exact same id every time.
#[test]
fn legacy_event_mapping_is_reproducible_and_preserves_the_original_row() {
    let path = temp_database_path("ledger-legacy-mapping");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let sequence_id = database
        .append_event("legacy-task", "task.completed", b"{\"ok\":true}")
        .expect("legacy event appends through the pre-08-1 path");

    let stored = database
        .read_events_after(sequence_id - 1, 1)
        .expect("read back")
        .remove(0);
    // Legacy rows never get the typed columns populated.
    assert_eq!(stored.sequence_id, sequence_id);
    assert_eq!(stored.task_id, "legacy-task");
    assert_eq!(stored.event_type, "task.completed");

    let mapped_once = crate::execution_ledger::legacy_event_id(
        stored.sequence_id,
        &stored.task_id,
        &stored.event_type,
        &stored.payload,
        &stored.created_at,
    );
    let mapped_again = crate::execution_ledger::legacy_event_id(
        stored.sequence_id,
        &stored.task_id,
        &stored.event_type,
        &stored.payload,
        &stored.created_at,
    );
    assert_eq!(
        mapped_once, mapped_again,
        "mapping the same durable row twice must reproduce the same event_id"
    );
    assert_eq!(
        mapped_once.len(),
        crate::execution_ledger::LEGACY_EVENT_ID_HEX_LEN
    );

    // Re-reading the row after the mapping was computed proves the row
    // itself — sequence_id included — was never rewritten.
    let reread = database
        .read_events_after(sequence_id - 1, 1)
        .expect("re-read")
        .remove(0);
    assert_eq!(reread, stored);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn append_ledger_event_round_trips_through_events_table() {
    let path = temp_database_path("ledger-append");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let event = sample_ledger_event(
        "event-1",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Running,
    );
    let sequence_id = database
        .append_ledger_event(&event)
        .expect("typed event appends");

    let stored = database
        .read_events_after(sequence_id - 1, 1)
        .expect("read back")
        .remove(0);
    let round_tripped: crate::execution_ledger::ExecutionEventV1 =
        serde_json::from_slice(&stored.payload).expect("payload decodes");
    assert_eq!(round_tripped, event);
    assert_eq!(stored.event_type, "ledger.tool_call");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn append_ledger_events_batches_rows_in_one_commit() {
    let path = temp_database_path("ledger-append-batch");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let first = sample_ledger_event(
        "batch-event-1",
        "batch-run",
        "batch-action-1",
        crate::execution_ledger::ActionState::Running,
    );
    let second = sample_ledger_event(
        "batch-event-2",
        "batch-run",
        "batch-action-2",
        crate::execution_ledger::ActionState::WaitingApproval,
    );

    let sequence_ids = database
        .append_ledger_events(&[first, second])
        .expect("batch appends");

    assert_eq!(sequence_ids, vec![1, 2]);
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn append_ledger_events_rolls_back_on_duplicate_terminal_outcome() {
    let path = temp_database_path("ledger-append-batch-rollback");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let first = sample_ledger_event(
        "batch-terminal-1",
        "batch-run",
        "batch-action",
        crate::execution_ledger::ActionState::Succeeded,
    );
    let second = sample_ledger_event(
        "batch-terminal-2",
        "batch-run",
        "batch-action",
        crate::execution_ledger::ActionState::Failed,
    );

    let error = database
        .append_ledger_events(&[first, second])
        .expect_err("duplicate terminal outcome rejects the whole batch");
    assert!(matches!(
        error,
        StorageError::LedgerContract(
            crate::execution_ledger::LedgerContractError::DuplicateTerminalOutcome { .. }
        )
    ));
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn duplicate_event_id_violates_partial_unique_index() {
    let path = temp_database_path("ledger-dup-id");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let first = sample_ledger_event(
        "same-event-id",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Running,
    );
    let second = sample_ledger_event(
        "same-event-id",
        "run-1",
        "action-2",
        crate::execution_ledger::ActionState::Running,
    );
    database
        .append_ledger_event(&first)
        .expect("first insert succeeds");
    let error = database
        .append_ledger_event(&second)
        .expect_err("duplicate event_id must be rejected");
    assert!(matches!(error, StorageError::Sqlite(_)));
    let _ = std::fs::remove_file(&path);
}

/// The single-terminal-outcome guarantee (план 08-1's
/// `assert_single_terminal`) must hold at the real write path, not just
/// as an in-memory helper: a second terminal event for the same
/// `action_id` is rejected even via two independent `append_ledger_event`
/// calls (no batch, no shared transaction between them).
#[test]
fn second_terminal_outcome_for_same_action_is_rejected_at_write_time() {
    let path = temp_database_path("ledger-single-terminal");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let first_terminal = sample_ledger_event(
        "event-terminal-1",
        "run-1",
        "action-guarded",
        crate::execution_ledger::ActionState::Succeeded,
    );
    database
        .append_ledger_event(&first_terminal)
        .expect("first terminal outcome accepted");

    let second_terminal = sample_ledger_event(
        "event-terminal-2",
        "run-1",
        "action-guarded",
        crate::execution_ledger::ActionState::Failed,
    );
    let error = database
        .append_ledger_event(&second_terminal)
        .expect_err("second terminal outcome for the same action must be rejected");
    assert!(matches!(
        error,
        StorageError::LedgerContract(
            crate::execution_ledger::LedgerContractError::DuplicateTerminalOutcome { .. }
        )
    ));
    // Rejected write must not have landed.
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 1);
    let _ = std::fs::remove_file(&path);
}

/// A non-terminal follow-up (e.g. Running -> WaitingApproval) for the
/// same action is unaffected by the guard — only a second *terminal*
/// outcome is rejected.
#[test]
fn non_terminal_follow_up_for_same_action_is_accepted() {
    let path = temp_database_path("ledger-non-terminal-followup");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let running = sample_ledger_event(
        "event-followup-1",
        "run-1",
        "action-followup",
        crate::execution_ledger::ActionState::Running,
    );
    database
        .append_ledger_event(&running)
        .expect("running accepted");
    let waiting = sample_ledger_event(
        "event-followup-2",
        "run-1",
        "action-followup",
        crate::execution_ledger::ActionState::WaitingApproval,
    );
    database
        .append_ledger_event(&waiting)
        .expect("non-terminal follow-up accepted");
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 2);
    let _ = std::fs::remove_file(&path);
}

fn insert_workflow_fixture(database: &LocalDatabase, run_id: &str, node_id: &str) {
    use crate::workflow_store::{NodeState, RunState, WorkflowNodeRecord, WorkflowRunRecord};
    let run = WorkflowRunRecord {
        run_id: run_id.to_string(),
        task_id: "task-ledger".into(),
        template_id: "template-1".into(),
        template_version: 1,
        graph_id: "graph-1".into(),
        graph_version: 1,
        graph_hash: "graph-hash".into(),
        graph_json: "{}".into(),
        inputs_json: "{}".into(),
        policy_json: "{}".into(),
        state: RunState::Running,
        created_at_ms: 1_700_000_000_000,
        updated_at_ms: 1_700_000_000_000,
        terminal_reason: String::new(),
        cancel_requested: false,
        lease_owner: String::new(),
        lease_expires_at_ms: 0,
    };
    let node = WorkflowNodeRecord {
        run_id: run_id.to_string(),
        node_id: node_id.to_string(),
        action_kind: "shell".into(),
        state: NodeState::Running,
        attempts: 0,
        output_json: String::new(),
        error_code: String::new(),
        error_message: String::new(),
        approval_id: String::new(),
        updated_at_ms: 1_700_000_000_000,
    };
    crate::workflow_store::insert_run(database.connection(), &run, &[node])
        .expect("workflow fixture inserts");
}

#[test]
fn node_transition_and_event_commit_together() {
    let path = temp_database_path("ledger-transition-ok");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");
    let event = sample_ledger_event(
        "event-transition-ok",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Succeeded,
    );
    database
        .append_ledger_event_with_node_transition(
            &event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Running,
            crate::execution_ledger::ActionState::Succeeded,
            1_700_000_001_000,
        )
        .expect("legal transition commits both parts");

    let node_state: String = database
        .connection()
        .query_row(
            "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
            [],
            |row| row.get(0),
        )
        .expect("node row reads");
    assert_eq!(node_state, "succeeded");
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 1);
    let _ = std::fs::remove_file(&path);
}

/// План 08-2/08-4: `workflow_run_events.run_sequence` must be linked back
/// to the global ledger row it corresponds to, in the same transaction
/// that wrote both.
#[test]
fn node_transition_links_workflow_run_sequence_to_global_ledger_event() {
    let path = temp_database_path("ledger-transition-linkage");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");
    let event = sample_ledger_event(
        "event-linkage-1",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Succeeded,
    );
    let sequence_id = database
        .append_ledger_event_with_node_transition(
            &event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Running,
            crate::execution_ledger::ActionState::Succeeded,
            1_700_000_001_000,
        )
        .expect("legal transition commits both parts");

    let (run_sequence, ledger_sequence_id, ledger_event_id): (i64, Option<i64>, Option<String>) =
        database
            .connection()
            .query_row(
                "SELECT run_sequence, ledger_sequence_id, ledger_event_id
                       FROM workflow_run_events WHERE run_id = 'run-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("workflow_run_events row reads");
    assert_eq!(run_sequence, 0, "first event in a fresh run starts at 0");
    assert_eq!(ledger_sequence_id, Some(sequence_id));
    assert_eq!(ledger_event_id.as_deref(), Some("event-linkage-1"));
    let _ = std::fs::remove_file(&path);
}

/// A rejected transition must not leave a dangling `workflow_run_events`
/// row either — both inserts share the one transaction that rolls back.
#[test]
fn illegal_transition_rolls_back_workflow_run_events_too() {
    let path = temp_database_path("ledger-transition-illegal-linkage");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");
    let event = sample_ledger_event(
        "event-illegal-linkage-1",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Running,
    );
    let _ = database.append_ledger_event_with_node_transition(
        &event,
        "run-1",
        "node-1",
        crate::execution_ledger::ActionState::Succeeded,
        crate::execution_ledger::ActionState::Running,
        1_700_000_001_000,
    );
    let count: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_events WHERE run_id = 'run-1'",
            [],
            |row| row.get(0),
        )
        .expect("count reads");
    assert_eq!(
        count, 0,
        "rolled-back transition must not leave a linkage row"
    );
    let _ = std::fs::remove_file(&path);
}

/// План 08-2 acceptance: `cancelling` is a real, storable transient state
/// for `workflow_run_nodes.state` after the CHECK-rebuild migration, not
/// just a value the CHECK constraint happens to tolerate. A node must be
/// able to pass through it (`Running -> Cancelling -> Cancelled`) via the
/// same atomic write path as any other transition.
#[test]
fn node_passes_through_cancelling_before_reaching_cancelled() {
    let path = temp_database_path("ledger-cancelling-transition");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");

    let cancelling_event = sample_ledger_event(
        "event-cancelling-1",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Cancelling,
    );
    database
        .append_ledger_event_with_node_transition(
            &cancelling_event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Running,
            crate::execution_ledger::ActionState::Cancelling,
            1_700_000_001_000,
        )
        .expect("Running -> Cancelling is allowed");
    let mid_state: String = database
        .connection()
        .query_row(
            "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
            [],
            |row| row.get(0),
        )
        .expect("node row reads");
    assert_eq!(mid_state, "cancelling");

    let cancelled_event = sample_ledger_event(
        "event-cancelling-2",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Cancelled,
    );
    database
        .append_ledger_event_with_node_transition(
            &cancelled_event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Cancelling,
            crate::execution_ledger::ActionState::Cancelled,
            1_700_000_002_000,
        )
        .expect("Cancelling -> Cancelled is allowed");
    let final_state: String = database
        .connection()
        .query_row(
            "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
            [],
            |row| row.get(0),
        )
        .expect("node row reads");
    assert_eq!(final_state, "cancelled");
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 2);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn illegal_transition_rolls_back_without_writing_event() {
    let path = temp_database_path("ledger-transition-illegal");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");
    let event = sample_ledger_event(
        "event-transition-illegal",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Running,
    );
    let error = database
        .append_ledger_event_with_node_transition(
            &event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Succeeded,
            crate::execution_ledger::ActionState::Running,
            1_700_000_001_000,
        )
        .expect_err("Succeeded -> Running is not an allowed transition");
    assert!(matches!(
        error,
        StorageError::LedgerContract(
            crate::execution_ledger::LedgerContractError::IllegalTransition { .. }
        )
    ));
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 0);
    let node_state: String = database
        .connection()
        .query_row(
            "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
            [],
            |row| row.get(0),
        )
        .expect("node row reads");
    assert_eq!(
        node_state, "running",
        "node state must not change on rollback"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn node_state_mismatch_rolls_back_without_writing_event() {
    let path = temp_database_path("ledger-transition-conflict");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");
    // Узел на самом деле в 'running', но вызывающий думает, что в 'ready'.
    let event = sample_ledger_event(
        "event-transition-conflict",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Running,
    );
    let error = database
        .append_ledger_event_with_node_transition(
            &event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Ready,
            crate::execution_ledger::ActionState::Running,
            1_700_000_001_000,
        )
        .expect_err("stale from-state must be rejected");
    assert!(matches!(
        error,
        StorageError::LedgerNodeTransitionConflict { .. }
    ));
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 0);
    let _ = std::fs::remove_file(&path);
}

/// План 08-4 acceptance: "SQLite failure с полным rollback" — a genuine
/// SQLite-level constraint violation (not an application-level guard)
/// hitting mid-transaction, after the `workflow_run_nodes` UPDATE has
/// already run, must still roll back that UPDATE along with the failed
/// INSERT. `event_id`'s partial UNIQUE index is the real constraint
/// used here — the transition itself is legal, only `event_id` collides
/// with an already-committed row from a prior, unrelated write.
#[test]
fn sqlite_constraint_failure_mid_transaction_rolls_back_the_node_update_too() {
    let path = temp_database_path("ledger-sqlite-failure-rollback");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");

    let already_committed = sample_ledger_event(
        "event-id-collision",
        "run-1",
        "action-unrelated",
        crate::execution_ledger::ActionState::Running,
    );
    database
        .append_ledger_event(&already_committed)
        .expect("first write with this event_id commits");

    // Same event_id, otherwise a perfectly legal Running -> Succeeded
    // transition on a node that really is in 'running'.
    let colliding_event = sample_ledger_event(
        "event-id-collision",
        "run-1",
        "action-1",
        crate::execution_ledger::ActionState::Succeeded,
    );
    let error = database
        .append_ledger_event_with_node_transition(
            &colliding_event,
            "run-1",
            "node-1",
            crate::execution_ledger::ActionState::Running,
            crate::execution_ledger::ActionState::Succeeded,
            1_700_000_001_000,
        )
        .expect_err("duplicate event_id must fail at the SQLite constraint");
    assert!(matches!(error, StorageError::Sqlite(_)));

    // The UPDATE that ran before the failing INSERT must not have
    // survived: the node is still 'running', not 'succeeded'.
    let node_state: String = database
        .connection()
        .query_row(
            "SELECT state FROM workflow_run_nodes WHERE run_id = 'run-1' AND node_id = 'node-1'",
            [],
            |row| row.get(0),
        )
        .expect("node row reads");
    assert_eq!(
        node_state, "running",
        "the node update must roll back together with the failed insert"
    );
    // Only the one earlier, unrelated commit is visible — the failed
    // attempt left no trace of its own events/workflow_run_events rows.
    assert_eq!(database.latest_event_sequence().expect("sequence reads"), 1);
    let workflow_event_count: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM workflow_run_events WHERE run_id = 'run-1'",
            [],
            |row| row.get(0),
        )
        .expect("count reads");
    assert_eq!(workflow_event_count, 0);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn record_core_start_publishes_one_system_scope_event() {
    let path = temp_database_path("ledger-core-start");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let sequence_id = database
        .record_core_start("core-instance-1")
        .expect("core_start publishes");
    assert!(sequence_id > 0);
    let stored = database
        .read_events_after(sequence_id - 1, 1)
        .expect("read back")
        .remove(0);
    assert_eq!(stored.event_type, "ledger.recovery_decision");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn reconcile_startup_flags_open_dispatch_marker_as_unknown_outcome() {
    let path = temp_database_path("ledger-reconcile");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    insert_workflow_fixture(&database, "run-1", "node-1");

    database
        .create_project("project-reconcile", "Reconcile", ".", None)
        .expect("project creates");
    let task = WorkItemRecord {
        id: "task-reconcile".into(),
        project_id: "project-reconcile".into(),
        parent_id: None,
        title: "reconcile me".into(),
        description: String::new(),
        source_ref: None,
        acceptance_criteria: String::new(),
        non_goals: String::new(),
        status: "in_progress".into(),
        priority: 0,
        estimate: None,
        complexity: None,
        attempt_count: 0,
        version: 1,
    };
    database.create_work_item(&task).expect("task creates");
    let run = RunRecord {
        id: "effect-run-1".into(),
        work_item_id: task.id.clone(),
        status: "running".into(),
        policy_snapshot: vec![],
        role_snapshot: vec![],
        skill_snapshot: vec![],
        model_route_snapshot: vec![],
    };
    let checkpoint = RunCheckpointRecord {
        run_id: run.id.clone(),
        checkpoint_id: "checkpoint-1".into(),
        stage: "build".into(),
        node_id: "node-1".into(),
        attempt: 1,
        input_hash: "input-hash".into(),
        state_json: b"{}".to_vec(),
        pending_effects_json: b"[]".to_vec(),
        committed_at: "2026-01-01T00:00:00Z".into(),
    };
    let effect = RunEffectRecord {
        effect_id: "effect-open-1".into(),
        run_id: run.id.clone(),
        node_id: "node-1".into(),
        kind: "bounded_build".into(),
        idempotency_key: "run-1:node-1".into(),
        immutable_intent_hash: "intent-hash".into(),
        state: "prepared".into(),
        started_at: None,
        completed_at: None,
        result_hash: None,
    };
    database
        .prepare_run_effect(&run, &checkpoint, &effect)
        .expect("effect prepares");
    database
        .mark_effect_executing("effect-open-1")
        .expect("effect marker opens (started, not completed)");

    let mut running_event = sample_ledger_event(
        "event-running-1",
        "run-1",
        "action-open-1",
        crate::execution_ledger::ActionState::Running,
    );
    running_event.effect_id = Some("effect-open-1".into());
    database
        .append_ledger_event(&running_event)
        .expect("running action recorded");

    let reconciled = database
        .reconcile_ledger_on_startup()
        .expect("reconciliation runs");
    assert_eq!(reconciled.len(), 1);
    assert_eq!(reconciled[0].0, "action-open-1");
    assert_eq!(
        reconciled[0].1,
        crate::execution_ledger::ActionState::UnknownOutcome
    );

    // Исходная строка не переписана — reconciliation добавила новую.
    let all_action_events: Vec<crate::execution_ledger::ActionState> = database
        .read_events_after(0, 100)
        .expect("events read")
        .into_iter()
        .filter_map(|record| {
            serde_json::from_slice::<crate::execution_ledger::ExecutionEventV1>(&record.payload)
                .ok()
        })
        .filter(|event| event.action_id.as_deref() == Some("action-open-1"))
        .filter_map(|event| event.state_after)
        .collect();
    assert_eq!(
        all_action_events,
        vec![
            crate::execution_ledger::ActionState::Running,
            crate::execution_ledger::ActionState::UnknownOutcome,
        ]
    );

    assert!(
        database
            .reconcile_ledger_on_startup()
            .expect("second reconciliation runs")
            .is_empty(),
        "unknown_outcome is terminal; reconciliation must not repeat"
    );
    let _ = std::fs::remove_file(&path);
}
