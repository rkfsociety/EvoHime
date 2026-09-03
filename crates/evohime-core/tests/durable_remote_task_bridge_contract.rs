use evohime_core::durable_remote_task_bridge as bridge;

fn toolset() -> bridge::RemoteTaskToolset {
    bridge::RemoteTaskToolset {
        schema_version: 1,
        id: "provider-docs".into(),
        version: 1,
        provider_kind: bridge::RemoteProviderKind::IntegrationProvider,
        provider_ref: "provider.docs".into(),
        operation_names: vec!["search".into()],
        content_hash: "toolset-hash".into(),
    }
}

#[test]
fn submit_hashes_payload_and_keeps_result_as_artifact_ref() {
    let policy = bridge::default_policy();
    let record = bridge::build_record(
        "remote-1".into(),
        &toolset(),
        "search".into(),
        b"secret-like input",
        "run-1".into(),
        1,
        &policy,
    )
    .unwrap();
    assert_eq!(record.request_hash.len(), 64);
    assert!(record.result_artifact_ref.is_none());
    assert!(bridge::validate_record(&record, &toolset(), &policy).is_ok());
}

#[test]
fn unknown_schema_and_unregistered_operation_fail_closed() {
    let policy = bridge::default_policy();
    let mut invalid = toolset();
    invalid.schema_version = 2;
    assert_eq!(
        bridge::validate_toolset(&invalid, &policy),
        Err(bridge::RemoteTaskError::UnsupportedVersion(2))
    );
    assert_eq!(
        bridge::build_record(
            "remote-1".into(),
            &toolset(),
            "delete".into(),
            b"{}",
            "run-1".into(),
            1,
            &policy
        ),
        Err(bridge::RemoteTaskError::OperationDenied)
    );
}
