use evohime_core::human_work_items::{
    HumanWorkItem, HumanWorkItemState, HumanWorkItemsRegistry, ResponseSchema,
};

fn item() -> HumanWorkItem {
    HumanWorkItem {
        schema_version: 1,
        id: "inbox-1".into(),
        revision: 1,
        title: "Verify".into(),
        instructions: "Verify the output".into(),
        response_schema: ResponseSchema::Text,
        state: HumanWorkItemState::WaitingForHuman,
        team_slot: None,
        response: None,
        submitted_by: None,
        expires_at_ms: None,
    }
}

#[test]
fn restartable_snapshot_rejects_stale_late_submission() {
    let mut registry = HumanWorkItemsRegistry::default();
    registry.create(item(), "create").unwrap();
    let started = registry
        .transition("inbox-1", 1, "start", None, "shell", 0)
        .unwrap();
    assert!(registry
        .transition("inbox-1", 1, "submit", Some("done".into()), "shell", 0)
        .is_err());
    assert_eq!(
        registry
            .transition("inbox-1", started.revision, "cancel", None, "shell", 0)
            .unwrap()
            .state,
        HumanWorkItemState::Cancelled
    );
}
