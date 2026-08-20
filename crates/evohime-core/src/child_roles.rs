//! Bounded contract for child roles and task handoff.
//!
//! The contract is deliberately standalone: runtime wiring and `lib.rs`
//! integration belong to a later task. Handoffs are immutable, redacted at
//! construction time, and serialize deterministically for tracing and replay.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub const MAX_ID_CHARS: usize = 128;
pub const MAX_ROLE_NAME_CHARS: usize = 64;
pub const MAX_PURPOSE_CHARS: usize = 512;
pub const MAX_PAYLOAD_FIELDS: usize = 32;
pub const MAX_FIELD_NAME_CHARS: usize = 64;
pub const MAX_FIELD_VALUE_CHARS: usize = 2_048;
pub const MAX_HANDOFF_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildRole {
    Coordinator,
    Researcher,
    Planner,
    Implementer,
    Reviewer,
    Tester,
    Custom,
}

/// Capability matrix for specialized workflows. The matrix is advisory input
/// to Core policy; every tool call still re-checks the effective grant.
pub fn allowed_capabilities(role: ChildRole) -> &'static [&'static str] {
    match role {
        ChildRole::Coordinator => &[
            "workspace.read",
            "workspace.search",
            "git.status",
            "git.diff",
        ],
        ChildRole::Researcher => &["workspace.read", "workspace.search", "git.status"],
        ChildRole::Implementer => &[
            "workspace.read",
            "workspace.search",
            "git.status",
            "git.diff",
        ],
        ChildRole::Tester => &[
            "workspace.read",
            "workspace.search",
            "git.status",
            "git.diff",
        ],
        ChildRole::Reviewer => &[
            "workspace.read",
            "workspace.search",
            "git.status",
            "git.diff",
        ],
        ChildRole::Planner | ChildRole::Custom => &["workspace.read", "workspace.search"],
    }
}

pub fn can_request_capability(role: ChildRole, capability: &str) -> bool {
    allowed_capabilities(role).contains(&capability)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    Delegate,
    ReturnResult,
    RequestReview,
    RequestRetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    Pending,
    Accepted,
    Rejected,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    TooManyFields { actual: usize, maximum: usize },
    InvalidRoleName,
    HandoffTooLarge { actual: usize, maximum: usize },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyFields { actual, maximum } => {
                write!(f, "payload has {actual} fields, maximum is {maximum}")
            }
            Self::InvalidRoleName => write!(f, "custom role name is invalid"),
            Self::HandoffTooLarge { actual, maximum } => {
                write!(f, "handoff is {actual} bytes, maximum is {maximum}")
            }
        }
    }
}

impl std::error::Error for ContractError {}

/// A role identity may use a built-in role or a bounded custom label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleIdentity {
    pub role: ChildRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl RoleIdentity {
    pub fn builtin(role: ChildRole) -> Self {
        Self { role, name: None }
    }

    pub fn custom(name: impl Into<String>) -> Result<Self, ContractError> {
        let name = name.into();
        validate_text("role_name", &name, MAX_ROLE_NAME_CHARS, true)?;
        if !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_ .".contains(character))
        {
            return Err(ContractError::InvalidRoleName);
        }
        Ok(Self {
            role: ChildRole::Custom,
            name: Some(name),
        })
    }

    fn validate(&self) -> Result<(), ContractError> {
        match (self.role, self.name.as_deref()) {
            (ChildRole::Custom, Some(name)) => {
                validate_text("role_name", name, MAX_ROLE_NAME_CHARS, true)?;
                if !name.chars().all(|character| {
                    character.is_ascii_alphanumeric() || "-_ .".contains(character)
                }) {
                    return Err(ContractError::InvalidRoleName);
                }
                Ok(())
            }
            (ChildRole::Custom, None) => Err(ContractError::InvalidRoleName),
            (_, Some(name)) => Err(if name.trim().is_empty() {
                ContractError::EmptyField("role_name")
            } else {
                ContractError::InvalidRoleName
            }),
            (_, None) => Ok(()),
        }
    }
}

/// Sorted payload makes handoff JSON stable independently of insertion order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HandoffPayload {
    #[serde(flatten)]
    pub fields: BTreeMap<String, String>,
}

