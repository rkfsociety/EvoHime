use super::*;

#[test]
fn metadata_store_is_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let input = UpsertInput {
        id: "local.core",
        kind: "local",
        endpoint: None,
        auth_ref: None,
        capabilities_json: "[]",
        version: 1,
        health: "healthy",
        now_ms: 1,
    };
    assert!(upsert(&c, input).unwrap());
    assert!(!upsert(&c, UpsertInput { now_ms: 2, ..input }).unwrap());
    assert_eq!(list(&c).unwrap().len(), 1);
}

#[test]
fn stale_version_cannot_rewind_backend_metadata() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let current = UpsertInput {
        id: "local.core",
        kind: "local",
        endpoint: None,
        auth_ref: None,
        capabilities_json: "[\"new\"]",
        version: 2,
        health: "healthy",
        now_ms: 2,
    };
    assert!(upsert(&c, current).unwrap());
    assert!(!upsert(
        &c,
        UpsertInput {
            capabilities_json: "[\"old\"]",
            version: 1,
            health: "failed",
            now_ms: 3,
            ..current
        }
    )
    .unwrap());
    let rows = list(&c).unwrap();
    assert_eq!(rows[0].version, 2);
    assert_eq!(rows[0].health, "healthy");
}

#[test]
fn listing_is_bounded() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    for index in 0..300 {
        assert!(upsert(
            &c,
            UpsertInput {
                id: &format!("backend-{index:03}"),
                kind: "local",
                endpoint: None,
                auth_ref: None,
                capabilities_json: "[]",
                version: 1,
                health: "healthy",
                now_ms: index,
            }
        )
        .unwrap());
    }
    assert_eq!(list(&c).unwrap().len(), MAX_BACKENDS as usize);
}

#[test]
fn oversized_capabilities_are_rejected_before_storage() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert!(!upsert(
        &c,
        UpsertInput {
            id: "local.core",
            kind: "local",
            endpoint: None,
            auth_ref: None,
            capabilities_json: &"x".repeat(MAX_CAPABILITIES_BYTES + 1),
            version: 1,
            health: "healthy",
            now_ms: 1,
        }
    )
    .unwrap());
    assert!(list(&c).unwrap().is_empty());
}
