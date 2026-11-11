use super::*;

fn schema() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    connection
}

fn profile_input<'a>(id: &'a str, revision: u64, now_ms: i64) -> A2aProfileInput<'a> {
    A2aProfileInput {
        profile_id: id,
        revision,
        display_name: "Remote Agent",
        protocol_kind: "a2a",
        endpoint_origin: "https://agents.example.com",
        allow_loopback: false,
        allowed_ports_json: "[]",
        auth_profile_ref: Some("auth.remote"),
        skill_allowlist_json: "[\"code.review\"]",
        capability_allowlist_json: "[\"agent.execute\"]",
        privacy_ceiling: "sensitive",
        concurrency_limit: 2,
        enabled: true,
        content_hash: "hash",
        now_ms,
    }
}

#[test]
fn newer_profile_revision_wins_and_stale_revision_is_ignored() {
    let connection = schema();
    assert!(upsert_profile(&connection, profile_input("remote.agent", 2, 20)).unwrap());
    assert!(!upsert_profile(&connection, profile_input("remote.agent", 2, 10)).unwrap());
    let current: (i64, String) = connection
        .query_row(
            "SELECT revision,content_hash FROM a2a_remote_profiles WHERE profile_id=?1",
            ["remote.agent"],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(current, (2, "hash".into()));
}

#[test]
fn disabling_a_profile_preserves_history_and_keeps_revisions() {
    let connection = schema();
    upsert_profile(&connection, profile_input("remote.agent", 1, 1)).unwrap();
    upsert_profile(&connection, profile_input("remote.agent", 2, 2)).unwrap();
    assert_eq!(disable_profile(&connection, "remote.agent", 3).unwrap(), 2);
    let enabled: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM a2a_remote_profiles WHERE profile_id=?1 AND enabled=1",
            ["remote.agent"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(enabled, 0);
    let revisions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM a2a_remote_profiles WHERE profile_id=?1",
            ["remote.agent"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(revisions, 2);
}

#[test]
fn card_snapshots_are_immutable_and_store_only_hashes() {
    let connection = schema();
    let input = A2aCardSnapshotInput {
        profile_id: "remote.agent",
        profile_revision: 1,
        normalized_hash: "n1",
        raw_hash: "r1",
        origin: "https://agents.example.com",
        protocol_version: 1,
        fetched_at_ms: 1,
        expires_at_ms: 100,
        supports_streaming: true,
        skills_json: "[]",
        schema_hashes_json: "[]",
        auth_schemes_json: "[\"bearer\"]",
        now_ms: 1,
    };
    assert!(insert_card_snapshot(&connection, input).unwrap());
    assert!(!insert_card_snapshot(
        &connection,
        A2aCardSnapshotInput {
            raw_hash: "r2",
            now_ms: 2,
            ..input
        }
    )
    .unwrap());
    let columns: Vec<String> = connection
        .prepare("SELECT name FROM pragma_table_info('a2a_card_snapshots') ORDER BY cid")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(!columns.iter().any(|column| column.contains("body")));
    let raw: String = connection
        .query_row(
            "SELECT raw_hash FROM a2a_card_snapshots WHERE normalized_hash=?1",
            ["n1"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(raw, "r1");
}

#[test]
fn task_binding_revision_cas_rejects_stale_write() {
    let connection = schema();
    let input = A2aTaskBindingInput {
        binding_id: "binding-1",
        delegation_id: "d1",
        remote_task_id: Some("remote-1"),
        state: "running",
        reconciliation: "not_required",
        attempt: 1,
        idempotency_key: "idem-1",
        request_hash: "req",
        capsule_hash: "cap",
        policy_hash: "pol",
        revision: 3,
        created_at_ms: 1,
        updated_at_ms: 3,
        content_hash: "h3",
    };
    assert!(upsert_task_binding(&connection, input).unwrap());
    assert!(!upsert_task_binding(
        &connection,
        A2aTaskBindingInput {
            revision: 2,
            content_hash: "h2",
            updated_at_ms: 4,
            ..input
        }
    )
    .unwrap());
    assert_eq!(
        binding_revision(&connection, "binding-1").unwrap(),
        Some((3, "h3".into()))
    );
}

#[test]
fn nonterminal_bindings_exclude_terminal_states_in_creation_order() {
    let connection = schema();
    for (id, state, created, delegation) in [
        ("b-done", "completed", 1, "d-done"),
        ("b-run", "running", 2, "d-run"),
        ("b-unknown", "unknown_outcome", 3, "d-unknown"),
        ("b-failed", "failed", 4, "d-failed"),
    ] {
        upsert_task_binding(
            &connection,
            A2aTaskBindingInput {
                binding_id: id,
                delegation_id: delegation,
                remote_task_id: None,
                state,
                reconciliation: "not_required",
                attempt: 0,
                idempotency_key: "idem",
                request_hash: "req",
                capsule_hash: "cap",
                policy_hash: "pol",
                revision: 1,
                created_at_ms: created,
                updated_at_ms: created,
                content_hash: "h",
            },
        )
        .unwrap();
    }
    let pending = list_nonterminal_bindings(&connection, 10).unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|binding| binding.binding_id.as_str())
            .collect::<Vec<_>>(),
        vec!["b-run", "b-unknown"]
    );
}
