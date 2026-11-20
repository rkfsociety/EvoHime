//! Typed, bounded boundary between EvaPulse and the Hive.
//!
//! EvaPulse may publish an external signal, but it does not receive Hive
//! capabilities or create a Hive task directly. The envelope deliberately
//! contains metadata only; the Hive decides whether a signal becomes work.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::{CoreCommand, EventJournal, TaskCoordinator};

/// Version of the EvaPulse-to-Hive envelope contract.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum UTF-8 payload carried by one signal.
pub const MAX_PAYLOAD_BYTES: usize = 16 * 1024;
/// Maximum accepted lifetime of a signal.
pub const MAX_TTL_MS: i64 = 24 * 60 * 60 * 1_000;
/// Maximum length of an envelope identity field.
pub const MAX_ID_BYTES: usize = 128;
/// Maximum number of idempotency keys retained by one live receiver.
pub const MAX_SEEN_IDEMPOTENCY_KEYS: usize = 512;
/// Maximum number of durable acceptance/dead-letter records retained per kind.
pub const MAX_DURABLE_SIGNAL_RECORDS: usize = 512;

/// Provenance of a Hive task. This is metadata, not an authorization grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiveTaskOrigin {
    /// A task created by the normal Hive UI or internal workflow.
    HiveInternal,
    /// A task accepted by the Hive after an EvaPulse signal.
    EvaPulse,
}

/// Supported external signal kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvaPulseSignalKind {
    /// A bounded observation that may deserve Queen review.
    Observation,
    /// A request to review a bounded recovery condition.
    RecoveryReview,
    /// A request to review a bounded evolution candidate.
    EvolutionReview,
}

/// Result of the Hive boundary's decision about a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HiveSignalOutcome {
    /// The Hive accepted the signal and owns any resulting task.
    Accepted,
    /// The Hive declined the signal without creating work.
    Rejected,
    /// The Hive retained the signal for a later decision.
    Deferred,
    /// The boundary could not process the signal.
    Failed,
}

/// A bounded message sent from EvaPulse to the Hive boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvaPulseSignal {
    /// Contract version.
    pub schema_version: u32,
    /// Globally unique signal identifier used for deduplication.
    pub message_id: String,
    /// Correlates retries and the eventual Hive decision.
    pub correlation_id: String,
    /// Stable source identity; never a capability token.
    pub source: String,
    /// Stable idempotency key for delivery fencing.
    pub idempotency_key: String,
    /// Signal classification.
    pub kind: EvaPulseSignalKind,
    /// Creation time in Unix milliseconds.
    pub created_at_ms: i64,
    /// Expiration time in Unix milliseconds.
    pub expires_at_ms: i64,
    /// Redacted, bounded metadata payload.
    pub payload: Vec<u8>,
}

/// A typed response from the Hive boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiveSignalResult {
    /// Contract version.
    pub schema_version: u32,
    /// Signal identifier being answered.
    pub message_id: String,
    /// Correlation copied from the signal.
    pub correlation_id: String,
    /// Hive's decision.
    pub outcome: HiveSignalOutcome,
    /// Provenance of a created Hive task, when one exists.
    pub task_origin: Option<HiveTaskOrigin>,
    /// Redacted bounded reason code.
    pub reason: String,
}

/// Validation failures at the EvaPulse/Hive boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryError {
    /// A required identity is missing or too long.
    InvalidIdentity,
    /// The contract version is not supported.
    UnsupportedVersion,
    /// The payload or serialized envelope exceeds its bound.
    PayloadTooLarge,
    /// The expiry is invalid or exceeds the maximum TTL.
    InvalidTtl,
    /// The signal is already expired at the supplied clock time.
    Expired,
}

impl std::fmt::Display for BoundaryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIdentity => "invalid_identity",
            Self::UnsupportedVersion => "unsupported_version",
            Self::PayloadTooLarge => "payload_too_large",
            Self::InvalidTtl => "invalid_ttl",
            Self::Expired => "expired",
        })
    }
}

impl std::error::Error for BoundaryError {}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && value.is_ascii()
}

/// Validates an incoming signal against contract, size, and TTL limits.
pub fn validate_signal(signal: &EvaPulseSignal, now_ms: i64) -> Result<(), BoundaryError> {
    if signal.schema_version != CONTRACT_VERSION {
        return Err(BoundaryError::UnsupportedVersion);
    }
    if !valid_id(&signal.message_id)
        || !valid_id(&signal.correlation_id)
        || !valid_id(&signal.source)
        || !valid_id(&signal.idempotency_key)
    {
        return Err(BoundaryError::InvalidIdentity);
    }
    if signal.payload.len() > MAX_PAYLOAD_BYTES {
        return Err(BoundaryError::PayloadTooLarge);
    }
    let ttl = signal.expires_at_ms.saturating_sub(signal.created_at_ms);
    if ttl <= 0 || ttl > MAX_TTL_MS || signal.created_at_ms > now_ms {
        return Err(BoundaryError::InvalidTtl);
    }
    if signal.expires_at_ms <= now_ms {
        return Err(BoundaryError::Expired);
    }
    Ok(())
}

