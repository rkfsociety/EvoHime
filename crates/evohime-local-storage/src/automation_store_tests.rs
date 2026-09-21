use super::*;
#[test]
fn idempotency_is_durable_and_scoped() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let d = AutomationDefinitionRecord {
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        definition_json: "{}".into(),
        definition_hash: "h".into(),
    };
    insert_definition(&c, &d, 1).unwrap();
    insert_definition(
        &c,
        &AutomationDefinitionRecord {
            definition_json: "{\"replacement\":true}".into(),
            definition_hash: "replacement".into(),
            ..d.clone()
        },
        2,
    )
    .unwrap();
    assert_eq!(get_definition(&c, "d", 1, "o").unwrap().unwrap(), d);
    let r = AutomationRunRecord {
        run_id: "run".into(),
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        idempotency_key: "k".into(),
        payload_hash: "p".into(),
        state: "admitted".into(),
        generation: 1,
        permission_snapshot: "ps".into(),
        approval_snapshot: "as".into(),
    };
    insert_run(&c, &r, 1).unwrap();
    assert_eq!(
        find_run_by_idempotency(&c, "o", "d", 1, "k")
            .unwrap()
            .unwrap(),
        r
    );
    assert!(find_run_by_idempotency(&c, "other", "d", 1, "k")
        .unwrap()
        .is_none());
    assert!(matches!(
        admit_run(&c, &r, 2).unwrap(),
        AdmitRunResult::Existing(_)
    ));
    let mut conflict = r.clone();
    conflict.payload_hash = "different".into();
    assert!(matches!(
        admit_run(&c, &conflict, 2).unwrap(),
        AdmitRunResult::IdempotencyConflict { .. }
    ));
    let mut c = c;
    assert!(transition_run(
        &mut c,
        RunTransition {
            run_id: "run",
            from_state: "admitted",
            to_state: "queued",
            generation: 1,
            event_type: "queued",
            payload_json: "{}",
            now_ms: 2,
        },
    )
    .unwrap());
    assert!(!transition_run(
        &mut c,
        RunTransition {
            run_id: "run",
            from_state: "queued",
            to_state: "running",
            generation: 0,
            event_type: "running",
            payload_json: "{}",
            now_ms: 3,
        },
    )
    .unwrap());
    assert!(acquire_lease(&c, "run", "core-a", 1, 10, 30).unwrap());
    assert!(!acquire_lease(&c, "run", "core-b", 2, 20, 30).unwrap());
    assert!(acquire_lease(&c, "run", "core-b", 2, 40, 30).unwrap());
}

#[test]
fn schedule_cursor_is_durable_and_compare_and_swap_fenced() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let definition = AutomationDefinitionRecord {
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        definition_json: "{}".into(),
        definition_hash: "h".into(),
    };
    insert_definition(&c, &definition, 1).unwrap();
    let schedule = AutomationScheduleRecord {
        schedule_id: "s".into(),
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        hour: 12,
        minute: 0,
        timezone_minutes: 0,
        missed_grace_ms: 60_000,
        enabled: true,
        last_slot: None,
        preset_id: None,
        preset_revision: None,
        preset_content_hash: None,
        workspace_path: String::new(),
    };
    upsert_schedule(&c, &schedule, 1).unwrap();
    assert!(advance_schedule_slot(&c, "s", None, "slot-1", 2).unwrap());
    assert!(!advance_schedule_slot(&c, "s", None, "slot-2", 3).unwrap());
    assert!(!advance_schedule_slot(&c, "s", Some("old"), "slot-2", 3).unwrap());
    assert!(advance_schedule_slot(&c, "s", Some("slot-1"), "slot-2", 4).unwrap());
    assert_eq!(
        get_schedule(&c, "s").unwrap().unwrap().last_slot.as_deref(),
        Some("slot-2")
    );
    assert!(!upsert_schedule(
        &c,
        &AutomationScheduleRecord {
            revision: 0,
            hour: 1,
            ..schedule.clone()
        },
        5
    )
    .unwrap());
    assert_eq!(get_schedule(&c, "s").unwrap().unwrap().hour, 12);
    assert!(!upsert_schedule(
        &c,
        &AutomationScheduleRecord {
            hour: 1,
            ..schedule.clone()
        },
        6
    )
    .unwrap());
    assert_eq!(get_schedule(&c, "s").unwrap().unwrap().hour, 12);
}

#[test]
fn schedule_keeps_immutable_preset_snapshot_reference() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let definition = AutomationDefinitionRecord {
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        definition_json: "{}".into(),
        definition_hash: "h".into(),
    };
    insert_definition(&c, &definition, 1).unwrap();
    let schedule = AutomationScheduleRecord {
        schedule_id: "s".into(),
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        hour: 1,
        minute: 2,
        timezone_minutes: 0,
        missed_grace_ms: 1,
        enabled: true,
        last_slot: None,
        preset_id: Some("p".into()),
        preset_revision: Some(3),
        preset_content_hash: Some("hash".into()),
        workspace_path: "C:/workspace".into(),
    };
    upsert_schedule(&c, &schedule, 1).unwrap();
    let loaded = get_schedule(&c, "s").unwrap().unwrap();
    assert_eq!(loaded.preset_id.as_deref(), Some("p"));
    assert_eq!(loaded.preset_revision, Some(3));
    assert_eq!(loaded.preset_content_hash.as_deref(), Some("hash"));
    assert_eq!(loaded.workspace_path, "C:/workspace");
}

#[test]
fn archive_restore_is_atomic_checksum_verified_and_retention_bounded() {
    let mut c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let record = AutomationRunRecord {
        run_id: "run-archive".into(),
        definition_id: "d".into(),
        revision: 1,
        owner_scope: "o".into(),
        idempotency_key: "k".into(),
        payload_hash: "p".into(),
        state: "completed".into(),
        generation: 1,
        permission_snapshot: "ps".into(),
        approval_snapshot: "as".into(),
    };
    insert_run(&c, &record, 10).unwrap();
    save_snapshot(
        &c,
        SnapshotInsert {
            snapshot_id: "snapshot-1",
            run_id: "run-archive",
            definition_revision: 1,
            generation: 1,
            event_sequence: 0,
            snapshot_json: "{}",
            checksum_sha256: "checksum",
            now_ms: 11,
        },
    )
    .unwrap();
    save_snapshot(
        &c,
        SnapshotInsert {
            snapshot_json: "{\"replacement\":true}",
            checksum_sha256: "replacement",
            now_ms: 12,
            ..SnapshotInsert {
                snapshot_id: "snapshot-1",
                run_id: "run-archive",
                definition_revision: 1,
                generation: 1,
                event_sequence: 0,
                snapshot_json: "{}",
                checksum_sha256: "checksum",
                now_ms: 11,
            }
        },
    )
    .unwrap();
    assert_eq!(
        c.query_row(
            "SELECT snapshot_json FROM automation_snapshots WHERE snapshot_id='snapshot-1'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "{}"
    );
    assert!(archive_run(&mut c, "archive-1", "run-archive", 20, 100).unwrap());
    assert!(get_run(&c, "run-archive").unwrap().is_none());
    assert!(restore_archive(&mut c, "archive-1", 30).unwrap());
    assert_eq!(get_run(&c, "run-archive").unwrap().unwrap(), record);
    assert_eq!(
        c.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM automation_snapshots WHERE run_id='run-archive'",
            [],
            |row| row.get(0)
        )
        .unwrap(),
        1
    );
    assert!(!restore_archive(&mut c, "archive-1", 31).unwrap());
    assert_eq!(sweep_expired_archives(&c, 100).unwrap(), 1);
}
