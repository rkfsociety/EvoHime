//! Bounded, redacted observability hook contract.
//!
//! Модуль намеренно не подключён к `lib.rs`: интеграция с runtime и delivery
//! pipeline будет отдельной задачей. Hook только наблюдает снимок контекста и
//! не имеет операции, которая может изменить порядок элементов контекста.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

pub const MAX_PAYLOAD_FIELDS: usize = 32;
pub const MAX_FIELD_NAME_CHARS: usize = 64;
pub const MAX_FIELD_VALUE_CHARS: usize = 512;
pub const MAX_CONTEXT_ITEMS: usize = 128;
pub const MAX_CONTEXT_ITEM_CHARS: usize = 128;
pub const MAX_EVENT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookName {
    BeforeContext,
    BeforeTool,
    AfterTool,
    BeforeCommit,
    AfterTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny,
    Observe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    EmptyId(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    TooManyFields { actual: usize, maximum: usize },
    TooManyContextItems { actual: usize, maximum: usize },
    EventTooLarge { actual: usize, maximum: usize },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyFields { actual, maximum } => {
                write!(f, "payload has {actual} fields, maximum is {maximum}")
            }
            Self::TooManyContextItems { actual, maximum } => {
                write!(f, "context has {actual} items, maximum is {maximum}")
            }
            Self::EventTooLarge { actual, maximum } => {
                write!(f, "event is {actual} bytes, maximum is {maximum}")
            }
        }
    }
}

impl std::error::Error for ContractError {}

/// Payload uses a sorted map so its serialized form is stable across runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct HookPayload {
    #[serde(flatten)]
    pub fields: BTreeMap<String, String>,
}

