use evohime_core::model_edit_protocol_registry::*;
use sha2::{Digest, Sha256};

#[test]
fn stale_and_ambiguous_edits_never_produce_output() {
    let definition = EditProtocolDefinition {
        schema_version: 1,
        protocol_id: "p".into(),
        revision: 1,
        model_profile_id: "profile".into(),
        file_path: "src/lib.rs".into(),
        expected_hash: hex::encode(Sha256::digest(b"x x")),
        protocol: EditProtocol::SearchReplace {
            search: "x".into(),
            replace: "y".into(),
            expected_matches: 1,
        },
        max_output_bytes: 1024,
        repair_attempt: 0,
    };
    assert_eq!(
        preflight(&definition, "x x"),
        Err(EditProtocolError::AmbiguousMatch)
    );
    assert_eq!(
        preflight(&definition, "changed"),
        Err(EditProtocolError::StaleRevision)
    );
    assert!(repair_feedback(&EditProtocolError::AmbiguousMatch, MAX_REPAIR_ATTEMPTS).is_err());
}
