//! Bounded contract for child roles and task handoff.
//!
//! The contract is deliberately standalone: runtime wiring and `lib.rs`
//! integration belong to a later task. Handoffs are immutable, redacted at
//! construction time, and serialize deterministically for tracing and replay.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

/// Maximum length of handoff and task identifiers.
pub const MAX_ID_CHARS: usize = 128;
/// Maximum length of a custom child-role name.
pub const MAX_ROLE_NAME_CHARS: usize = 64;
/// Maximum length of a handoff purpose description.
pub const MAX_PURPOSE_CHARS: usize = 512;
/// Maximum number of fields retained in a handoff payload.
pub const MAX_PAYLOAD_FIELDS: usize = 32;
/// Maximum length of one payload field name.
pub const MAX_FIELD_NAME_CHARS: usize = 64;
/// Maximum length of one payload field value.
pub const MAX_FIELD_VALUE_CHARS: usize = 2_048;
/// Maximum serialized handoff envelope size in bytes.
pub const MAX_HANDOFF_BYTES: usize = 32 * 1024;

/// Built-in or custom role assigned to a child task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildRole {
    /// Coordinates child work without directly implementing it.
    Coordinator,
    /// Gathers and summarizes bounded evidence.
    Researcher,
    /// Produces a plan without applying changes.
    Planner,
    /// Implements an authorized change.
    Implementer,
    /// Reviews a result against requirements.
    Reviewer,
    /// Runs authorized verification steps.
    Tester,
    /// Role with a bounded custom display name.
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
            "workspace.write",
            "git.status",
            "git.diff",
        ],
        ChildRole::Tester => &[
            "workspace.read",
            "workspace.search",
            "git.status",
            "git.diff",
            "test.execute",
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

/// Returns whether the advisory role matrix includes the requested capability.
pub fn can_request_capability(role: ChildRole, capability: &str) -> bool {
    allowed_capabilities(role).contains(&capability)
}

/// Purpose of a transfer between child roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffKind {
    /// Delegate ownership of work to another role.
    Delegate,
    /// Return a result to the requesting role.
    ReturnResult,
    /// Ask another role to review an artifact.
    RequestReview,
    /// Ask another role to retry a bounded operation.
    RequestRetry,
}

/// Lifecycle state of a child-role handoff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandoffStatus {
    /// Handoff was created and awaits a response.
    Pending,
    /// Recipient accepted the handoff.
    Accepted,
    /// Recipient rejected the handoff.
    Rejected,
    /// Handoff work was completed.
    Completed,
}

/// Validation or size failure while constructing a child-role handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// Required text field is empty.
    EmptyField(&'static str),
    /// Text exceeds the field's declared character limit.
    FieldTooLong {
        /// Name of the field that exceeded its limit.
        field: &'static str,
        /// Maximum permitted character count.
        max: usize,
    },
    /// Deterministic envelope serialization failed.
    Serialization(String),
    /// Payload has more fields than allowed.
    TooManyFields {
        /// Number of fields supplied by the caller.
        actual: usize,
        /// Maximum number of accepted payload fields.
        maximum: usize,
    },
    /// Custom role label contains disallowed characters.
    InvalidRoleName,
    /// Serialized handoff exceeds the envelope byte limit.
    HandoffTooLarge {
        /// Serialized byte length of the handoff.
        actual: usize,
        /// Maximum serialized byte length.
        maximum: usize,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::Serialization(error) => write!(f, "handoff serialization failed: {error}"),
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
    /// Built-in role category.
    pub role: ChildRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional bounded name required for a custom role.
    pub name: Option<String>,
}

impl RoleIdentity {
    /// Creates an identity using a built-in role.
    pub fn builtin(role: ChildRole) -> Self {
        Self { role, name: None }
    }

    /// Creates a custom role identity after validating its display name.
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
    /// Sorted, redacted payload fields.
    pub fields: BTreeMap<String, String>,
}

impl HandoffPayload {
    /// Validates and redacts fields before constructing a deterministic payload.
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

/// Bounded ownership-transfer envelope between child roles.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffEnvelope {
    /// Stable handoff identifier.
    pub handoff_id: String,
    /// Parent task identifier.
    pub task_id: String,
    /// Transfer purpose, such as delegation or review.
    pub kind: HandoffKind,
    /// Current handoff lifecycle state.
    pub status: HandoffStatus,
    /// Sending role identity.
    pub from: RoleIdentity,
    /// Receiving role identity.
    pub to: RoleIdentity,
    /// Human-readable reason for the transfer.
    pub purpose: String,
    /// Bounded and redacted handoff data.
    pub payload: HandoffPayload,
    /// Monotonic sequence within the parent task.
    pub sequence: u64,
}

impl HandoffEnvelope {
    /// Validates and creates an envelope from its complete input object.
    pub fn new(input: HandoffEnvelopeInput) -> Result<Self, ContractError> {
        let envelope = Self {
            handoff_id: input.handoff_id,
            task_id: input.task_id,
            kind: input.kind,
            status: HandoffStatus::Pending,
            from: input.from,
            to: input.to,
            purpose: input.purpose,
            payload: input.payload,
            sequence: input.sequence,
        };
        envelope.validate()?;
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| ContractError::Serialization(error.to_string()))?;
        if bytes.len() > MAX_HANDOFF_BYTES {
            return Err(ContractError::HandoffTooLarge {
                actual: bytes.len(),
                maximum: MAX_HANDOFF_BYTES,
            });
        }
        Ok(envelope)
    }

    /// Checks envelope identifiers, purpose, and both role identities.
    pub fn validate(&self) -> Result<(), ContractError> {
        validate_text("handoff_id", &self.handoff_id, MAX_ID_CHARS, true)?;
        validate_text("task_id", &self.task_id, MAX_ID_CHARS, true)?;
        validate_text("purpose", &self.purpose, MAX_PURPOSE_CHARS, true)?;
        self.from.validate()?;
        self.to.validate()?;
        Ok(())
    }

    /// Serializes the envelope with deterministic field ordering.
    pub fn to_deterministic_json(&self) -> Result<String, ContractError> {
        serde_json::to_string(self).map_err(|error| ContractError::Serialization(error.to_string()))
    }
}