impl HandoffPayload {
    pub fn new(fields: impl IntoIterator<Item = (String, String)>) -> Result<Self, ContractError> {
        let mut redacted = BTreeMap::new();
        for (name, value) in fields {
            validate_text("field_name", &name, MAX_FIELD_NAME_CHARS, true)?;
            validate_text("field_value", &value, MAX_FIELD_VALUE_CHARS, false)?;
            redacted.insert(name.clone(), redact_value(&name, &value));
        }
        if redacted.len() > MAX_PAYLOAD_FIELDS {
            return Err(ContractError::TooManyFields {
                actual: redacted.len(),
                maximum: MAX_PAYLOAD_FIELDS,
            });
        }
        Ok(Self { fields: redacted })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffEnvelope {
    pub handoff_id: String,
    pub task_id: String,
    pub kind: HandoffKind,
    pub status: HandoffStatus,
    pub from: RoleIdentity,
    pub to: RoleIdentity,
    pub purpose: String,
    pub payload: HandoffPayload,
    pub sequence: u64,
}

impl HandoffEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        handoff_id: impl Into<String>,
        task_id: impl Into<String>,
        kind: HandoffKind,
        from: RoleIdentity,
        to: RoleIdentity,
        purpose: impl Into<String>,
        payload: HandoffPayload,
        sequence: u64,
    ) -> Result<Self, ContractError> {
        let envelope = Self {
            handoff_id: handoff_id.into(),
            task_id: task_id.into(),
            kind,
            status: HandoffStatus::Pending,
            from,
            to,
            purpose: purpose.into(),
            payload,
            sequence,
        };
        envelope.validate()?;
        let bytes = serde_json::to_vec(&envelope).expect("HandoffEnvelope is serializable");
        if bytes.len() > MAX_HANDOFF_BYTES {
            return Err(ContractError::HandoffTooLarge {
                actual: bytes.len(),
                maximum: MAX_HANDOFF_BYTES,
            });
        }
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("handoff_id", &self.handoff_id, MAX_ID_CHARS, true)?;
        validate_text("task_id", &self.task_id, MAX_ID_CHARS, true)?;
        validate_text("purpose", &self.purpose, MAX_PURPOSE_CHARS, true)?;
        self.from.validate()?;
        self.to.validate()?;
        Ok(())
    }

    pub fn to_deterministic_json(&self) -> String {
        serde_json::to_string(self).expect("HandoffEnvelope is serializable")
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), ContractError> {
    if required && value.trim().is_empty() {
        return Err(ContractError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(ContractError::FieldTooLong { field, max });
    }
    Ok(())
}

fn redact_value(name: &str, value: &str) -> String {
    if is_sensitive_name(name) || contains_sensitive_phrase(value) {
        return "[REDACTED]".to_owned();
    }
    value
        .split_inclusive(char::is_whitespace)
        .map(|part| {
            let token = part.trim_end_matches(char::is_whitespace);
            let suffix = &part[token.len()..];
            if is_sensitive_token(token) {
                format!("[REDACTED]{suffix}")
            } else {
                part.to_owned()
            }
        })
        .collect()
}

fn contains_sensitive_phrase(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("bearer ")
        || lower.contains("sk-")
        || lower.contains("ghp_")
        || lower.contains("github_pat_")
        || lower.contains("aiza")
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

fn is_sensitive_token(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("aiza")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload() -> HandoffPayload {
        HandoffPayload::new([
            ("zeta".to_owned(), "last".to_owned()),
            ("alpha".to_owned(), "first".to_owned()),
        ])
        .unwrap()
    }

    #[test]
    fn custom_roles_are_bounded_and_validated() {
        assert!(RoleIdentity::custom("build-review").is_ok());
        assert!(RoleIdentity::custom("bad/role").is_err());
        assert!(RoleIdentity::builtin(ChildRole::Tester).validate().is_ok());
    }

    #[test]
    fn envelope_redacts_secret_fields_and_tokens() {
        let payload = HandoffPayload::new([
            ("api_token".to_owned(), "do-not-leak".to_owned()),
            ("notes".to_owned(), "Bearer abc123".to_owned()),
        ])
        .unwrap();
        assert_eq!(payload.fields["api_token"], "[REDACTED]");
        assert_eq!(payload.fields["notes"], "[REDACTED]");
    }

    #[test]
    fn deterministic_json_sorts_payload_keys() {
        let envelope = HandoffEnvelope::new(
            "h-1",
            "task-1",
            HandoffKind::Delegate,
            RoleIdentity::builtin(ChildRole::Coordinator),
            RoleIdentity::builtin(ChildRole::Implementer),
            "implement slice",
            payload(),
            1,
        )
        .unwrap();
        let json = envelope.to_deterministic_json();
        assert!(json.find("alpha").unwrap() < json.find("zeta").unwrap());
        assert_eq!(json, envelope.to_deterministic_json());
    }

    #[test]
    fn limits_reject_unbounded_input() {
        assert!(HandoffPayload::new(
            (0..=MAX_PAYLOAD_FIELDS)
                .map(|index| { (format!("field-{index}"), "value".to_owned()) })
        )
        .is_err());
        assert!(HandoffEnvelope::new(
            "",
            "task-1",
            HandoffKind::Delegate,
            RoleIdentity::builtin(ChildRole::Planner),
            RoleIdentity::builtin(ChildRole::Tester),
            "purpose",
            payload(),
            0,
        )
        .is_err());
    }

    #[test]
    fn serde_round_trip_preserves_contract() {
        let envelope = HandoffEnvelope::new(
            "h-2",
            "task-2",
            HandoffKind::ReturnResult,
            RoleIdentity::builtin(ChildRole::Implementer),
            RoleIdentity::builtin(ChildRole::Reviewer),
            "review result",
            payload(),
            2,
        )
        .unwrap();
        let restored: HandoffEnvelope =
            serde_json::from_str(&envelope.to_deterministic_json()).unwrap();
        assert_eq!(restored, envelope);
    }
}
