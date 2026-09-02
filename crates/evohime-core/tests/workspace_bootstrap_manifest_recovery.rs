use evohime_core::workspace_bootstrap_manifest::{
    run_bounded, with_content_hash, BootstrapManifestError, BootstrapStep, BootstrapStepKind,
    NetworkRequirement, StepIdempotency, WorkspaceBootstrapManifest,
};

fn manifest(network: NetworkRequirement, kind: BootstrapStepKind) -> WorkspaceBootstrapManifest {
    with_content_hash(WorkspaceBootstrapManifest {
        schema_version: 1,
        id: "integration".into(),
        workspace_id: "workspace".into(),
        revision: 1,
        steps: vec![BootstrapStep {
            id: "step".into(),
            kind,
            logical_executable: Some("cargo".into()),
            args: vec!["--version".into()],
            workspace_relative_path: None,
            network,
            idempotency: StepIdempotency::Idempotent,
            timeout_ms: 5_000,
        }],
        cache_inputs: vec![],
        content_hash: String::new(),
    })
    .unwrap()
}

#[tokio::test]
async fn network_requirement_is_denied_before_process_dispatch() {
    let value = run_bounded(
        std::path::Path::new("."),
        &manifest(
            NetworkRequirement::GeneralInternet,
            BootstrapStepKind::RunCommand,
        ),
    )
    .await;
    assert_eq!(value, Err(BootstrapManifestError::NetworkDenied));
}

#[tokio::test]
async fn unsupported_file_effect_is_not_silently_executed() {
    let value = run_bounded(
        std::path::Path::new("."),
        &manifest(
            NetworkRequirement::None,
            BootstrapStepKind::CopyTemplateIfMissing,
        ),
    )
    .await;
    assert_eq!(value, Err(BootstrapManifestError::UnsupportedEffect));
}
