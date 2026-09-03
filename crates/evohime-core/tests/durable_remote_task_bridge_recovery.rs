use evohime_core::durable_remote_task_bridge as bridge;

fn toolset() -> bridge::RemoteTaskToolset {
    bridge::RemoteTaskToolset {
        schema_version: 1,
        id: "mcp-docs".into(),
        version: 1,
        provider_kind: bridge::RemoteProviderKind::Mcp,
        provider_ref: "mcp.docs".into(),
        operation_names: vec!["search".into()],
        content_hash: "hash".into(),
    }
}

#[test]
fn poll_lease_and_cancel_preserve_unknown_side_effect_boundary() {
    let policy = bridge::default_policy();
    let mut record = bridge::build_record(
        "remote-1".into(),
        &toolset(),
        "search".into(),
        b"{}",
        "run-1".into(),
        1,
        &policy,
    )
    .unwrap();
    bridge::lease_for_poll(&mut record, "core", 2, &policy).unwrap();
    assert_eq!(record.transport_status, "polling");
    let version = record.version;
    bridge::cancel(&mut record, version, 3).unwrap();
    assert_eq!(record.status, bridge::RemoteTaskStatus::CancelRequested);
    assert_eq!(record.transport_status, "cancel_requested");
    assert!(bridge::lease_for_poll(&mut record, "core", 4, &policy).is_err());
}
