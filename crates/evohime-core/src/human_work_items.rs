//! Core-owned durable human work items.  A human response is data, never an
//! approval, a capability grant, or an executable identity.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_TEXT_BYTES: usize = 8 * 1024;
pub const MAX_ITEMS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanWorkItemState {
    Draft,
    WaitingForHuman,
    InProgress,
    Submitted,
    Accepted,
    NeedsRevision,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResponseSchema {
    Text,
    Choice { choices: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSlotRef {
    pub session_id: String,
    pub slot_id: String,
    pub protocol_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanWorkItem {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub title: String,
    /// Bounded user-visible instruction, explicitly not a raw model prompt.
    pub instructions: String,
    pub response_schema: ResponseSchema,
    pub state: HumanWorkItemState,
    pub team_slot: Option<TeamSlotRef>,
    pub response: Option<String>,
    pub submitted_by: Option<String>,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HumanWorkItemError {
    Invalid(&'static str),
    UnsupportedVersion(u32),
    Duplicate,
    NotFound,
    Stale,
    IdempotencyConflict,
    InvalidTransition,
    Expired,
    SlotDenied,
    Limit,
}
impl std::fmt::Display for HumanWorkItemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Invalid(v) => v,
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::Duplicate => "duplicate",
            Self::NotFound => "not_found",
            Self::Stale => "stale",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::InvalidTransition => "invalid_transition",
            Self::Expired => "expired",
            Self::SlotDenied => "human_slot_denied",
            Self::Limit => "limit",
        })
    }
}
impl std::error::Error for HumanWorkItemError {}

fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}
fn text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT_BYTES
}

pub fn validate_item(item: &HumanWorkItem) -> Result<(), HumanWorkItemError> {
    if item.schema_version != CONTRACT_VERSION {
        return Err(HumanWorkItemError::UnsupportedVersion(item.schema_version));
    }
    if !id(&item.id) || item.revision == 0 || !text(&item.title) || !text(&item.instructions) {
        return Err(HumanWorkItemError::Invalid("item"));
    }
    match &item.response_schema {
        ResponseSchema::Text => {}
        ResponseSchema::Choice { choices }
            if !choices.is_empty() && choices.len() <= 32 && choices.iter().all(|v| text(v)) => {}
        ResponseSchema::Choice { .. } => {
            return Err(HumanWorkItemError::Invalid("response_schema"))
        }
    }
    if let Some(slot) = &item.team_slot {
        if !id(&slot.session_id) || !id(&slot.slot_id) || slot.protocol_hash.len() != 64 {
            return Err(HumanWorkItemError::Invalid("team_slot"));
        }
    }
    if let Some(value) = &item.response {
        validate_response(&item.response_schema, value)?;
    }
    if item.submitted_by.as_ref().is_some_and(|v| !id(v)) {
        return Err(HumanWorkItemError::Invalid("actor"));
    }
    Ok(())
}
pub fn validate_response(schema: &ResponseSchema, value: &str) -> Result<(), HumanWorkItemError> {
    if !text(value) {
        return Err(HumanWorkItemError::Limit);
    }
    match schema {
        ResponseSchema::Text => Ok(()),
        ResponseSchema::Choice { choices } if choices.iter().any(|choice| choice == value) => {
            Ok(())
        }
        ResponseSchema::Choice { .. } => Err(HumanWorkItemError::Invalid("response_schema")),
    }
}
pub fn canonical_hash(item: &HumanWorkItem) -> Result<String, HumanWorkItemError> {
    validate_item(item)?;
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(item).map_err(|_| HumanWorkItemError::Invalid("serialization"))?,
    )))
}

