use super::*;

#[test]
fn schema_is_idempotent_and_separates_layout_hash() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    install_schema(&connection).unwrap();
    let tables: i64 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE 'visual_workflow_%'", [], |row| row.get(0)).unwrap();
    assert_eq!(tables, 3);
}

#[test]
fn draft_revision_and_handoff_publish_are_atomic() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let definition = br#"{"contract_version":"visual-workflow-builder/v1","graph":{"graph_id":"g","version":1},"layout":{}}"#;
    let first = save_draft(
        &connection,
        SaveDraft {
            draft_id: "d",
            owner_scope: "w",
            expected_revision: 0,
            definition_json: definition,
            layout_json: b"{}",
            execution_hash: "e",
            layout_hash: "l",
            composer_provenance_json: None,
            updated_at_ms: 1,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(first, 1);
    save_draft(
        &connection,
        SaveDraft {
            draft_id: "d",
            owner_scope: "w",
            expected_revision: 1,
            definition_json: definition,
            layout_json: b"{}",
            execution_hash: "e",
            layout_hash: "l",
            composer_provenance_json: Some(b"{\"request_hash\":\"r\"}"),
            updated_at_ms: 2,
        },
    )
    .unwrap()
    .unwrap();
    save_draft(
        &connection,
        SaveDraft {
            draft_id: "d",
            owner_scope: "w",
            expected_revision: 2,
            definition_json: definition,
            layout_json: b"{}",
            execution_hash: "e",
            layout_hash: "l",
            composer_provenance_json: None,
            updated_at_ms: 2,
        },
    )
    .unwrap()
    .unwrap();
    let provenance: Option<Vec<u8>> = connection
        .query_row(
            "SELECT composer_provenance_json FROM visual_workflow_drafts WHERE draft_id='d'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        provenance.as_deref(),
        Some(b"{\"request_hash\":\"r\"}".as_slice())
    );
    assert_eq!(
        save_draft(
            &connection,
            SaveDraft {
                draft_id: "d",
                owner_scope: "w",
                expected_revision: 0,
                definition_json: definition,
                layout_json: b"{}",
                execution_hash: "e",
                layout_hash: "l",
                composer_provenance_json: None,
                updated_at_ms: 2
            }
        )
        .unwrap(),
        Err("stale_revision")
    );
    issue_handoff(
        &connection,
        Handoff {
            handle: "h",
            draft_id: "d",
            owner_scope: "w",
            revision: 3,
            draft_hash: "e",
            precondition: "3:e",
            created_at_ms: 3,
        },
    )
    .unwrap();
    assert!(publish_from_handoff(&connection, "h", "d", "w", 4)
        .unwrap()
        .is_ok());
    assert!(publish_from_handoff(&connection, "h", "d", "w", 5)
        .unwrap()
        .is_err());
    let versions: i64 = connection
        .query_row("SELECT count(*) FROM visual_workflow_versions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(versions, 1);
}

#[test]
fn creating_a_draft_cannot_take_over_an_identifier_owned_by_another_scope() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    save_draft(
        &connection,
        SaveDraft {
            draft_id: "shared-id",
            owner_scope: "workspace:one",
            expected_revision: 0,
            definition_json: b"{}",
            layout_json: b"{}",
            execution_hash: "execution-hash",
            layout_hash: "layout-hash",
            composer_provenance_json: None,
            updated_at_ms: 1,
        },
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        save_draft(
            &connection,
            SaveDraft {
                draft_id: "shared-id",
                owner_scope: "workspace:two",
                expected_revision: 0,
                definition_json: b"{\"other\":true}",
                layout_json: b"{}",
                execution_hash: "other-execution-hash",
                layout_hash: "other-layout-hash",
                composer_provenance_json: None,
                updated_at_ms: 2,
            },
        )
        .unwrap(),
        Err("owner_conflict")
    );
    assert_eq!(
        read_draft(&connection, "shared-id", "workspace:one")
            .unwrap()
            .map(|draft| draft.0),
        Some(1)
    );
    assert!(read_draft(&connection, "shared-id", "workspace:two")
        .unwrap()
        .is_none());
}

#[test]
fn reissuing_handoff_updates_without_replacing_row() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    connection
            .execute(
                "INSERT INTO visual_workflow_drafts(draft_id,owner_scope,revision,state,definition_json,layout_json,execution_hash,layout_hash,updated_at_ms) VALUES('draft','scope',1,'valid', '{}', '{}', 'hash-1', 'layout-1', 1)",
                [],
            )
            .unwrap();
    let first = Handoff {
        handle: "handoff",
        draft_id: "draft",
        owner_scope: "scope",
        revision: 1,
        draft_hash: "hash-1",
        precondition: "save-1",
        created_at_ms: 1,
    };
    issue_handoff(
        &connection,
        Handoff {
            handle: first.handle,
            draft_id: first.draft_id,
            owner_scope: first.owner_scope,
            revision: first.revision,
            draft_hash: first.draft_hash,
            precondition: first.precondition,
            created_at_ms: first.created_at_ms,
        },
    )
    .unwrap();
    issue_handoff(
        &connection,
        Handoff {
            draft_id: "draft",
            revision: 2,
            draft_hash: "hash-2",
            precondition: "save-2",
            created_at_ms: 2,
            handle: "handoff",
            owner_scope: "scope",
        },
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM visual_workflow_handoffs WHERE handle='handoff'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    assert!(consume_handoff(&connection, "handoff", "scope").unwrap());
}

#[test]
fn stale_handoff_cannot_publish_new_draft_revision() {
    let connection = Connection::open_in_memory().unwrap();
    install_schema(&connection).unwrap();
    let definition = br#"{"contract_version":"visual-workflow-builder/v1","graph":{"graph_id":"g","version":1},"layout":{}}"#;
    save_draft(
        &connection,
        SaveDraft {
            draft_id: "d",
            owner_scope: "w",
            expected_revision: 0,
            definition_json: definition,
            layout_json: b"{}",
            execution_hash: "e",
            layout_hash: "l",
            composer_provenance_json: None,
            updated_at_ms: 1,
        },
    )
    .unwrap()
    .unwrap();
    issue_handoff(
        &connection,
        Handoff {
            handle: "h",
            draft_id: "d",
            owner_scope: "w",
            revision: 1,
            draft_hash: "e",
            precondition: "1:e",
            created_at_ms: 2,
        },
    )
    .unwrap();
    save_draft(
        &connection,
        SaveDraft {
            draft_id: "d",
            owner_scope: "w",
            expected_revision: 1,
            definition_json: definition,
            layout_json: b"{}",
            execution_hash: "new",
            layout_hash: "l",
            composer_provenance_json: None,
            updated_at_ms: 3,
        },
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        publish_from_handoff(&connection, "h", "d", "w", 4).unwrap(),
        Err("stale_handoff")
    );
    let status: String = connection
        .query_row(
            "SELECT status FROM visual_workflow_handoffs WHERE handle='h'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "active");
}
