//! Core-owned, fail-closed edit protocol registry.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PROTOCOLS: usize = 16;
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REPAIR_ATTEMPTS: u8 = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditProtocol {
    SearchReplace {
        search: String,
        replace: String,
        expected_matches: u32,
    },
    Patch {
        operations: Vec<PatchOperation>,
    },
    Structured {
        fields: Vec<StructuredField>,
    },
    WholeFile {
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchOperation {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredField {
    pub path: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditProtocolDefinition {
    pub schema_version: u32,
    pub protocol_id: String,
    pub revision: u64,
    pub model_profile_id: String,
    pub file_path: String,
    pub expected_hash: String,
    pub protocol: EditProtocol,
    pub max_output_bytes: usize,
    pub repair_attempt: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreflightResult {
    pub protocol_hash: String,
    pub match_count: u32,
    pub output_hash: String,
    pub output_bytes: usize,
    pub changed: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditProtocolError {
    #[error("invalid edit protocol: {0}")]
    Invalid(&'static str),
    #[error("unsupported edit protocol version")]
    UnsupportedVersion,
    #[error("revision/hash precondition is required")]
    MissingPrecondition,
    #[error("ambiguous search/replace match")]
    AmbiguousMatch,
    #[error("edit range is invalid")]
    InvalidRange,
    #[error("edit output exceeds limit")]
    TooLarge,
    #[error("repair attempts are exhausted")]
    RepairExhausted,
    #[error("stale file revision")]
    StaleRevision,
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.contains('\0')
}

pub fn validate(definition: &EditProtocolDefinition) -> Result<(), EditProtocolError> {
    if definition.schema_version != SCHEMA_VERSION {
        return Err(EditProtocolError::UnsupportedVersion);
    }
    if !valid_text(&definition.protocol_id, 128)
        || definition.revision == 0
        || !valid_text(&definition.model_profile_id, 128)
        || !valid_text(&definition.file_path, 4096)
        || definition.file_path.contains("..")
        || definition.file_path.starts_with('/')
    {
        return Err(EditProtocolError::Invalid("identity_or_path"));
    }
    if definition.expected_hash.len() != 64
        || !definition
            .expected_hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(EditProtocolError::MissingPrecondition);
    }
    if definition.max_output_bytes == 0
        || definition.max_output_bytes > MAX_INPUT_BYTES
        || definition.repair_attempt > MAX_REPAIR_ATTEMPTS
    {
        return Err(EditProtocolError::Invalid("limits"));
    }
    match &definition.protocol {
        EditProtocol::SearchReplace {
            search,
            replace,
            expected_matches,
        } => {
            if search.is_empty() || *expected_matches != 1 || replace.len() > MAX_INPUT_BYTES {
                return Err(EditProtocolError::Invalid("search_replace"));
            }
        }
        EditProtocol::Patch { operations } => {
            if operations.is_empty()
                || operations.len() > 256
                || operations
                    .iter()
                    .any(|op| op.start > op.end || op.replacement.len() > MAX_INPUT_BYTES)
            {
                return Err(EditProtocolError::Invalid("patch"));
            }
        }
        EditProtocol::Structured { fields } => {
            if fields.is_empty()
                || fields.len() > 128
                || fields.iter().any(|f| {
                    !valid_text(&f.path, 512) || !f.path.starts_with('/') || f.path.contains("..")
                })
            {
                return Err(EditProtocolError::Invalid("structured"));
            }
        }
        EditProtocol::WholeFile { content } => {
            if content.len() > definition.max_output_bytes {
                return Err(EditProtocolError::TooLarge);
            }
        }
    }
    Ok(())
}

pub fn canonical_hash(definition: &EditProtocolDefinition) -> Result<String, EditProtocolError> {
    validate(definition)?;
    Ok(hash(&serde_json::to_vec(definition).map_err(|_| {
        EditProtocolError::Invalid("serialization")
    })?))
}

pub fn preflight(
    definition: &EditProtocolDefinition,
    original: &str,
) -> Result<PreflightResult, EditProtocolError> {
    validate(definition)?;
    if hash(original.as_bytes()) != definition.expected_hash {
        return Err(EditProtocolError::StaleRevision);
    }
    let (output, matches) = match &definition.protocol {
        EditProtocol::SearchReplace {
            search, replace, ..
        } => {
            let count = original.matches(search).count() as u32;
            if count != 1 {
                return Err(if count > 1 {
                    EditProtocolError::AmbiguousMatch
                } else {
                    EditProtocolError::Invalid("search_not_found")
                });
            }
            (original.replacen(search, replace, 1), count)
        }
        EditProtocol::Patch { operations } => {
            let mut ordered = operations.clone();
            ordered.sort_by_key(|op| op.start);
            if ordered.windows(2).any(|w| w[0].end > w[1].start)
                || ordered.iter().any(|op| {
                    op.end > original.len()
                        || !original.is_char_boundary(op.start)
                        || !original.is_char_boundary(op.end)
                })
            {
                return Err(EditProtocolError::InvalidRange);
            }
            let mut out = original.to_owned();
            for op in ordered.into_iter().rev() {
                out.replace_range(op.start..op.end, &op.replacement);
            }
            (out, operations.len() as u32)
        }
        EditProtocol::Structured { fields } => {
            let mut value: serde_json::Value = serde_json::from_str(original)
                .map_err(|_| EditProtocolError::Invalid("structured_document"))?;
            for field in fields {
                let Some(key) = field.path.strip_prefix("/") else {
                    return Err(EditProtocolError::Invalid("structured_path"));
                };
                let object = value
                    .as_object_mut()
                    .ok_or(EditProtocolError::Invalid("structured_object"))?;
                object.insert(
                    key.to_owned(),
                    serde_json::Value::String(field.value.clone()),
                );
            }
            (
                serde_json::to_string_pretty(&value)
                    .map_err(|_| EditProtocolError::Invalid("serialization"))?,
                fields.len() as u32,
            )
        }
        EditProtocol::WholeFile { content } => (content.clone(), 1),
    };
    if output.len() > definition.max_output_bytes {
        return Err(EditProtocolError::TooLarge);
    }
    Ok(PreflightResult {
        protocol_hash: canonical_hash(definition)?,
        match_count: matches,
        output_hash: hash(output.as_bytes()),
        output_bytes: output.len(),
        changed: output != original,
    })
}

pub fn repair_feedback(
    error: &EditProtocolError,
    attempt: u8,
) -> Result<serde_json::Value, EditProtocolError> {
    if attempt >= MAX_REPAIR_ATTEMPTS {
        return Err(EditProtocolError::RepairExhausted);
    }
    Ok(
        serde_json::json!({"status":"repairable_failure","error_code":error.to_string(),"failed_only":true,"attempt":attempt,"next_attempt":attempt + 1}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition(protocol: EditProtocol, original: &str) -> EditProtocolDefinition {
        EditProtocolDefinition {
            schema_version: 1,
            protocol_id: "p".into(),
            revision: 1,
            model_profile_id: "profile".into(),
            file_path: "src/lib.rs".into(),
            expected_hash: hash(original.as_bytes()),
            protocol,
            max_output_bytes: 1024,
            repair_attempt: 0,
        }
    }
    #[test]
    fn search_replace_is_exact_and_dry_run() {
        let d = definition(
            EditProtocol::SearchReplace {
                search: "old".into(),
                replace: "new".into(),
                expected_matches: 1,
            },
            "old",
        );
        let result = preflight(&d, "old").unwrap();
        assert!(result.changed);
        assert_eq!(result.match_count, 1);
    }
    #[test]
    fn ambiguous_and_stale_fail_closed() {
        let d = definition(
            EditProtocol::SearchReplace {
                search: "x".into(),
                replace: "y".into(),
                expected_matches: 1,
            },
            "x x",
        );
        assert_eq!(preflight(&d, "x x"), Err(EditProtocolError::AmbiguousMatch));
        assert_eq!(preflight(&d, "z"), Err(EditProtocolError::StaleRevision));
    }
    #[test]
    fn patch_is_bounded_and_repair_feedback_is_limited() {
        let d = definition(
            EditProtocol::Patch {
                operations: vec![PatchOperation {
                    start: 0,
                    end: 1,
                    replacement: "y".into(),
                }],
            },
            "x",
        );
        assert_eq!(preflight(&d, "x").unwrap().output_hash, hash(b"y"));
        assert!(repair_feedback(&EditProtocolError::Invalid("x"), 0).is_ok());
        assert_eq!(
            repair_feedback(&EditProtocolError::Invalid("x"), 3),
            Err(EditProtocolError::RepairExhausted)
        );
    }
}
