use super::*;

#[test]
fn inbound_is_deduplicated_and_clear_is_cascading() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
    put_bridge(&c, "b", bridge, 1).unwrap();
    assert!(get_bridge(&c, "b").unwrap().is_some());
    let binding = br#"{"conversation_id":"c","principal_id":"p"}"#;
    assert!(put_binding(&c, binding, "bind", "b", "thread", 1).unwrap());
    assert!(get_binding(&c, "bind").unwrap().is_some());
    assert!(put_inbound(&c, "m", "bind", b"{}", 1).unwrap());
    assert!(!put_inbound(&c, "m", "bind", b"{}", 1).unwrap());
    assert!(claim_idempotency(&c, "other-bridge-key", "create").unwrap());
    clear_bridge(&c, "b").unwrap();
    assert!(list_inbound(&c).unwrap().is_empty());
    assert!(!claim_idempotency(&c, "other-bridge-key", "create").unwrap());
}

#[test]
fn rejects_invalid_bridge_and_binding_json() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();

    assert!(put_bridge(&c, "b", b"{invalid", 1).is_err());
    assert!(put_binding(&c, b"{invalid", "bind", "b", "thread", 1).is_err());
    assert!(get_bridge(&c, "b").unwrap().is_none());
    assert!(get_binding(&c, "bind").unwrap().is_none());
}

#[test]
fn rejects_structurally_empty_bridge_and_binding_json() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();

    assert!(put_bridge(&c, "b", br#"{}"#, 1).is_err());
    assert!(put_binding(&c, br#"{}"#, "bind", "b", "thread", 1).is_err());
}

#[test]
fn rejects_orphan_and_cross_identity_bindings() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
    put_bridge(&c, "b", bridge, 1).unwrap();

    let mismatch = br#"{"conversation_id":"other","principal_id":"p"}"#;
    assert!(put_binding(&c, mismatch, "bind-mismatch", "b", "thread", 1).is_err());
    let valid = br#"{"conversation_id":"c","principal_id":"p"}"#;
    assert!(put_binding(&c, valid, "bind-orphan", "missing", "thread", 1).is_err());
}

#[test]
fn rejects_stale_and_unrepresentable_bridge_revisions() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
    put_bridge(&c, "b", bridge, 2).unwrap();

    assert!(put_bridge(&c, "b", bridge, 2).is_err());
    assert!(put_bridge(&c, "b", bridge, 1).is_err());
    assert!(put_bridge(&c, "other", bridge, u64::MAX).is_err());
    let stored: serde_json::Value =
        serde_json::from_slice(&get_bridge(&c, "b").unwrap().unwrap()).unwrap();
    assert_eq!(stored["revision"], 2);
}

#[test]
fn rejects_negative_persisted_revision() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let bridge = br#"{"provider":"telegram","conversation_id":"c","principal_id":"p","pairing_hash":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","state":"paired"}"#;
    put_bridge(&c, "b", bridge, 1).unwrap();
    c.execute(
        "UPDATE conversation_bridges SET revision=-1 WHERE bridge_id='b'",
        [],
    )
    .unwrap();

    assert!(get_bridge(&c, "b").is_err());
    assert!(bridge_revision(&c, "b").is_err());
}

#[test]
fn oversized_bridge_json_is_rejected_before_parsing() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert!(put_bridge(&c, "bridge", &vec![b'x'; MAX_BRIDGE_JSON_BYTES + 1], 1).is_err());
    assert!(get_bridge(&c, "bridge").unwrap().is_none());
}
