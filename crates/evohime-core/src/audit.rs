//! Bounded local audit trail contract.
//!
//! The contract is intentionally standalone. Runtime wiring, persistence and
//! export policy are separate concerns; this module only validates and
//! serializes append-only, redacted audit records deterministically.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub const MAX_RECORDS: usize = 4_096;
pub const MAX_EVENT_NAME_CHARS: usize = 64;
pub const MAX_FIELD_NAME_CHARS: usize = 64;
pub const MAX_FIELD_VALUE_CHARS: usize = 2_048;
pub const MAX_FIELDS: usize = 48;
pub const MAX_RECORD_BYTES: usize = 16 * 1024;
pub const MAX_JSONL_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind {
    Approval,
    ToolCall,
    Budget,
    Failure,
    Diff,
    Evidence,
    Storage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    TooManyFields { actual: usize, maximum: usize },
    TooManyRecords { actual: usize, maximum: usize },
    SequenceMismatch { expected: u64, actual: u64 },
    RecordTooLarge { actual: usize, maximum: usize },
    TrailTooLarge { actual: usize, maximum: usize },
}

impl fmt::Display for AuditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyFields { actual, maximum } => {
                write!(f, "audit record has {actual} fields, maximum is {maximum}")
            }
            Self::TooManyRecords { actual, maximum } => {
                write!(f, "audit trail has {actual} records, maximum is {maximum}")
            }
            Self::SequenceMismatch { expected, actual } => {
                write!(f, "audit sequence expected {expected}, got {actual}")
            }
            Self::RecordTooLarge { actual, maximum } => {
                write!(f, "audit record is {actual} bytes, maximum is {maximum}")
            }
            Self::TrailTooLarge { actual, maximum } => {
                write!(f, "audit trail is {actual} bytes, maximum is {maximum}")
            }
        }
    }
}

impl std::error::Error for AuditError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    pub sequence: u64,
    pub event_id: String,
    pub kind: AuditKind,
    pub actor: String,
    pub fields: BTreeMap<String, String>,
}

