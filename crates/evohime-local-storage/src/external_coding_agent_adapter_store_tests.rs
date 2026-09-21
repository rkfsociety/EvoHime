use super::*;

#[test]
fn event_delivery_is_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let input = RecordEventInput {
        conversation_id: "c",
        run_id: "r",
        state: "running",
        outcome: "",
        correlation_id: "x",
        idempotency_key: "i",
        now_ms: 1,
    };
    assert!(record_event(&c, input).unwrap());
    assert!(!record_event(&c, RecordEventInput { now_ms: 2, ..input }).unwrap());
}

#[test]
fn stale_preset_revision_cannot_rewind_current_preset() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let current = UpsertPresetInput {
        id: "preset-1",
        revision: 2,
        protocol: "stdio",
        executable_ref: "new-agent",
        capabilities_json: "[]",
        slots_json: "[]",
        control_level: "supervised",
        enabled: true,
        content_hash: "new",
        protocol_kind: "evohime_v1",
        auth_mode: "declared_credential_slots",
        backend_class: "external_agent_backend",
        executable_identity_json: "{}",
        now_ms: 2,
    };
    assert!(upsert_preset(&c, current).unwrap());
    assert!(!upsert_preset(
        &c,
        UpsertPresetInput {
            revision: 1,
            executable_ref: "old-agent",
            content_hash: "old",
            now_ms: 3,
            ..current
        }
    )
    .unwrap());
    let row: (i64, String, String) = c
        .query_row(
            "SELECT revision,executable_ref,content_hash FROM external_agent_presets WHERE id=?1",
            ["preset-1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, (2, "new-agent".into(), "new".into()));
}

#[test]
fn duplicate_preset_revision_cannot_replace_current_preset() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let current = UpsertPresetInput {
        id: "preset-1",
        revision: 2,
        protocol: "stdio",
        executable_ref: "new-agent",
        capabilities_json: "[]",
        slots_json: "[]",
        control_level: "supervised",
        enabled: true,
        content_hash: "new",
        protocol_kind: "evohime_v1",
        auth_mode: "declared_credential_slots",
        backend_class: "external_agent_backend",
        executable_identity_json: "{}",
        now_ms: 2,
    };
    assert!(upsert_preset(&c, current).unwrap());
    assert!(!upsert_preset(
        &c,
        UpsertPresetInput {
            executable_ref: "replacement-agent",
            content_hash: "replacement",
            now_ms: 3,
            ..current
        }
    )
    .unwrap());
    let row: (i64, String, String) = c
        .query_row(
            "SELECT revision,executable_ref,content_hash FROM external_agent_presets WHERE id=?1",
            ["preset-1"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(row, (2, "new-agent".into(), "new".into()));
}