/// Validates that a result cannot grant capabilities through its provenance.
pub fn validate_result(result: &HiveSignalResult) -> Result<(), BoundaryError> {
    if result.schema_version != CONTRACT_VERSION
        || !valid_id(&result.message_id)
        || !valid_id(&result.correlation_id)
        || !valid_id(&result.reason)
    {
        return Err(BoundaryError::InvalidIdentity);
    }
    if result.outcome != HiveSignalOutcome::Accepted && result.task_origin.is_some() {
        return Err(BoundaryError::InvalidIdentity);
    }
    if result.outcome == HiveSignalOutcome::Accepted
        && result.task_origin != Some(HiveTaskOrigin::EvaPulse)
    {
        return Err(BoundaryError::InvalidIdentity);
    }
    Ok(())
}

/// Core-owned receiver that turns an accepted signal into a normal Hive task.
///
/// EvaPulse does not get access to the coordinator through this type's public
/// contract: it supplies only a validated signal, prompt, and already-resolved
/// route. The receiver remains the owner of the decision-to-task transition.
#[derive(Clone)]
pub struct HiveSignalReceiver {
    coordinator: TaskCoordinator,
    seen: Arc<Mutex<HashMap<String, String>>>,
    journal: Option<EventJournal>,
}

impl HiveSignalReceiver {
    /// Creates a receiver backed by the existing Core coordinator.
    pub fn new(coordinator: TaskCoordinator) -> Self {
        Self {
            coordinator,
            seen: Arc::new(Mutex::new(HashMap::new())),
            journal: None,
        }
    }

    /// Attaches the Core journal used to recover accepted signals after restart.
    pub fn with_journal(mut self, journal: EventJournal) -> Self {
        self.journal = Some(journal);
        self
    }

