use super::*;

#[test]
fn work_item_update_is_revision_fenced_and_tables_are_additive() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    put_work_item(
        &connection,
        PutWorkItemInput {
            item_id: "w",
            revision: 1,
            status: "unassigned",
            assigned_instance_id: None,
            attempt: 0,
            item_json: b"{}",
            now_ms: 1,
        },
    )
    .unwrap();
    assert!(!replace_work_item(
        &connection,
        ReplaceWorkItemInput {
            item_id: "w",
            expected_revision: 0,
            revision: 2,
            status: "assigned",
            assigned_instance_id: Some("a"),
            attempt: 1,
            item_json: b"{}",
            now_ms: 2,
        }
    )
    .unwrap());
    assert!(replace_work_item(
        &connection,
        ReplaceWorkItemInput {
            item_id: "w",
            expected_revision: 1,
            revision: 2,
            status: "assigned",
            assigned_instance_id: Some("a"),
            attempt: 1,
            item_json: b"{}",
            now_ms: 2,
        }
    )
    .unwrap());
    assert_eq!(
        get_work_item(&connection, "w").unwrap(),
        Some(b"{}".to_vec())
    );
    put_idempotency(&connection, "k", b"result").unwrap();
    assert_eq!(
        get_idempotency(&connection, "k").unwrap(),
        Some(b"result".to_vec())
    );
}
