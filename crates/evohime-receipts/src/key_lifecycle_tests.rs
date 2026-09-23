use super::*;

#[cfg(windows)]
#[test]
fn protected_storage_survives_rotation_with_history_fallback() {
    let root = std::env::temp_dir().join(format!("evohime-receipt-storage-{}", Uuid::now_v7()));
    let manager = ReceiptKeyManager::new(&root);
    manager.initialize().unwrap();
    let first_id = manager.storage_key_id().unwrap();
    let envelope = manager.protect_storage(b"bounded recovery").unwrap();
    let second_id = manager.rotate_storage_key(true).unwrap();
    assert_ne!(first_id, second_id);
    assert_eq!(
        manager.unprotect_storage(&envelope).unwrap(),
        b"bounded recovery"
    );
    assert!(manager.rotate_storage_key(false).is_err());
    let _ = std::fs::remove_dir_all(root);
}

fn genesis() -> KeyTransition {
    let pair = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let signer = Ed25519KeyPair::from_pkcs8(pair.as_ref()).unwrap();
    let public = signer.public_key().as_ref().to_vec();
    let mut item = KeyTransition {
        transition_version: 1,
        transition_id: Uuid::now_v7().to_string(),
        created_at: now(),
        reason: "initial".into(),
        actor: "system".into(),
        previous_key_id: None,
        new_key_id: key_id(&public),
        new_public_key: b64(&public),
        continuity: "genesis".into(),
        signed_by_key_id: key_id(&public),
        signature: String::new(),
        previous_transition_hash: None,
    };
    item.signature = b64(signer.sign(&signed_bytes(&item).unwrap()).as_ref());
    item
}

#[test]
fn synthetic_genesis_requires_explicit_pin() {
    let item = genesis();
    assert_eq!(
        verify_transitions(std::slice::from_ref(&item), None).unwrap(),
        VerificationStatus::Untrusted
    );
    assert_eq!(
        verify_transitions(std::slice::from_ref(&item), Some(&item.new_key_id)).unwrap(),
        VerificationStatus::Verified
    );
}

#[test]
fn rotation_state_rejects_invalid_phase() {
    let state = RotationState {
        state_version: 1,
        rotation_id: Uuid::now_v7().to_string(),
        phase: "prepared".into(),
        old_key_id: "ed25519:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .into(),
        new_key_id: "ed25519:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .into(),
        transition_hash: "c".repeat(64),
        error_code: None,
        created_at: now(),
        updated_at: now(),
        reason: "manual".into(),
        actor: "user".into(),
        active_key_observed: true,
        audit_event_id: "audit".into(),
    };
    assert!(validate_rotation_state(&state).is_ok());
    let mut bad = state;
    bad.phase = "private_key".into();
    assert_eq!(
        validate_rotation_state(&bad).unwrap_err().to_string(),
        "key.rotation_incomplete"
    );
}

#[test]
fn verifier_accepts_reordered_chain_and_rejects_fork() {
    // Build a transition signed by the genesis key, retaining that signer
    // explicitly so the vector exercises the same verification path.
    let pair = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
    let genesis_signer = Ed25519KeyPair::from_pkcs8(pair.as_ref()).unwrap();
    let genesis_public = genesis_signer.public_key().as_ref().to_vec();
    let mut first = KeyTransition {
        transition_version: 1,
        transition_id: Uuid::now_v7().to_string(),
        created_at: now(),
        reason: "initial".into(),
        actor: "system".into(),
        previous_key_id: None,
        new_key_id: key_id(&genesis_public),
        new_public_key: b64(&genesis_public),
        continuity: "genesis".into(),
        signed_by_key_id: key_id(&genesis_public),
        signature: String::new(),
        previous_transition_hash: None,
    };
    first.signature = b64(genesis_signer.sign(&signed_bytes(&first).unwrap()).as_ref());
    let (second_signer, _) = SecretSigner::generate().unwrap();
    let second_public = second_signer.public().unwrap();
    let mut second = KeyTransition {
        transition_version: 1,
        transition_id: Uuid::now_v7().to_string(),
        created_at: now(),
        reason: "manual".into(),
        actor: "user".into(),
        previous_key_id: Some(first.new_key_id.clone()),
        new_key_id: key_id(&second_public),
        new_public_key: b64(&second_public),
        continuity: "chained".into(),
        signed_by_key_id: first.new_key_id.clone(),
        signature: String::new(),
        previous_transition_hash: Some(transition_hash(&first).unwrap()),
    };
    second.signature = b64(genesis_signer
        .sign(&signed_bytes(&second).unwrap())
        .as_ref());
    assert_eq!(
        verify_transitions(&[second.clone(), first.clone()], Some(&first.new_key_id)).unwrap(),
        VerificationStatus::Verified
    );
    let mut fork = second;
    fork.transition_id = Uuid::now_v7().to_string();
    assert!(verify_transitions(&[first, fork], None).is_err());
}

#[test]
fn shared_key_transition_vector_is_verified() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../../contracts/receipts/v1/key-transition-vectors.json"
    ))
    .unwrap();
    let value = &vector["positive"][0];
    let transition: KeyTransition = serde_json::from_value(serde_json::json!({
        "transition_version": 1,
        "transition_id": "018c4f4e-5c00-7abc-8def-0123456789ab",
        "created_at": "2025-01-15T12:34:56.789Z",
        "reason": "initial",
        "actor": "system",
        "new_key_id": value["new_key_id"],
        "new_public_key": value["new_public_key"],
        "continuity": "genesis",
        "signed_by_key_id": value["new_key_id"],
        "signature": value["signature"]
    }))
    .unwrap();
    assert_eq!(
        String::from_utf8(signed_bytes(&transition).unwrap()).unwrap(),
        value["canonical_unsigned"]
    );
    assert_eq!(
        verify_transitions(
            std::slice::from_ref(&transition),
            Some(&transition.new_key_id)
        )
        .unwrap(),
        VerificationStatus::Verified
    );
}

#[test]
fn sqlite_transition_and_audit_commit_is_idempotent_and_fork_safe() {
    let mut connection = rusqlite::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE receipt_key_transitions (sequence INTEGER PRIMARY KEY AUTOINCREMENT, transition_id TEXT NOT NULL UNIQUE, transition_hash TEXT NOT NULL UNIQUE, previous_key_id TEXT, new_key_id TEXT NOT NULL, continuity TEXT NOT NULL, canonical_json BLOB NOT NULL, created_at TEXT NOT NULL); CREATE TABLE receipt_key_audit (event_id TEXT PRIMARY KEY, transition_id TEXT NOT NULL UNIQUE, event_type TEXT NOT NULL, old_key_id TEXT, new_key_id TEXT, transition_hash TEXT NOT NULL, reason TEXT NOT NULL, actor TEXT NOT NULL, outcome TEXT NOT NULL, error_code TEXT, created_at TEXT NOT NULL);").unwrap();
    let transition = genesis();
    commit_transition_and_audit(
        &mut connection,
        &transition,
        "audit-1",
        "initial",
        "system",
        "ok",
        None,
    )
    .unwrap();
    commit_transition_and_audit(
        &mut connection,
        &transition,
        "audit-1",
        "initial",
        "system",
        "ok",
        None,
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM receipt_key_transitions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );
    let mut fork = transition;
    fork.signature = "invalid".into();
    assert_eq!(
        commit_transition_and_audit(
            &mut connection,
            &fork,
            "audit-2",
            "initial",
            "system",
            "ok",
            None
        )
        .unwrap_err()
        .to_string(),
        "key.rotation_fork"
    );
}
