use evohime_core::core_topic_subscription_event_bus::{transition, DeliveryState};
#[test]
fn nack_retries_then_dead_letters_without_blind_retry() {
    assert_eq!(
        transition(DeliveryState::InFlight, "nack", 1).unwrap(),
        DeliveryState::Queued
    );
    assert_eq!(
        transition(DeliveryState::InFlight, "nack", 3).unwrap(),
        DeliveryState::DeadLetter
    );
}
#[test]
fn crash_reconciliation_is_unknown() {
    assert_eq!(
        transition(DeliveryState::InFlight, "reconcile", 1).unwrap(),
        DeliveryState::Unknown
    );
}
