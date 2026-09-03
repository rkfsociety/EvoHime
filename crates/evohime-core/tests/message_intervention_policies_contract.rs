use evohime_core::message_intervention_policies::*;

fn policy() -> MessageInterventionPolicy {
    let mut value = MessageInterventionPolicy {
        schema_version: 1,
        id: "policy".into(),
        version: 1,
        hooks: vec![MessageInterventionHook {
            id: "redact".into(),
            version: 1,
            priority: 1,
            phases: vec![HookPhase::BeforeRecipientContext],
            action: InterventionAction::Redact,
            failure_mode: FailureMode::FailClosed,
            allowed_routes: vec!["recipient".into()],
            allowed_sensitivity: vec![SensitivityClass::Internal],
            message_kinds: vec!["notice".into()],
        }],
        content_hash: String::new(),
    };
    value.content_hash = canonical_hash(&value).unwrap();
    value
}

fn context() -> MessageInterventionContext {
    MessageInterventionContext {
        team_session_id: "session".into(),
        sender: "sender".into(),
        recipients: vec!["recipient".into()],
        message_kind: "notice".into(),
        contract_ref: None,
        payload_metadata: "bytes=4".into(),
        sensitivity: SensitivityClass::Internal,
        phase: HookPhase::BeforeRecipientContext,
        causation_id: None,
        routing_snapshot_hash: "snapshot".into(),
        idempotency_key: "delivery".into(),
    }
}

#[test]
fn typed_projection_patch_is_bounded_and_redacted() {
    let verdict = evaluate(&policy(), &context(), false).unwrap();
    assert_eq!(verdict.action, InterventionAction::Redact);
    assert_eq!(
        verdict.projection_patches,
        vec!["payload_metadata=redacted"]
    );
    assert_eq!(verdict.redaction_status, "metadata_only");
}

#[test]
fn duplicate_delivery_is_not_replayed() {
    assert_eq!(
        evaluate(&policy(), &context(), true),
        Err(InterventionError::Duplicate)
    );
}
