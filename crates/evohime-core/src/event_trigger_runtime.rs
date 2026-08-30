//! Core-owned bounded Event Trigger Runtime contract and ingress validation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

pub const CONTRACT_VERSION: &str = "event-trigger/v1";
pub const MAX_EVENT_BYTES: usize = 256 * 1024;
pub const MAX_INLINE_PAYLOAD_BYTES: usize = 32 * 1024;
pub const MAX_MAPPING_FIELDS: usize = 32;
pub const MAX_QUEUE_DEPTH: usize = 256;
pub const MAX_DEDUP_ENTRIES: usize = 10_000;
pub const DEDUP_TTL_MS: i64 = 24 * 60 * 60 * 1000;
pub const MAX_EVENTS_PER_MINUTE: u32 = 60;
pub const MAX_CHAIN_DEPTH: u8 = 8;
pub const FINGERPRINT_WINDOW_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    IntegrationWebhook,
    LocalWorkspaceEvent,
    SystemEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerState {
    Draft,
    Connecting,
    Active,
    Paused,
    Broken,
    Revoked,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventOutcome {
    Accepted,
    Pending,
    Coalesced,
    Throttled,
    DroppedWithAudit,
    Dispatched,
    Rejected,
    Unknown,
    DuplicateIgnored,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBinding {
    pub workflow_id: String,
    pub workflow_version: u64,
    pub execution_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerDefinition {
    pub contract_version: String,
    pub trigger_id: String,
    pub owner_scope: String,
    pub source_kind: SourceKind,
    pub event_kind: String,
    pub workflow: WorkflowBinding,
    pub mapping: BTreeMap<String, String>,
    pub state: TriggerState,
    pub content_hash: String,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub contract_version: String,
    pub event_id: String,
    pub trigger_id: String,
    pub source_kind: SourceKind,
    pub event_kind: String,
    pub schema_version: u32,
    pub received_at_ms: i64,
    pub payload: Value,
    pub payload_hash: String,
    pub authenticity: String,
    pub origin: String,
    pub correlation_id: String,
    pub provider_event_key: Option<String>,
    pub chain_depth: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TriggerError {
    InvalidField(&'static str),
    UnsupportedVersion,
    Oversized,
    InvalidAuthenticity,
    InvalidSchema,
    MappingRejected,
    Duplicate,
    StaleVersion,
    RateLimited,
    CircuitOpen,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Admission {
    pub outcome: EventOutcome,
    pub event_id: String,
    pub mapped_input: Option<Value>,
    pub error_code: Option<String>,
}

#[derive(Debug, Default)]
pub struct Runtime {
    dedup: HashMap<String, i64>,
    queue_depth: HashMap<String, usize>,
    recent: HashMap<String, Vec<i64>>,
}

impl Runtime {
    pub fn ingest(
        &mut self,
        definition: &TriggerDefinition,
        envelope: &EventEnvelope,
        now_ms: i64,
    ) -> Result<Admission, TriggerError> {
        validate_definition(definition)?;
        validate_envelope(
            envelope,
            serde_json::to_vec(envelope)
                .map_err(|_| TriggerError::InvalidSchema)?
                .len(),
        )?;
        if definition.state != TriggerState::Active {
            return Ok(Admission {
                outcome: EventOutcome::Rejected,
                event_id: envelope.event_id.clone(),
                mapped_input: None,
                error_code: Some("trigger_not_active".into()),
            });
        }
        if definition.source_kind != envelope.source_kind
            || definition.event_kind != envelope.event_kind
        {
            return Ok(Admission {
                outcome: EventOutcome::Rejected,
                event_id: envelope.event_id.clone(),
                mapped_input: None,
                error_code: Some("source_mismatch".into()),
            });
        }
        self.dedup.retain(|_, expiry| *expiry > now_ms);
        let raw_key = envelope
            .provider_event_key
            .clone()
            .unwrap_or_else(|| envelope.payload_hash.clone());
        let key = format!("{}:{raw_key}", definition.trigger_id);
        if self.dedup.get(&key).is_some_and(|expiry| *expiry > now_ms) {
            return Ok(Admission {
                outcome: EventOutcome::DuplicateIgnored,
                event_id: envelope.event_id.clone(),
                mapped_input: None,
                error_code: Some("duplicate_ignored".into()),
            });
        }
        let recent = self
            .recent
            .entry(definition.trigger_id.clone())
            .or_default();
        recent.retain(|at| *at >= now_ms - 60_000);
        if recent.len() >= MAX_EVENTS_PER_MINUTE as usize {
            return Ok(Admission {
                outcome: EventOutcome::Throttled,
                event_id: envelope.event_id.clone(),
                mapped_input: None,
                error_code: Some("rate_limited".into()),
            });
        }
        let queue = self
            .queue_depth
            .entry(definition.trigger_id.clone())
            .or_default();
        if *queue >= MAX_QUEUE_DEPTH {
            return Ok(Admission {
                outcome: EventOutcome::DroppedWithAudit,
                event_id: envelope.event_id.clone(),
                mapped_input: None,
                error_code: Some("queue_full".into()),
            });
        }
        let mapped = map_input(definition, &envelope.payload)?;
        recent.push(now_ms);
        *queue += 1;
        self.dedup.insert(key, now_ms + DEDUP_TTL_MS);
        Ok(Admission {
            outcome: EventOutcome::Pending,
            event_id: envelope.event_id.clone(),
            mapped_input: Some(mapped),
            error_code: None,
        })
    }

    pub fn complete(&mut self, trigger_id: &str) {
        if let Some(depth) = self.queue_depth.get_mut(trigger_id) {
            *depth = depth.saturating_sub(1);
        }
    }
}

pub fn canonical_hash<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("contract serialization");
    let mut hash = Sha256::new();
    hash.update(bytes);
    hex::encode(hash.finalize())
}

pub fn validate_definition(def: &TriggerDefinition) -> Result<(), TriggerError> {
    if def.contract_version != CONTRACT_VERSION {
        return Err(TriggerError::UnsupportedVersion);
    }
    if def.trigger_id.is_empty() || def.owner_scope.is_empty() || def.event_kind.is_empty() {
        return Err(TriggerError::InvalidField("identity"));
    }
    if def.workflow.workflow_id.is_empty()
        || def.workflow.workflow_version == 0
        || def.workflow.execution_hash.is_empty()
    {
        return Err(TriggerError::InvalidField("workflow_binding"));
    }
    if def.mapping.len() > MAX_MAPPING_FIELDS
        || def
            .mapping
            .keys()
            .any(|k| k.is_empty() || k.starts_with('/'))
    {
        return Err(TriggerError::MappingRejected);
    }
    Ok(())
}

pub fn validate_envelope(envelope: &EventEnvelope, raw_size: usize) -> Result<(), TriggerError> {
    if raw_size > MAX_EVENT_BYTES {
        return Err(TriggerError::Oversized);
    }
    if envelope.contract_version != CONTRACT_VERSION {
        return Err(TriggerError::UnsupportedVersion);
    }
    if envelope.event_id.is_empty()
        || envelope.trigger_id.is_empty()
        || envelope.correlation_id.is_empty()
    {
        return Err(TriggerError::InvalidField("identity"));
    }
    if envelope.chain_depth > MAX_CHAIN_DEPTH {
        return Err(TriggerError::InvalidField("chain_depth"));
    }
    if envelope.authenticity != "core_local" && envelope.authenticity != "verified_signature" {
        return Err(TriggerError::InvalidAuthenticity);
    }
    if envelope.source_kind == SourceKind::IntegrationWebhook
        && envelope.authenticity != "verified_signature"
    {
        return Err(TriggerError::InvalidAuthenticity);
    }
    let payload_size = serde_json::to_vec(&envelope.payload)
        .map_err(|_| TriggerError::InvalidSchema)?
        .len();
    if payload_size > MAX_INLINE_PAYLOAD_BYTES {
        return Err(TriggerError::Oversized);
    }
    Ok(())
}

pub fn map_input(def: &TriggerDefinition, payload: &Value) -> Result<Value, TriggerError> {
    let object = payload.as_object().ok_or(TriggerError::InvalidSchema)?;
    let mut mapped = serde_json::Map::new();
    for (destination, source) in &def.mapping {
        let value = object.get(source).ok_or(TriggerError::MappingRejected)?;
        mapped.insert(destination.clone(), value.clone());
    }
    Ok(Value::Object(mapped))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn definition() -> TriggerDefinition {
        TriggerDefinition {
            contract_version: CONTRACT_VERSION.into(),
            trigger_id: "t".into(),
            owner_scope: "w".into(),
            source_kind: SourceKind::LocalWorkspaceEvent,
            event_kind: "file_changed".into(),
            workflow: WorkflowBinding {
                workflow_id: "wf".into(),
                workflow_version: 1,
                execution_hash: "hash".into(),
            },
            mapping: [("value".into(), "value".into())].into_iter().collect(),
            state: TriggerState::Active,
            content_hash: "hash".into(),
            created_at_ms: 1,
        }
    }
    fn envelope(source: SourceKind, auth: &str) -> EventEnvelope {
        EventEnvelope {
            contract_version: CONTRACT_VERSION.into(),
            event_id: "e".into(),
            trigger_id: "t".into(),
            source_kind: source,
            event_kind: "file_changed".into(),
            schema_version: 1,
            received_at_ms: 1,
            payload: serde_json::json!({"value": 1}),
            payload_hash: "hash".into(),
            authenticity: auth.into(),
            origin: "test".into(),
            correlation_id: "c".into(),
            provider_event_key: None,
            chain_depth: 0,
        }
    }
    #[test]
    fn definition_rejects_latest_binding() {
        let mut d = definition();
        d.workflow.workflow_version = 0;
        assert!(matches!(
            validate_definition(&d),
            Err(TriggerError::InvalidField("workflow_binding"))
        ));
    }
    #[test]
    fn webhook_requires_verified_authenticity() {
        assert!(matches!(
            validate_envelope(&envelope(SourceKind::IntegrationWebhook, "core_local"), 10),
            Err(TriggerError::InvalidAuthenticity)
        ));
    }
    #[test]
    fn mapping_is_allowlisted() {
        let d = definition();
        assert_eq!(
            map_input(&d, &serde_json::json!({"value": 7})).unwrap()["value"],
            7
        );
        assert!(map_input(&d, &serde_json::json!({})).is_err());
    }
    #[test]
    fn oversized_is_rejected_before_processing() {
        let e = envelope(SourceKind::LocalWorkspaceEvent, "core_local");
        assert!(matches!(
            validate_envelope(&e, MAX_EVENT_BYTES + 1),
            Err(TriggerError::Oversized)
        ));
    }
    #[test]
    fn runtime_deduplicates_and_maps_local_event() {
        let d = definition();
        let e = envelope(SourceKind::LocalWorkspaceEvent, "core_local");
        let mut r = Runtime::default();
        assert_eq!(
            r.ingest(&d, &e, 100).unwrap().outcome,
            EventOutcome::Pending
        );
        assert_eq!(
            r.ingest(&d, &e, 101).unwrap().outcome,
            EventOutcome::DuplicateIgnored
        );
    }
    #[test]
    fn runtime_throttles_after_sixty_events() {
        let mut d = definition();
        d.mapping.clear();
        let mut r = Runtime::default();
        for i in 0..60 {
            let mut e = envelope(SourceKind::LocalWorkspaceEvent, "core_local");
            e.event_id = format!("e{i}");
            e.payload_hash = format!("h{i}");
            assert_eq!(
                r.ingest(&d, &e, 100).unwrap().outcome,
                EventOutcome::Pending
            );
        }
        let mut e = envelope(SourceKind::LocalWorkspaceEvent, "core_local");
        e.event_id = "e61".into();
        e.payload_hash = "h61".into();
        assert_eq!(
            r.ingest(&d, &e, 100).unwrap().outcome,
            EventOutcome::Throttled
        );
    }
}