    /// Accepts a signal and creates one Hive-owned task after validation.
    pub(crate) async fn accept(
        &self,
        signal: EvaPulseSignal,
        prompt: String,
        preferred_route_hint: String,
    ) -> Result<String, String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_millis()).ok())
            .unwrap_or(i64::MAX);
        if let Err(error) = validate_signal(&signal, now_ms) {
            let reason = format!("hive_signal_rejected:{error}");
            self.persist_dead_letter(&signal, &reason).await;
            return Err(reason);
        }
        let result = HiveSignalResult {
            schema_version: CONTRACT_VERSION,
            message_id: signal.message_id.clone(),
            correlation_id: signal.correlation_id.clone(),
            outcome: HiveSignalOutcome::Accepted,
            task_origin: Some(HiveTaskOrigin::EvaPulse),
            reason: "accepted".to_owned(),
        };
        validate_result(&result).map_err(|error| format!("hive_result_rejected:{error}"))?;
        let idempotency_key = signal.idempotency_key.clone();
        if let Some(journal) = self.journal.as_ref() {
            let latest = journal.latest_sequence().await;
            let records = journal
                .replay(
                    latest.saturating_sub(MAX_SEEN_IDEMPOTENCY_KEYS as i64),
                    MAX_SEEN_IDEMPOTENCY_KEYS,
                )
                .await;
            let records = match records {
                Ok(records) => records,
                Err(error) => {
                    let reason = format!("hive_signal_replay_failed:{error}");
                    self.persist_dead_letter(&signal, &reason).await;
                    return Err(reason);
                }
            };
            for record in records
                .iter()
                .filter(|record| record.event_type == "eva_pulse.signal.accepted")
            {
                let Ok(stored) = serde_json::from_slice::<DurableAcceptance>(&record.payload)
                else {
                    continue;
                };
                if stored.idempotency_key == idempotency_key {
                    let mut seen = self
                        .seen
                        .lock()
                        .map_err(|_| "hive_signal_dedup_lock_poisoned".to_owned())?;
                    seen.insert(idempotency_key.clone(), stored.task_id.clone());
                    return Ok(stored.task_id);
                }
            }
        }
        let task_id = uuid::Uuid::new_v4().to_string();
        {
            let mut seen = self
                .seen
                .lock()
                .map_err(|_| "hive_signal_dedup_lock_poisoned".to_owned())?;
            if let Some(existing_task_id) = seen.get(&idempotency_key) {
                return Ok(existing_task_id.clone());
            }
            if seen.len() >= MAX_SEEN_IDEMPOTENCY_KEYS {
                if let Some(evicted_key) = seen.keys().next().cloned() {
                    seen.remove(&evicted_key);
                }
            }
            seen.insert(idempotency_key.clone(), task_id.clone());
        }
        let dispatch_result = self
            .coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: task_id.clone(),
                prompt,
                workspace_root: None,
                preferred_route_hint: Some(preferred_route_hint),
                conversation: None,
                goal_id: None,
                session_binding: None,
            })
            .await
            .map_err(|error| error.to_string());
        if let Err(error) = dispatch_result {
            if let Ok(mut seen) = self.seen.lock() {
                seen.remove(&idempotency_key);
            }
            let reason = "hive_task_dispatch_failed";
            self.persist_dead_letter(&signal, reason).await;
            return Err(error);
        }
        if let Some(journal) = self.journal.as_ref() {
            let payload = serde_json::to_vec(&DurableAcceptance {
                idempotency_key,
                task_id: task_id.clone(),
                message_id: signal.message_id,
                correlation_id: signal.correlation_id,
            })
            .map_err(|error| format!("hive_signal_encode_failed:{error}"))?;
            journal
                .record_audit(&task_id, "eva_pulse.signal.accepted", &payload)
                .await
                .map_err(|error| format!("hive_signal_persist_failed:{error}"))?;
            self.retain_signal_records(journal).await;
        }
        Ok(task_id)
    }

    async fn persist_dead_letter(&self, signal: &EvaPulseSignal, reason: &str) {
        let Some(journal) = self.journal.as_ref() else {
            return;
        };
        let Ok(payload) = serde_json::to_vec(&DurableDeadLetter {
            message_id: bounded_identity(&signal.message_id),
            correlation_id: bounded_identity(&signal.correlation_id),
            idempotency_key: bounded_identity(&signal.idempotency_key),
            reason: bounded_reason(reason),
        }) else {
            return;
        };
        let _ = journal
            .record_audit(
                "eva-pulse-dead-letter",
                "eva_pulse.signal.dead_letter",
                &payload,
            )
            .await;
        self.retain_signal_records(journal).await;
    }

    async fn retain_signal_records(&self, journal: &EventJournal) {
        let Ok(database) = journal.database.try_lock() else {
            return;
        };
        let limit = MAX_DURABLE_SIGNAL_RECORDS as i64;
        let _ = database.connection().execute(
            "DELETE FROM events
             WHERE event_type IN ('eva_pulse.signal.accepted', 'eva_pulse.signal.dead_letter')
               AND sequence_id NOT IN (
                   SELECT sequence_id FROM events
                   WHERE event_type = events.event_type
                   ORDER BY sequence_id DESC LIMIT ?1
               )",
            [limit],
        );
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DurableAcceptance {
    idempotency_key: String,
    task_id: String,
    message_id: String,
    correlation_id: String,
}

#[derive(Debug, Serialize)]
struct DurableDeadLetter {
    message_id: String,
    correlation_id: String,
    idempotency_key: String,
    reason: String,
}

fn bounded_identity(value: &str) -> String {
    value.chars().take(MAX_ID_BYTES).collect()
}

fn bounded_reason(value: &str) -> String {
    value.chars().take(MAX_ID_BYTES).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal() -> EvaPulseSignal {
        EvaPulseSignal {
            schema_version: CONTRACT_VERSION,
            message_id: "pulse-1".into(),
            correlation_id: "corr-1".into(),
            source: "eva-pulse".into(),
            idempotency_key: "pulse-1".into(),
            kind: EvaPulseSignalKind::Observation,
            created_at_ms: 1_000,
            expires_at_ms: 10_000,
            payload: b"bounded observation".to_vec(),
        }
    }

    #[test]
    fn validates_round_trip_and_rejects_expiry() {
        let original = signal();
        validate_signal(&original, 2_000).unwrap();
        let encoded = serde_json::to_vec(&original).unwrap();
        let decoded: EvaPulseSignal = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, original);
        assert_eq!(
            validate_signal(&decoded, 10_000),
            Err(BoundaryError::Expired)
        );
    }

    #[test]
    fn rejects_unknown_version_and_oversized_payload() {
        let mut unknown = signal();
        unknown.schema_version += 1;
        assert_eq!(
            validate_signal(&unknown, 2_000),
            Err(BoundaryError::UnsupportedVersion)
        );
        let mut oversized = signal();
        oversized.payload = vec![0; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(
            validate_signal(&oversized, 2_000),
            Err(BoundaryError::PayloadTooLarge)
        );
    }

    #[test]
    fn rejects_signal_that_expired_before_receiver_acceptance() {
        let mut expired = signal();
        expired.created_at_ms = 1;
        expired.expires_at_ms = 2;
        assert_eq!(validate_signal(&expired, 3), Err(BoundaryError::Expired));
    }

    #[test]
    fn accepted_result_requires_eva_pulse_provenance() {
        let result = HiveSignalResult {
            schema_version: CONTRACT_VERSION,
            message_id: "pulse-1".into(),
            correlation_id: "corr-1".into(),
            outcome: HiveSignalOutcome::Accepted,
            task_origin: Some(HiveTaskOrigin::EvaPulse),
            reason: "accepted".into(),
        };
        validate_result(&result).unwrap();
        let mut forged = result;
        forged.task_origin = Some(HiveTaskOrigin::HiveInternal);
        assert_eq!(
            validate_result(&forged),
            Err(BoundaryError::InvalidIdentity)
        );
    }
}
