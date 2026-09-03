use evohime_core::declarative_runtime_components::*;

#[test]
fn recovery_path_does_not_turn_unknown_outcome_into_success() {
    assert!(validate_transition(&RuntimeState::Starting, &RuntimeState::UnknownOutcome).is_ok());
    assert!(
        validate_transition(&RuntimeState::UnknownOutcome, &RuntimeState::Reconciliation).is_ok()
    );
    assert!(validate_transition(&RuntimeState::UnknownOutcome, &RuntimeState::Ready).is_err());
    assert!(validate_transition(&RuntimeState::Reconciliation, &RuntimeState::Ready).is_ok());
}