/// Полный набор полей передачи между дочерними ролями. Единый объект не даёт
/// перепутать идентичность отправителя, получателя и полезную нагрузку.
pub struct HandoffEnvelopeInput {
    /// Stable handoff identifier.
    pub handoff_id: String,
    /// Parent task identifier.
    pub task_id: String,
    /// Purpose of the ownership transfer.
    pub kind: HandoffKind,
    /// Sending role identity.
    pub from: RoleIdentity,
    /// Receiving role identity.
    pub to: RoleIdentity,
    /// Human-readable transfer objective.
    pub purpose: String,
    /// Redacted payload transferred to the receiver.
    pub payload: HandoffPayload,
    /// Monotonic sequence within the parent task.
    pub sequence: u64,
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
        let envelope = HandoffEnvelope::new(HandoffEnvelopeInput {
            handoff_id: "h-1".into(),
            task_id: "task-1".into(),
            kind: HandoffKind::Delegate,
            from: RoleIdentity::builtin(ChildRole::Coordinator),
            to: RoleIdentity::builtin(ChildRole::Implementer),
            purpose: "implement slice".into(),
            payload: payload(),
            sequence: 1,
        })
        .unwrap();
        let json = envelope.to_deterministic_json().unwrap();
        assert!(json.find("alpha").unwrap() < json.find("zeta").unwrap());
        assert_eq!(json, envelope.to_deterministic_json().unwrap());
    }

    #[test]
    fn limits_reject_unbounded_input() {
        assert!(HandoffPayload::new(
            (0..=MAX_PAYLOAD_FIELDS)
                .map(|index| { (format!("field-{index}"), "value".to_owned()) })
        )
        .is_err());
        assert!(HandoffEnvelope::new(HandoffEnvelopeInput {
            handoff_id: String::new(),
            task_id: "task-1".into(),
            kind: HandoffKind::Delegate,
            from: RoleIdentity::builtin(ChildRole::Planner),
            to: RoleIdentity::builtin(ChildRole::Tester),
            purpose: "purpose".into(),
            payload: payload(),
            sequence: 0,
        })
        .is_err());
    }

    #[test]
    fn serde_round_trip_preserves_contract() {
        let envelope = HandoffEnvelope::new(HandoffEnvelopeInput {
            handoff_id: "h-2".into(),
            task_id: "task-2".into(),
            kind: HandoffKind::ReturnResult,
            from: RoleIdentity::builtin(ChildRole::Implementer),
            to: RoleIdentity::builtin(ChildRole::Reviewer),
            purpose: "review result".into(),
            payload: payload(),
            sequence: 2,
        })
        .unwrap();
        let restored: HandoffEnvelope =
            serde_json::from_str(&envelope.to_deterministic_json().unwrap()).unwrap();
        assert_eq!(restored, envelope);
    }
}
