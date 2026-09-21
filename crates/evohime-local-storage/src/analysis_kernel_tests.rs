use super::*;
use crate::LocalDatabase;

fn session() -> AnalysisKernelSessionV1 {
    AnalysisKernelSessionV1 {
        schema_version: 1,
        id: "kernel-1".into(),
        task_id: "task-1".into(),
        workspace_id: "workspace-1".into(),
        runtime_version: "trusted-local-1".into(),
        package_manifest_hash: "a".repeat(64),
        policy_hash: "b".repeat(64),
        status: KernelStatus::Created,
        revision: 0,
        limits: KernelLimitsV1::default(),
        created_at_ms: 1,
        updated_at_ms: 1,
    }
}

#[test]
fn canonical_hash_is_stable_and_authority_fields_are_validated() {
    let value = session();
    assert_eq!(
        value.canonical_json().unwrap(),
        value.canonical_json().unwrap()
    );
    assert_eq!(value.content_hash().unwrap().len(), 64);
    let mut invalid = value;
    invalid.schema_version = 2;
    assert!(matches!(
        invalid.validate(),
        Err(AnalysisKernelError::UnsupportedVersion(2))
    ));
}

#[test]
fn object_rejects_secret_and_ephemeral_process_memory() {
    let object = KernelObjectRefV1 {
        id: "object-1".into(),
        kernel_id: "kernel-1".into(),
        logical_name: "rows".into(),
        type_hint: "json".into(),
        size: 10,
        sensitivity: KernelSensitivity::Secret,
        persistence: KernelObjectPersistence::Ephemeral,
        content_hash: None,
        artifact_locator: None,
        provenance: "core:test".into(),
        created_at_ms: 1,
        invalidated_at_ms: None,
    };
    assert!(matches!(
        object.validate(),
        Err(AnalysisKernelError::SecretObject)
    ));
}

#[test]
fn store_round_trip_and_stale_update_are_typed() {
    let path = std::env::temp_dir().join(format!(
        "evohime-analysis-kernel-{}.db",
        uuid::Uuid::new_v4()
    ));
    let db = LocalDatabase::open(&path).unwrap();
    let store = AnalysisKernelStore::new(db.connection());
    let value = session();
    store.create_session(&value).unwrap();
    assert_eq!(store.get_session("kernel-1").unwrap().unwrap(), value);
    assert_eq!(
        store
            .set_status("kernel-1", 0, KernelStatus::Running, 2)
            .unwrap(),
        1
    );
    assert!(matches!(
        store.set_status("kernel-1", 0, KernelStatus::Stopped, 3),
        Err(StorageError::AnalysisKernel(
            AnalysisKernelError::VersionConflict { .. }
        ))
    ));
    store
        .put_idempotency("kernel-1", "idem", "json_parse", br#"{"ok":true}"#, 3)
        .unwrap();
    assert_eq!(
        store
            .get_idempotency("kernel-1", "idem", "json_parse")
            .unwrap()
            .unwrap(),
        br#"{"ok":true}"#
    );
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn running_session_recovery_is_bounded() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = AnalysisKernelStore::new(&connection);
    for index in 0..300 {
        let mut value = session();
        value.id = format!("kernel-{index:03}");
        value.task_id = format!("task-{index:03}");
        value.status = KernelStatus::Running;
        store.create_session(&value).unwrap();
    }
    assert_eq!(
        store.list_running_sessions().unwrap().len(),
        ANALYSIS_KERNEL_MAX_RUNNING_SESSIONS
    );
}

#[test]
fn object_listing_is_bounded() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = AnalysisKernelStore::new(&connection);
    store.create_session(&session()).unwrap();
    for index in 0..(ANALYSIS_KERNEL_MAX_OBJECTS + 1) {
        store
            .put_object(&KernelObjectRefV1 {
                id: format!("object-{index:04}"),
                kernel_id: "kernel-1".into(),
                logical_name: format!("object-{index:04}"),
                type_hint: "json".into(),
                size: 0,
                sensitivity: KernelSensitivity::Public,
                persistence: KernelObjectPersistence::Ephemeral,
                content_hash: None,
                artifact_locator: None,
                provenance: "core:test".into(),
                created_at_ms: index as i64 + 1,
                invalidated_at_ms: None,
            })
            .unwrap();
    }
    assert_eq!(
        store.list_objects("kernel-1").unwrap().len(),
        ANALYSIS_KERNEL_MAX_OBJECTS
    );
}

#[test]
fn duplicate_object_replay_keeps_original_metadata() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let store = AnalysisKernelStore::new(&connection);
    store.create_session(&session()).unwrap();
    let object = KernelObjectRefV1 {
        id: "object-1".into(),
        kernel_id: "kernel-1".into(),
        logical_name: "rows".into(),
        type_hint: "json".into(),
        size: 10,
        sensitivity: KernelSensitivity::Public,
        persistence: KernelObjectPersistence::Checkpointed,
        content_hash: Some("a".repeat(64)),
        artifact_locator: Some("artifact://first".into()),
        provenance: "core:test".into(),
        created_at_ms: 1,
        invalidated_at_ms: None,
    };
    store.put_object(&object).unwrap();
    let replacement = KernelObjectRefV1 {
        artifact_locator: Some("artifact://replacement".into()),
        ..object.clone()
    };
    store.put_object(&replacement).unwrap();
    assert_eq!(store.list_objects("kernel-1").unwrap(), vec![object]);
}
