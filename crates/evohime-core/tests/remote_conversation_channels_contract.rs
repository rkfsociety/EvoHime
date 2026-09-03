use evohime_core::remote_conversation_channels::*;

#[test]
fn pairing_and_identity_are_core_bound() {
    let c = ChannelConnection {
        schema_version: 1,
        connection_id: "c".into(),
        owner_scope: "o".into(),
        provider: Provider::Telegram,
        external_identity: "u".into(),
        state: ConnectionState::Active,
        revision: 1,
        queue_limit: 4,
        attachment_limit_bytes: 1024,
        expires_at_ms: 1000,
    };
    let mut p = PairingCode {
        connection_id: "c".into(),
        code_hash: hash_pairing_code("x").unwrap(),
        expires_at_ms: 100,
        consumed: false,
    };
    assert!(consume_pairing(&c, &mut p, "x", 1, "u").is_ok());
    assert!(canonical_hash(&c).is_ok());
}