impl AuditRecord {
    pub fn new(
        sequence: u64,
        event_id: impl Into<String>,
        kind: AuditKind,
        actor: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, AuditError> {
        let event_id = event_id.into();
        let actor = actor.into();
        validate_text("event_id", &event_id, MAX_EVENT_NAME_CHARS, true)?;
        validate_text("actor", &actor, MAX_EVENT_NAME_CHARS, true)?;

        let mut redacted = BTreeMap::new();
        for (name, value) in fields {
            validate_text("field_name", &name, MAX_FIELD_NAME_CHARS, true)?;
            validate_text("field_value", &value, MAX_FIELD_VALUE_CHARS, false)?;
            redacted.insert(name.clone(), redact_value(&name, &value));
        }
        if redacted.len() > MAX_FIELDS {
            return Err(AuditError::TooManyFields {
                actual: redacted.len(),
                maximum: MAX_FIELDS,
            });
        }

        let record = Self {
            sequence,
            event_id,
            kind,
            actor,
            fields: redacted,
        };
        record.validate_size()?;
        Ok(record)
    }

    pub fn to_json_line(&self) -> Result<String, AuditError> {
        let json = serde_json::to_string(self).expect("AuditRecord is serializable");
        let bytes = json.len() + 1;
        if bytes > MAX_RECORD_BYTES {
            return Err(AuditError::RecordTooLarge {
                actual: bytes,
                maximum: MAX_RECORD_BYTES,
            });
        }
        Ok(format!("{json}\n"))
    }

    fn validate_size(&self) -> Result<(), AuditError> {
        let _ = self.to_json_line()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditTrail {
    records: Vec<AuditRecord>,
    jsonl_bytes: usize,
}

impl AuditTrail {
    pub fn append(&mut self, record: AuditRecord) -> Result<(), AuditError> {
        let expected = self.records.last().map_or(0, |item| item.sequence + 1);
        if record.sequence != expected {
            return Err(AuditError::SequenceMismatch {
                expected,
                actual: record.sequence,
            });
        }
        if self.records.len() >= MAX_RECORDS {
            return Err(AuditError::TooManyRecords {
                actual: self.records.len() + 1,
                maximum: MAX_RECORDS,
            });
        }
        let line = record.to_json_line()?;
        let next_bytes = self.jsonl_bytes.saturating_add(line.len());
        if next_bytes > MAX_JSONL_BYTES {
            return Err(AuditError::TrailTooLarge {
                actual: next_bytes,
                maximum: MAX_JSONL_BYTES,
            });
        }
        self.jsonl_bytes = next_bytes;
        self.records.push(record);
        Ok(())
    }

    pub fn records(&self) -> &[AuditRecord] {
        &self.records
    }

    pub fn as_jsonl(&self) -> Result<String, AuditError> {
        let mut output = String::with_capacity(self.jsonl_bytes);
        for record in &self.records {
            output.push_str(&record.to_json_line()?);
        }
        Ok(output)
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), AuditError> {
    if required && value.trim().is_empty() {
        return Err(AuditError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(AuditError::FieldTooLong { field, max });
    }
    Ok(())
}

fn redact_value(name: &str, value: &str) -> String {
    if is_sensitive_name(name) || contains_secret(value) {
        return "[REDACTED]".to_owned();
    }
    value.to_owned()
}

fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Bounded secret-shape scan shared with the execution ledger's redaction
/// metadata (план 08-4) — same markers as the audit log's own redaction, so
/// the two layers agree on what counts as secret-shaped.
pub(crate) fn contains_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("bearer ")
        || lower.contains("sk-")
        || lower.contains("ghp_")
        || lower.contains("github_pat_")
        || lower.contains("aiza")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(sequence: u64, fields: impl IntoIterator<Item = (String, String)>) -> AuditRecord {
        AuditRecord::new(sequence, "event", AuditKind::ToolCall, "core", fields)
            .expect("valid audit record")
    }

    #[test]
    fn redacts_secret_fields_and_values() {
        let item = record(
            0,
            [
                ("api_token".to_owned(), "do-not-leak".to_owned()),
                ("message".to_owned(), "Bearer abc123".to_owned()),
            ],
        );
        assert_eq!(item.fields["api_token"], "[REDACTED]");
        assert_eq!(item.fields["message"], "[REDACTED]");
    }

    #[test]
    fn jsonl_is_deterministic_and_newline_terminated() {
        let left = record(
            0,
            [
                ("z".to_owned(), "last".to_owned()),
                ("a".to_owned(), "first".to_owned()),
            ],
        );
        let right = record(
            0,
            [
                ("a".to_owned(), "first".to_owned()),
                ("z".to_owned(), "last".to_owned()),
            ],
        );
        assert_eq!(left.to_json_line().unwrap(), right.to_json_line().unwrap());
        assert!(left.to_json_line().unwrap().ends_with('\n'));
    }

    #[test]
    fn trail_is_append_only_and_requires_contiguous_sequence() {
        let mut trail = AuditTrail::default();
        trail.append(record(0, [])).unwrap();
        assert_eq!(
            trail.append(record(2, [])),
            Err(AuditError::SequenceMismatch {
                expected: 1,
                actual: 2
            })
        );
        trail.append(record(1, [])).unwrap();
        assert_eq!(trail.records().len(), 2);
    }

    #[test]
    fn bounds_reject_oversized_fields_and_record_count() {
        let too_many = (0..=MAX_FIELDS)
            .map(|index| (format!("field-{index}"), "value".to_owned()))
            .collect::<Vec<_>>();
        assert!(matches!(
            AuditRecord::new(0, "event", AuditKind::Budget, "core", too_many),
            Err(AuditError::TooManyFields { .. })
        ));

        let too_long = "x".repeat(MAX_FIELD_VALUE_CHARS + 1);
        assert!(matches!(
            AuditRecord::new(
                0,
                "event",
                AuditKind::Evidence,
                "core",
                [("value".to_owned(), too_long)]
            ),
            Err(AuditError::FieldTooLong { .. })
        ));
    }

    #[test]
    fn trail_jsonl_preserves_record_order() {
        let mut trail = AuditTrail::default();
        trail
            .append(record(0, [("stage".to_owned(), "approval".to_owned())]))
            .unwrap();
        trail
            .append(record(1, [("stage".to_owned(), "tool".to_owned())]))
            .unwrap();
        let jsonl = trail.as_jsonl().unwrap();
        let lines = jsonl.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("approval"));
        assert!(lines[1].contains("tool"));
    }
}
