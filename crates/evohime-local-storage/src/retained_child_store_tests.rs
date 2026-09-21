use super::*;
#[derive(Debug, Serialize, serde::Deserialize, PartialEq)]
struct R {
    v: u8,
}
#[test]
fn scope_and_dedup() {
    let mut c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert_eq!(
        RetainedChildStore::next_parent_sequence(&mut c, "p").unwrap(),
        1
    );
    let input = InsertFollowUpInput {
        parent_id: "p",
        child_id: "c",
        idempotency_key: "k",
        expected_revision: 1,
        parent_sequence: 1,
        request: &R { v: 1 },
        now_ms: 1,
    };
    assert!(RetainedChildStore::insert_follow_up(&c, input).unwrap());
    assert!(!RetainedChildStore::insert_follow_up(&c, input).unwrap());
    assert_eq!(
        RetainedChildStore::get_child::<R>(&c, "other", "c").unwrap(),
        None
    );
}
#[test]
fn unknown_delivery_is_terminal_and_not_success() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    assert!(RetainedChildStore::insert_mailbox(
        &c,
        InsertMailboxInput {
            parent_id: "p",
            child_id: "c",
            idempotency_key: "k",
            message_id: "m",
            parent_sequence: 1,
            entry: &R { v: 1 },
            now_ms: 1,
        },
    )
    .unwrap());
    assert!(
        RetainedChildStore::transition_mailbox(&c, "p", "m", "pending", "unknown", None).unwrap()
    );
    assert!(
        !RetainedChildStore::transition_mailbox(&c, "p", "m", "unknown", "delivered", Some(2))
            .unwrap()
    );
}

#[test]
fn mailbox_limit_is_enforced_by_the_insert_statement() {
    let c = Connection::open_in_memory().unwrap();
    install_schema(&c).unwrap();
    for index in 0..MAX_PENDING_PER_CHILD {
        let key = format!("key-{index}");
        let message = format!("message-{index}");
        assert!(RetainedChildStore::insert_mailbox(
            &c,
            InsertMailboxInput {
                parent_id: "p",
                child_id: "c",
                idempotency_key: &key,
                message_id: &message,
                parent_sequence: index as u64,
                entry: &R { v: index as u8 },
                now_ms: index as u64,
            },
        )
        .unwrap());
    }
    assert!(matches!(
        RetainedChildStore::insert_mailbox(
            &c,
            InsertMailboxInput {
                parent_id: "p",
                child_id: "c",
                idempotency_key: "key-over-limit",
                message_id: "message-over-limit",
                parent_sequence: MAX_PENDING_PER_CHILD as u64,
                entry: &R { v: 0 },
                now_ms: MAX_PENDING_PER_CHILD as u64,
            },
        ),
        Err(RetainedStoreError::LimitExceeded)
    ));
}
