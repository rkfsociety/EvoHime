//! Core-owned, fail-closed edit protocol registry.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Current serialized schema version for edit protocol definitions.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of edit protocols accepted by a registry.
pub const MAX_PROTOCOLS: usize = 16;
/// Maximum size accepted for one edit input or replacement payload.
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of feedback-guided repair attempts.
pub const MAX_REPAIR_ATTEMPTS: u8 = 3;

/// Declarative edit strategy evaluated against a revision-bound source file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EditProtocol {
    /// Replace one exact search string after confirming it occurs once.
    SearchReplace {
        /// Exact text that must occur in the source.
        search: String,
        /// Text substituted for the matched range.
        replace: String,
        /// Required match count; currently only one is accepted.
        expected_matches: u32,
    },
    /// Apply non-overlapping byte-offset replacements.
    Patch {
        /// Operations applied from the end of the source toward its beginning.
        operations: Vec<PatchOperation>,
    },
    /// Set top-level JSON object fields by JSON-pointer-like paths.
    Structured {
        /// Fields to replace in the structured document.
        fields: Vec<StructuredField>,
    },
    /// Replace the complete source with fixed content.
    WholeFile {
        /// Complete replacement contents.
        content: String,
    },
}

/// One source byte range and its replacement text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchOperation {
    /// Inclusive start byte offset in the original UTF-8 text.
    pub start: usize,
    /// Exclusive end byte offset in the original UTF-8 text.
    pub end: usize,
    /// Text inserted in place of the selected range.
    pub replacement: String,
}

/// One structured-document field update.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructuredField {
    /// Top-level slash-prefixed object key to update.
    pub path: String,
    /// String value written to that key.
    pub value: String,
}

/// Revision-bound edit request and its resource/output limits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditProtocolDefinition {
    /// Schema version used to interpret this request.
    pub schema_version: u32,
    /// Stable identifier for this edit protocol instance.
    pub protocol_id: String,
    /// Nonzero revision of the protocol definition.
    pub revision: u64,
    /// Model profile that produced or is permitted to execute the edit.
    pub model_profile_id: String,
    /// Workspace-relative target path.
    pub file_path: String,
    /// SHA-256 digest of the exact source revision required for application.
    pub expected_hash: String,
    /// Strategy and replacement data for the edit.
    pub protocol: EditProtocol,
    /// Maximum encoded output size in bytes.
    pub max_output_bytes: usize,
    /// Current repair attempt, bounded by `MAX_REPAIR_ATTEMPTS`.
    pub repair_attempt: u8,
}

/// Dry-run summary of a validated edit against the expected source revision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreflightResult {
    /// Digest of the edit protocol definition.
    pub protocol_hash: String,
    /// Number of matched or applied operations.
    pub match_count: u32,
    /// Digest of the proposed output.
    pub output_hash: String,
    /// Encoded size of the proposed output.
    pub output_bytes: usize,
    /// Whether the proposed output differs from the source.
    pub changed: bool,
}

/// Validation, revision, size, or repair-limit failure.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditProtocolError {
    /// A protocol field or payload violates its contract.
    #[error("invalid edit protocol: {0}")]
    Invalid(&'static str),
    /// The definition uses an unsupported schema version.
    #[error("unsupported edit protocol version")]
    UnsupportedVersion,
    /// A valid expected source digest is required before editing.
    #[error("revision/hash precondition is required")]
    MissingPrecondition,
    /// Search text occurs more than once, so replacement is ambiguous.
    #[error("ambiguous search/replace match")]
    AmbiguousMatch,
    /// Patch offsets overlap, exceed the source, or split a UTF-8 character.
    #[error("edit range is invalid")]
    InvalidRange,
    /// Proposed output exceeds the definition's size bound.
    #[error("edit output exceeds limit")]
    TooLarge,
    /// No further repair attempt is permitted.
    #[error("repair attempts are exhausted")]
    RepairExhausted,
    /// Source bytes do not match the expected digest.
    #[error("stale file revision")]
    StaleRevision,
}

fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.contains('\0')
}

/// Validates request identity, source precondition, strategy data, and size bounds.
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

/// Validates and hashes a complete edit protocol definition.
pub fn canonical_hash(definition: &EditProtocolDefinition) -> Result<String, EditProtocolError> {
    validate(definition)?;
    Ok(hash(&serde_json::to_vec(definition).map_err(|_| {
        EditProtocolError::Invalid("serialization")
    })?))
}

/// Evaluates the proposed edit without writing the target file.
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

/// Produces bounded structured feedback for a retryable edit failure.
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
