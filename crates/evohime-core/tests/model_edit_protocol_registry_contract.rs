use evohime_core::model_edit_protocol_registry::*;
use sha2::{Digest, Sha256};

fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[test]
fn all_protocol_kinds_are_explicit_and_revision_bound() {
    for protocol in [
        EditProtocol::SearchReplace {
            search: "a".into(),
            replace: "b".into(),
            expected_matches: 1,
        },
        EditProtocol::Patch {
            operations: vec![PatchOperation {
                start: 0,
                end: 1,
                replacement: "b".into(),
            }],
        },
        EditProtocol::Structured {
            fields: vec![StructuredField {
                path: "/title".into(),
                value: "x".into(),
            }],
        },
        EditProtocol::WholeFile {
            content: "b".into(),
        },
    ] {
        let definition = EditProtocolDefinition {
            schema_version: 1,
            protocol_id: "p".into(),
            revision: 1,
            model_profile_id: "profile".into(),
            file_path: "src/lib.rs".into(),
            expected_hash: hash("a"),
            protocol,
            max_output_bytes: 1024,
            repair_attempt: 0,
        };
        assert!(validate(&definition).is_ok());
        assert!(canonical_hash(&definition).is_ok());
    }
}
