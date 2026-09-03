use evohime_core::batch_invocation_runtime::{
    self as batch, BatchError, FailurePolicy, ItemStatus,
};

#[test]
fn contract_is_bounded_and_redacted_projection_keeps_item_drilldown() {
    let value = batch::new_batch(
        "contract-batch".into(),
        "workflow.review".into(),
        1,
        vec!["one".into(), "two".into()],
        1,
        FailurePolicy::Continue,
        10,
        &batch::default_policy(),
    )
    .unwrap();
    assert_eq!(
        value.items[0].run_id.as_deref(),
        Some("contract-batch:0:run:0")
    );
    assert_eq!(
        value.items[1].run_id.as_deref(),
        Some("contract-batch:1:run:0")
    );
    let projection = batch::projection(&value);
    assert_eq!(projection["redacted"], true);
    assert_eq!(projection["item_count"], 2);
    assert!(projection["items"][0].get("input_payload").is_none());
}

#[test]
fn start_respects_concurrency_and_unknown_cannot_blind_retry() {
    let mut value = batch::new_batch(
        "recovery-batch".into(),
        "workflow.review".into(),
        1,
        vec!["one".into(), "two".into()],
        1,
        FailurePolicy::Continue,
        10,
        &batch::default_policy(),
    )
    .unwrap();
    assert_eq!(
        batch::start_batch(&mut value, 1, 11, &batch::default_policy()).unwrap(),
        1
    );
    assert_eq!(
        value
            .items
            .iter()
            .filter(|item| item.status == ItemStatus::Running)
            .count(),
        1
    );
    let running = value
        .items
        .iter_mut()
        .find(|item| item.status == ItemStatus::Running)
        .unwrap();
    running.status = ItemStatus::Running;
    value.content_hash = batch::canonical_hash(&value);
    assert_eq!(
        batch::resume_pending(&mut value, 2, 12, &batch::default_policy()).unwrap(),
        1
    );
    let unknown = value
        .items
        .iter()
        .find(|item| item.status == ItemStatus::Unknown)
        .unwrap()
        .item_id
        .clone();
    assert_eq!(
        batch::record_result(
            &mut value,
            &unknown,
            3,
            ItemStatus::Running,
            None,
            None,
            13,
            &batch::default_policy()
        ),
        Err(BatchError::UnknownRetry)
    );
}
