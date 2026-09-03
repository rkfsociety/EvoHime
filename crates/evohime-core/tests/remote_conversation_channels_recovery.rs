use evohime_core::remote_conversation_channels::*;

#[test]
fn revoke_duplicate_and_attachment_limits_fail_closed() {
    let c = ChannelConnection {
        schema_version: 1,
        connection_id: "c".into(),
        owner_scope: "o".into(),
        provider: Provider::Telegram,
        external_identity: "u".into(),
        state: ConnectionState::Revoked,
        revision: 1,
        queue_limit: 1,
        attachment_limit_bytes: 1,
        expires_at_ms: 1000,
    };
    let m = InboundMessage {
        message_id: "m".into(),
        connection_id: "c".into(),
        external_identity: "u".into(),
        text: "x".into(),
        attachment_bytes: 2,
    };
    assert_eq!(
        admit_message(&c, &m, 0, false, 1),
        Err(ChannelError::Revoked)
    );
}
