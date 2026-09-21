use super::*;

#[test]
fn protocol_revision_is_immutable_and_idempotent() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert!(save_protocol(&c, "coding", 1, "h", br#"{}"#, 1).unwrap());
    assert!(!save_protocol(&c, "coding", 1, "h", br#"{}"#, 2).unwrap());
    assert_eq!(load_all_json(&c).unwrap().len(), 1);
}

#[test]
fn protocol_listing_is_bounded() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    for index in 0..300 {
        assert!(
            save_protocol(&c, &format!("protocol-{index:03}"), 1, "hash", b"{}", index).unwrap()
        );
    }
    assert_eq!(load_all_json(&c).unwrap().len(), MAX_PROTOCOLS as usize);
}

#[test]
fn oversized_protocol_is_rejected_before_storage() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert!(!save_protocol(
        &c,
        "oversized",
        1,
        "hash",
        &vec![b'x'; MAX_PROTOCOL_BYTES + 1],
        1,
    )
    .unwrap());
    assert!(load_all_json(&c).unwrap().is_empty());
}

#[test]
fn oversized_session_snapshot_is_rejected_before_storage() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert!(!save_session(
        &c,
        SaveSessionInput {
            id: "session",
            protocol_id: "protocol",
            protocol_version: 1,
            hash: "hash",
            snapshot: &vec![b'x'; MAX_SESSION_SNAPSHOT_BYTES + 1],
            status: "running",
            phase: "execute",
            version: 1,
            now_ms: 1,
        }
    )
    .unwrap());
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM team_sop_sessions", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn existing_protocol_history_without_current_row_is_not_rewritten() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    c.execute(
        "INSERT INTO team_sop_protocol_revisions
         (protocol_id, version, content_hash, protocol_json, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "coding",
            3_i64,
            "original",
            br#"{"source":"original"}"#,
            10_i64
        ],
    )
    .unwrap();
    assert!(!save_protocol(
        &c,
        "coding",
        3,
        "replacement",
        br#"{"source":"replacement"}"#,
        20,
    )
    .unwrap());
    let stored: (String, Vec<u8>) = c
        .query_row(
            "SELECT content_hash, protocol_json FROM team_sop_protocol_revisions
             WHERE protocol_id=?1 AND version=?2",
            params!["coding", 3_i64],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        ("original".into(), br#"{"source":"original"}"#.to_vec())
    );
}

#[test]
fn session_version_fence_rejects_stale_snapshots() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let input = SaveSessionInput {
        id: "session",
        protocol_id: "coding",
        protocol_version: 1,
        hash: "h",
        snapshot: br#"{}"#,
        status: "running",
        phase: "execute",
        version: 2,
        now_ms: 2,
    };
    assert!(save_session(&c, input).unwrap());
    assert!(!save_session(
        &c,
        SaveSessionInput {
            status: "stale",
            phase: "old",
            version: 1,
            now_ms: 3,
            ..input
        }
    )
    .unwrap());
    let stored: String = c
        .query_row(
            "SELECT status FROM team_sop_sessions WHERE id='session'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored, "running");
}

#[test]
fn duplicate_session_version_cannot_replace_state() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    let input = SaveSessionInput {
        id: "session",
        protocol_id: "coding",
        protocol_version: 1,
        hash: "h",
        snapshot: br#"{"source":"first"}"#,
        status: "running",
        phase: "execute",
        version: 2,
        now_ms: 2,
    };
    assert!(save_session(&c, input).unwrap());
    assert!(!save_session(
        &c,
        SaveSessionInput {
            snapshot: br#"{"source":"replacement"}"#,
            status: "failed",
            phase: "wrong",
            now_ms: 3,
            ..input
        }
    )
    .unwrap());
    let stored: (String, String, Vec<u8>, i64) = c
        .query_row(
            "SELECT status, current_phase, snapshot_json, updated_at_ms
             FROM team_sop_sessions WHERE id=?1",
            ["session"],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        stored,
        (
            "running".into(),
            "execute".into(),
            br#"{"source":"first"}"#.to_vec(),
            2
        )
    );
}