impl HookPayload {
    pub fn new(fields: impl IntoIterator<Item = (String, String)>) -> Result<Self, ContractError> {
        let mut redacted = BTreeMap::new();
        for (name, value) in fields {
            validate_text("field_name", &name, MAX_FIELD_NAME_CHARS, true)?;
            validate_text("field_value", &value, MAX_FIELD_VALUE_CHARS, false)?;
            let value = if is_sensitive_name(&name) {
                "[REDACTED]".to_owned()
            } else {
                redact_text(&value)
            };
            redacted.insert(name, value);
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

/// Immutable ordered context metadata captured at hook creation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextOrder {
    pub items: Vec<String>,
}

impl ContextOrder {
    pub fn capture(items: impl IntoIterator<Item = String>) -> Result<Self, ContractError> {
        let items: Vec<_> = items.into_iter().collect();
        if items.len() > MAX_CONTEXT_ITEMS {
            return Err(ContractError::TooManyContextItems {
                actual: items.len(),
                maximum: MAX_CONTEXT_ITEMS,
            });
        }
        for item in &items {
            validate_text("context_item", item, MAX_CONTEXT_ITEM_CHARS, true)?;
        }
        Ok(Self { items })
    }

    /// Hooks expose the original sequence read-only; no sorting is performed.
    pub fn as_slice(&self) -> &[String] {
        &self.items
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEvent {
    pub hook: HookName,
    pub event_id: String,
    pub task_id: String,
    pub sequence: u64,
    pub decision: PolicyDecision,
    pub context_order: ContextOrder,
    pub payload: HookPayload,
}

impl HookEvent {
    pub fn new(
        hook: HookName,
        event_id: impl Into<String>,
        task_id: impl Into<String>,
        sequence: u64,
        decision: PolicyDecision,
        context_order: ContextOrder,
        payload: HookPayload,
    ) -> Result<Self, ContractError> {
        let event = Self {
            hook,
            event_id: event_id.into(),
            task_id: task_id.into(),
            sequence,
            decision,
            context_order,
            payload,
        };
        validate_text("event_id", &event.event_id, MAX_FIELD_VALUE_CHARS, true)?;
        validate_text("task_id", &event.task_id, MAX_FIELD_VALUE_CHARS, true)?;
        let json = serde_json::to_vec(&event).expect("HookEvent is serializable");
        if json.len() > MAX_EVENT_BYTES {
            return Err(ContractError::EventTooLarge {
                actual: json.len(),
                maximum: MAX_EVENT_BYTES,
            });
        }
        Ok(event)
    }

    pub fn to_deterministic_json(&self) -> String {
        serde_json::to_string(self).expect("HookEvent is serializable")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookPolicy {
    pub default_decision: PolicyDecision,
    pub denied_hooks: Vec<HookName>,
    pub observed_hooks: Vec<HookName>,
}

impl Default for HookPolicy {
    fn default() -> Self {
        Self {
            default_decision: PolicyDecision::Allow,
            denied_hooks: Vec::new(),
            observed_hooks: Vec::new(),
        }
    }
}

impl HookPolicy {
    pub fn decide(&self, hook: HookName) -> PolicyDecision {
        if self.denied_hooks.contains(&hook) {
            PolicyDecision::Deny
        } else if self.observed_hooks.contains(&hook) {
            PolicyDecision::Observe
        } else {
            self.default_decision
        }
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), ContractError> {
    if required && value.trim().is_empty() {
        return Err(ContractError::EmptyId(field));
    }
    if value.chars().count() > max {
        return Err(ContractError::FieldTooLong { field, max });
    }
    Ok(())
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

fn redact_text(input: &str) -> String {
    input
        .split_inclusive(char::is_whitespace)
        .map(|token| {
            let trimmed = token.trim_end_matches(char::is_whitespace);
            let suffix = &token[trimmed.len()..];
            let lower = trimmed.to_ascii_lowercase();
            let email =
                trimmed.contains('@') && trimmed.split('@').next().is_some_and(|p| !p.is_empty());
            let secret = lower.starts_with("bearer ")
                || lower.starts_with("sk-")
                || lower.starts_with("ghp_")
                || lower.starts_with("github_pat_")
                || lower.starts_with("aiza")
                || email;
            if secret {
                format!("[REDACTED]{suffix}")
            } else {
                token.to_owned()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ContextOrder {
        ContextOrder::capture([
            "system".to_owned(),
            "user:first".to_owned(),
            "assistant".to_owned(),
        ])
        .unwrap()
    }

    fn payload() -> HookPayload {
        HookPayload::new([
            ("command".to_owned(), "cargo test".to_owned()),
            ("authorization".to_owned(), "Bearer secret".to_owned()),
            ("note".to_owned(), "contact alice@example.test".to_owned()),
        ])
        .unwrap()
    }

    #[test]
    fn creates_redacted_bounded_structured_events_for_all_hooks() {
        for hook in [
            HookName::BeforeContext,
            HookName::BeforeTool,
            HookName::AfterTool,
            HookName::BeforeCommit,
            HookName::AfterTask,
        ] {
            let event = HookEvent::new(
                hook,
                "event-1",
                "task-1",
                1,
                PolicyDecision::Observe,
                context(),
                payload(),
            )
            .unwrap();
            assert_eq!(event.payload.fields["authorization"], "[REDACTED]");
            assert_eq!(event.payload.fields["note"], "contact [REDACTED]");
            assert!(event.to_deterministic_json().len() <= MAX_EVENT_BYTES);
        }
    }

    #[test]
    fn deterministic_json_is_stable_and_sorted_by_payload_key() {
        let first = HookPayload::new([
            ("zeta".to_owned(), "z".to_owned()),
            ("alpha".to_owned(), "a".to_owned()),
        ])
        .unwrap();
        let second = HookPayload::new([
            ("alpha".to_owned(), "a".to_owned()),
            ("zeta".to_owned(), "z".to_owned()),
        ])
        .unwrap();
        let left = HookEvent::new(
            HookName::BeforeTool,
            "e",
            "t",
            1,
            PolicyDecision::Allow,
            context(),
            first,
        )
        .unwrap();
        let right = HookEvent::new(
            HookName::BeforeTool,
            "e",
            "t",
            1,
            PolicyDecision::Allow,
            context(),
            second,
        )
        .unwrap();
        assert_eq!(left.to_deterministic_json(), right.to_deterministic_json());
        assert!(left
            .to_deterministic_json()
            .contains("\"alpha\":\"a\",\"zeta\":\"z\""));
    }

    #[test]
    fn policy_returns_deny_then_observe_then_default() {
        let policy = HookPolicy {
            default_decision: PolicyDecision::Allow,
            denied_hooks: vec![HookName::BeforeTool],
            observed_hooks: vec![HookName::AfterTool],
        };
        assert_eq!(policy.decide(HookName::BeforeTool), PolicyDecision::Deny);
        assert_eq!(policy.decide(HookName::AfterTool), PolicyDecision::Observe);
        assert_eq!(policy.decide(HookName::AfterTask), PolicyDecision::Allow);
    }

    #[test]
    fn hook_cannot_reorder_context() {
        let original = context();
        let event = HookEvent::new(
            HookName::BeforeContext,
            "e",
            "t",
            1,
            PolicyDecision::Allow,
            original.clone(),
            HookPayload::default(),
        )
        .unwrap();
        assert_eq!(event.context_order.as_slice(), original.as_slice());
        assert_eq!(
            event.context_order.items,
            ["system", "user:first", "assistant"]
        );
    }

    #[test]
    fn bounds_payload_context_and_event() {
        let fields = (0..=MAX_PAYLOAD_FIELDS)
            .map(|i| (format!("field_{i}"), "value".to_owned()))
            .collect::<Vec<_>>();
        assert!(matches!(
            HookPayload::new(fields),
            Err(ContractError::TooManyFields { .. })
        ));
        let items = (0..=MAX_CONTEXT_ITEMS)
            .map(|i| format!("item_{i}"))
            .collect::<Vec<_>>();
        assert!(matches!(
            ContextOrder::capture(items),
            Err(ContractError::TooManyContextItems { .. })
        ));
        assert!(matches!(
            HookEvent::new(
                HookName::AfterTask,
                "",
                "task",
                1,
                PolicyDecision::Allow,
                context(),
                HookPayload::default(),
            ),
            Err(ContractError::EmptyId("event_id"))
        ));
    }
}
