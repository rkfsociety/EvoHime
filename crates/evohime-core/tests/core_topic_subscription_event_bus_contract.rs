use evohime_core::core_topic_subscription_event_bus::*;
fn event() -> Event {
    let mut e = Event {
        event_id: "e".into(),
        topic: Topic {
            namespace: "workflow".into(),
            name: "done".into(),
            partition_key: Some("p".into()),
        },
        schema: "workflow.done".into(),
        schema_version: 1,
        producer: "core".into(),
        workflow_run_id: None,
        goal_id: None,
        correlation_id: "c".into(),
        causation_id: None,
        created_at_ms: 1,
        payload: serde_json::json!({"ok":true}),
        content_hash: String::new(),
    };
    let mut c = e.clone();
    c.content_hash.clear();
    e.content_hash = hash(&c).unwrap();
    e
}
#[test]
fn selector_delivery_and_capability_contract() {
    let e = event();
    assert!(matches(&Selector::NamespacePrefix("work".into()), &e));
    assert_eq!(
        transition(DeliveryState::InFlight, "nack", 3).unwrap(),
        DeliveryState::DeadLetter
    );
    assert_eq!(authorize("events.read", &[]), Err(Error::CapabilityDenied));
    assert!(validate_event(&e).is_ok());
}
