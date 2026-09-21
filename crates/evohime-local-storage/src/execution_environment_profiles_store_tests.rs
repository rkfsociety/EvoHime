use super::*;
#[test]
fn revisions_are_monotonic_and_activation_is_atomic() {
    let db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    assert!(save_profile_revision(
        &db,
        SaveProfileRevisionInput {
            id: "p",
            revision: 1,
            scope: "application:a",
            state: "ready",
            hash: "h",
            json: br#"{}"#,
            actor: "core",
            now_ms: 1
        }
    )
    .unwrap());
    assert!(!save_profile_revision(
        &db,
        SaveProfileRevisionInput {
            id: "p",
            revision: 1,
            scope: "application:a",
            state: "ready",
            hash: "h",
            json: br#"{}"#,
            actor: "core",
            now_ms: 1
        }
    )
    .unwrap());
    assert!(save_activation(
        &db,
        SaveActivationInput {
            profile_id: "p",
            revision: 1,
            scope: "application:a",
            status: "ready",
            snapshot_hash: "snap",
            activation_json: br#"{}"#,
            snapshot_json: br#"{}"#,
            now_ms: 2
        }
    )
    .unwrap());
    assert_eq!(
        load_current(&db, "application:a").unwrap(),
        Some(br#"{}"#.to_vec())
    );
}

#[test]
fn existing_history_without_current_profile_is_not_rewritten_or_rejected() {
    let db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    db.execute(
        "INSERT INTO execution_environment_profile_revisions
             (profile_id, revision, content_hash, profile_json, actor, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "p",
            3_i64,
            "original",
            br#"{"original":true}"#,
            "old-core",
            10_i64
        ],
    )
    .unwrap();

    assert!(!save_profile_revision(
        &db,
        SaveProfileRevisionInput {
            id: "p",
            revision: 3,
            scope: "application:a",
            state: "ready",
            hash: "replacement",
            json: br#"{"original":false}"#,
            actor: "new-core",
            now_ms: 20,
        },
    )
    .unwrap());
    assert_eq!(
        db.query_row(
            "SELECT content_hash, profile_json FROM execution_environment_profile_revisions
                 WHERE profile_id = ?1 AND revision = ?2",
            params!["p", 3_i64],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .unwrap(),
        ("original".to_owned(), br#"{"original":true}"#.to_vec())
    );
}

#[test]
fn run_binding_pins_the_first_current_snapshot() {
    let db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    let snapshot = br#"{"profile_id":"p","profile_revision":1,"snapshot_hash":"h"}"#;
    assert!(save_activation(
        &db,
        SaveActivationInput {
            profile_id: "p",
            revision: 1,
            scope: "application:application",
            status: "activated",
            snapshot_hash: "h",
            activation_json: br#"{}"#,
            snapshot_json: snapshot,
            now_ms: 1
        }
    )
    .unwrap());
    assert!(bind_current_to_run(&db, "run-1", "application:application", 2).unwrap());
    assert!(!bind_current_to_run(&db, "run-1", "application:application", 3).unwrap());
    assert_eq!(
        load_run_snapshot(&db, "run-1").unwrap(),
        Some(snapshot.to_vec())
    );
}

#[test]
fn activation_revision_fence_rejects_stale_current_snapshot() {
    let db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    assert!(save_activation(
        &db,
        SaveActivationInput {
            profile_id: "new",
            revision: 2,
            scope: "application:application",
            status: "activated",
            snapshot_hash: "new-hash",
            activation_json: br#"{}"#,
            snapshot_json: br#"{\"revision\":2}"#,
            now_ms: 2
        }
    )
    .unwrap());
    assert!(save_activation(
        &db,
        SaveActivationInput {
            profile_id: "old",
            revision: 1,
            scope: "application:application",
            status: "rolled_back",
            snapshot_hash: "old-hash",
            activation_json: br#"{}"#,
            snapshot_json: br#"{\"revision\":1}"#,
            now_ms: 3
        }
    )
    .unwrap());
    assert_eq!(
        load_current(&db, "application:application").unwrap(),
        Some(br#"{\"revision\":2}"#.to_vec())
    );
}

#[test]
fn duplicate_activation_revision_cannot_replace_current_snapshot() {
    let db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    assert!(save_activation(
        &db,
        SaveActivationInput {
            profile_id: "p",
            revision: 2,
            scope: "application:application",
            status: "activated",
            snapshot_hash: "first",
            activation_json: br#"{}"#,
            snapshot_json: br#"{"revision":2,"source":"first"}"#,
            now_ms: 2,
        },
    )
    .unwrap());
    assert!(save_activation(
        &db,
        SaveActivationInput {
            profile_id: "p",
            revision: 2,
            scope: "application:application",
            status: "replayed",
            snapshot_hash: "replacement",
            activation_json: br#"{"retry":true}"#,
            snapshot_json: br#"{"revision":2,"source":"replacement"}"#,
            now_ms: 3,
        },
    )
    .unwrap());
    assert_eq!(
        load_current(&db, "application:application").unwrap(),
        Some(br#"{"revision":2,"source":"first"}"#.to_vec())
    );
}

#[test]
fn idempotency_key_replays_only_the_same_command() {
    let db = Connection::open_in_memory().unwrap();
    install_schema(&db).unwrap();
    save_idempotent_command(
        &db,
        "application:application",
        "create-1",
        "a",
        br#"{"ok":true}"#,
        1,
    )
    .unwrap();
    assert_eq!(
        load_idempotent_command(&db, "application:application", "create-1").unwrap(),
        Some(("a".into(), br#"{"ok":true}"#.to_vec()))
    );
    save_idempotent_command(
        &db,
        "application:application",
        "create-1",
        "different",
        br#"{"ok":false}"#,
        2,
    )
    .unwrap();
    assert_eq!(
        load_idempotent_command(&db, "application:application", "create-1").unwrap(),
        Some(("a".into(), br#"{"ok":true}"#.to_vec()))
    );
}