#[derive(Debug, Default)]
pub struct HumanWorkItemsRegistry {
    pub items: BTreeMap<String, HumanWorkItem>,
    idempotency: BTreeMap<String, String>,
}
impl HumanWorkItemsRegistry {
    pub fn list(&self) -> Vec<HumanWorkItem> {
        self.items.values().cloned().collect()
    }
    pub fn create(
        &mut self,
        item: HumanWorkItem,
        key: &str,
    ) -> Result<HumanWorkItem, HumanWorkItemError> {
        validate_item(&item)?;
        if self.items.len() >= MAX_ITEMS {
            return Err(HumanWorkItemError::Limit);
        }
        let hash = canonical_hash(&item)?;
        if let Some(previous) = self.idempotency.get(key) {
            return if previous == &hash {
                Ok(item)
            } else {
                Err(HumanWorkItemError::IdempotencyConflict)
            };
        }
        if self.items.contains_key(&item.id) {
            return Err(HumanWorkItemError::Duplicate);
        }
        self.idempotency.insert(key.into(), hash);
        self.items.insert(item.id.clone(), item.clone());
        Ok(item)
    }
    pub fn transition(
        &mut self,
        id: &str,
        expected: u64,
        operation: &str,
        response: Option<String>,
        actor: &str,
        now_ms: i64,
    ) -> Result<HumanWorkItem, HumanWorkItemError> {
        let item = self.items.get_mut(id).ok_or(HumanWorkItemError::NotFound)?;
        if item.revision != expected {
            return Err(HumanWorkItemError::Stale);
        }
        if item
            .expires_at_ms
            .is_some_and(|deadline| now_ms >= deadline)
            && !matches!(
                item.state,
                HumanWorkItemState::Accepted
                    | HumanWorkItemState::Cancelled
                    | HumanWorkItemState::Expired
            )
        {
            item.state = HumanWorkItemState::Expired;
            item.revision += 1;
            return Err(HumanWorkItemError::Expired);
        }
        let next = match operation {
            "start" => HumanWorkItemState::InProgress,
            "submit" => HumanWorkItemState::Submitted,
            "accept" => HumanWorkItemState::Accepted,
            "revise" | "return" => HumanWorkItemState::NeedsRevision,
            "cancel" => HumanWorkItemState::Cancelled,
            _ => return Err(HumanWorkItemError::Invalid("unsupported_operation")),
        };
        let valid = matches!(
            (item.state, next),
            (
                HumanWorkItemState::Draft
                    | HumanWorkItemState::WaitingForHuman
                    | HumanWorkItemState::NeedsRevision,
                HumanWorkItemState::InProgress
            ) | (
                HumanWorkItemState::InProgress | HumanWorkItemState::NeedsRevision,
                HumanWorkItemState::Submitted
            ) | (
                HumanWorkItemState::Submitted,
                HumanWorkItemState::Accepted | HumanWorkItemState::NeedsRevision
            ) | (_, HumanWorkItemState::Cancelled)
        );
        if !valid {
            return Err(HumanWorkItemError::InvalidTransition);
        }
        if next == HumanWorkItemState::Submitted {
            let value = response.ok_or(HumanWorkItemError::Invalid("response"))?;
            validate_response(&item.response_schema, &value)?;
            item.response = Some(value);
            item.submitted_by = Some(actor.into());
        }
        item.state = next;
        item.revision += 1;
        Ok(item.clone())
    }
    pub fn transition_idempotent(
        &mut self,
        id: &str,
        expected: u64,
        operation: &str,
        response: Option<String>,
        actor: &str,
        now_ms: i64,
        key: &str,
    ) -> Result<HumanWorkItem, HumanWorkItemError> {
        let fingerprint = format!("{id}:{expected}:{operation}:{response:?}");
        if let Some(previous) = self.idempotency.get(key) {
            return if previous == &fingerprint {
                self.items
                    .get(id)
                    .cloned()
                    .ok_or(HumanWorkItemError::NotFound)
            } else {
                Err(HumanWorkItemError::IdempotencyConflict)
            };
        }
        let result = self.transition(id, expected, operation, response, actor, now_ms)?;
        self.idempotency.insert(key.into(), fingerprint);
        Ok(result)
    }
    pub fn expire_due(&mut self, now_ms: i64) -> Vec<HumanWorkItem> {
        self.items
            .values_mut()
            .filter_map(|item| {
                if item.expires_at_ms.is_some_and(|d| now_ms >= d)
                    && !matches!(
                        item.state,
                        HumanWorkItemState::Accepted
                            | HumanWorkItemState::Cancelled
                            | HumanWorkItemState::Expired
                    )
                {
                    item.state = HumanWorkItemState::Expired;
                    item.revision += 1;
                    Some(item.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn item() -> HumanWorkItem {
        HumanWorkItem {
            schema_version: 1,
            id: "review".into(),
            revision: 1,
            title: "Review".into(),
            instructions: "Check the change".into(),
            response_schema: ResponseSchema::Choice {
                choices: vec!["ok".into(), "fix".into()],
            },
            state: HumanWorkItemState::WaitingForHuman,
            team_slot: None,
            response: None,
            submitted_by: None,
            expires_at_ms: None,
        }
    }
    #[test]
    fn response_is_schema_validated_and_never_an_approval() {
        let mut r = HumanWorkItemsRegistry::default();
        r.create(item(), "create").unwrap();
        let started = r
            .transition("review", 1, "start", None, "shell", 0)
            .unwrap();
        assert!(r
            .transition(
                "review",
                started.revision,
                "submit",
                Some("grant".into()),
                "shell",
                0
            )
            .is_err());
        let submitted = r
            .transition(
                "review",
                started.revision,
                "submit",
                Some("ok".into()),
                "shell",
                0,
            )
            .unwrap();
        assert_eq!(submitted.submitted_by.as_deref(), Some("shell"));
        assert_eq!(submitted.state, HumanWorkItemState::Submitted);
    }
    #[test]
    fn expiry_is_fail_closed() {
        let mut r = HumanWorkItemsRegistry::default();
        let mut i = item();
        i.expires_at_ms = Some(1);
        r.create(i, "c").unwrap();
        assert_eq!(r.expire_due(1)[0].state, HumanWorkItemState::Expired);
    }
    #[test]
    fn repeated_transition_key_returns_the_same_transition() {
        let mut r = HumanWorkItemsRegistry::default();
        r.create(item(), "create").unwrap();
        let first = r
            .transition_idempotent("review", 1, "start", None, "shell", 0, "start-key")
            .unwrap();
        let repeated = r
            .transition_idempotent("review", 1, "start", None, "shell", 0, "start-key")
            .unwrap();
        assert_eq!(first, repeated);
    }
}
