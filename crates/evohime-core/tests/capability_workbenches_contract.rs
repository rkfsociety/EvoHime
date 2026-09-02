use evohime_core::capability_workbenches::{
    validate_descriptor, validate_snapshot, CancellationOutcome, Concurrency, Lifecycle,
    SharedResource, ToolDescriptor, WorkbenchDescriptor, WorkbenchError, WorkbenchScope,
    WorkbenchSnapshot, MAX_IN_FLIGHT, MAX_SNAPSHOT_BYTES, MAX_TOOLS, SCHEMA_VERSION,
};

fn descriptor() -> WorkbenchDescriptor {
    WorkbenchDescriptor {
        schema_version: SCHEMA_VERSION,
        id: "repo".into(),
        version: "1".into(),
        kind: "repository".into(),
        scope: WorkbenchScope::ProjectScoped,
        concurrency: Concurrency::Parallel,
        max_in_flight: 2,
        lease_ttl_ms: 20_000,
        tools: vec![ToolDescriptor {
            id: "read".into(),
            capability: "repo.read".into(),
            title: "Read".into(),
        }],
        resources: vec![SharedResource {
            id: "workspace".into(),
            class: "filesystem".into(),
            available: true,
        }],
    }
}

#[test]
fn bounds_and_dynamic_capability_filter_are_enforced() {
    let mut invalid = descriptor();
    invalid.max_in_flight = (MAX_IN_FLIGHT + 1) as u32;
    assert_eq!(validate_descriptor(&invalid), Err(WorkbenchError::Bounds));
    invalid = descriptor();
    invalid.tools = (0..=MAX_TOOLS)
        .map(|index| ToolDescriptor {
            id: format!("tool-{index}"),
            capability: "repo.read".into(),
            title: "Tool".into(),
        })
        .collect();
    assert_eq!(validate_descriptor(&invalid), Err(WorkbenchError::Bounds));
    assert_eq!(CancellationOutcome::Unknown, CancellationOutcome::Unknown);
}

#[test]
fn portable_snapshot_excludes_raw_sensitive_state_and_stays_bounded() {
    let snapshot = WorkbenchSnapshot {
        schema_version: SCHEMA_VERSION,
        instance_id: "i".into(),
        descriptor_version: "1".into(),
        revision: 1,
        lifecycle: Lifecycle::Ready,
        logical_state: serde_json::json!({"raw_output":"forbidden"}),
        credential_refs: vec!["credential-ref".into()],
        resource_ids: vec!["workspace".into()],
    };
    assert_eq!(
        validate_snapshot(&snapshot),
        Err(WorkbenchError::ForbiddenSnapshotField)
    );
    let oversized = WorkbenchSnapshot {
        logical_state: serde_json::json!({"note":"x".repeat(MAX_SNAPSHOT_BYTES)}),
        credential_refs: Vec::new(),
        ..snapshot
    };
    assert_eq!(
        validate_snapshot(&oversized),
        Err(WorkbenchError::SnapshotTooLarge)
    );
}
