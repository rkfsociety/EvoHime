//! Local Core-owned typed topic/subscription bus (plan 72).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current wire schema version for topic events.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum length of a topic, subscriber, and schema identifier.
pub const MAX_TEXT: usize = 128;
/// Maximum serialized event payload size in bytes.
pub const MAX_PAYLOAD: usize = 64 * 1024;
/// Maximum number of subscriptions retained by the bus.
pub const MAX_SUBSCRIPTIONS: usize = 256;
/// Maximum number of deliveries concurrently in flight.
pub const MAX_IN_FLIGHT: usize = 1024;
/// Maximum delivery attempts before dead-lettering.
pub const MAX_RETRIES: u32 = 3;
/// Typed event topic and optional partition identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Topic {
    /// Namespace used to group related topics.
    pub namespace: String,
    /// Topic name within the namespace.
    pub name: String,
    /// Optional key that preserves per-entity ordering.
    pub partition_key: Option<String>,
}
/// Rule selecting which events a subscription receives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Selector {
    /// Matches one complete topic identity.
    Exact(Topic),
    /// Matches topics whose namespace begins with this prefix.
    NamespacePrefix(String),
    /// Matches events with this schema name.
    Type(String),
}
/// Persistence guarantee requested for event delivery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Delivery {
    /// Delivery exists only while the current process is active.
    Ephemeral,
    /// Delivery is persisted for recovery and retry.
    Durable,
}
/// Ordering guarantee applied while dispatching events.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Ordering {
    /// Preserve event order within a partition key.
    Partition,
    /// Preserve order for this subscription.
    Subscription,
    /// Do not require a delivery ordering guarantee.
    None,
}
/// Retry limit for one failed event delivery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Maximum delivery attempts before the event is dead-lettered.
    pub max_attempts: u32,
}
/// Subscriber identity, event selector, delivery guarantees, and permission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subscription {
    /// Stable subscription identifier.
    pub id: String,
    /// Stable identity of the receiving component.
    pub subscriber: String,
    /// Event selection rule.
    pub selector: Selector,
    /// Persistence guarantee requested by the subscriber.
    pub delivery: Delivery,
    /// Required event ordering guarantee.
    pub ordering: Ordering,
    /// Bounded retry policy.
    pub retry: RetryPolicy,
    /// Capability required to register or use the subscription.
    pub capability: String,
}
/// Integrity-bound event envelope published on a typed topic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    /// Stable event identifier.
    pub event_id: String,
    /// Topic to which the event is published.
    pub topic: Topic,
    /// Payload schema identifier.
    pub schema: String,
    /// Payload schema version.
    pub schema_version: u32,
    /// Component that produced the event.
    pub producer: String,
    /// Optional workflow run associated with the event.
    pub workflow_run_id: Option<String>,
    /// Optional parent goal associated with the event.
    pub goal_id: Option<String>,
    /// Correlation identifier linking related events.
    pub correlation_id: String,
    /// Optional identifier of the event that caused this event.
    pub causation_id: Option<String>,
    /// Unix timestamp in milliseconds when the event was created.
    pub created_at_ms: i64,
    /// Typed event payload.
    pub payload: serde_json::Value,
    /// Digest of the event envelope with this field cleared.
    pub content_hash: String,
}
/// State of one event delivery to a subscriber.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeliveryState {
    /// Event is waiting to be dispatched.
    Queued,
    /// Subscriber is processing the event.
    InFlight,
    /// Subscriber acknowledged successful delivery.
    Acked,
    /// Retry budget was exhausted and delivery was isolated.
    DeadLetter,
    /// Prior process outcome is uncertain and requires reconciliation.
    Unknown,
}
/// Invalid event, version, bounds, permission, or delivery transition.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Input schema version is unsupported.
    #[error("unsupported bus schema version")]
    UnsupportedVersion,
    /// Serialized payload exceeds the supported size.
    #[error("event bus input exceeds bounds")]
    TooLarge,
    /// Topic, event, or content hash violates the contract.
    #[error("invalid event bus value: {0}")]
    Invalid(String),
    /// Subscriber lacks the required capability.
    #[error("capability denied")]
    CapabilityDenied,
    /// Action is not valid from the current delivery state.
    #[error("invalid delivery transition")]
    InvalidTransition,
}
fn text(v: &str) -> Result<(), Error> {
    if v.is_empty() || v.len() > MAX_TEXT || v.chars().any(|c| c.is_control()) {
        Err(Error::Invalid("bounded text".into()))
    } else {
        Ok(())
    }
}
/// Validates subscriber identity, retry bounds, selector, and capability.
pub fn validate_subscription(s: &Subscription) -> Result<(), Error> {
    text(&s.id)?;
    text(&s.subscriber)?;
    text(&s.capability)?;
    if s.retry.max_attempts == 0 || s.retry.max_attempts > MAX_RETRIES {
        return Err(Error::Invalid("retry policy".into()));
    }
    match &s.selector {
        Selector::Exact(t) => validate_topic(t)?,
        Selector::NamespacePrefix(v) => text(v)?,
        Selector::Type(v) => text(v)?,
    }
    Ok(())
}
/// Validates namespace, name, and optional partition key.
pub fn validate_topic(t: &Topic) -> Result<(), Error> {
    text(&t.namespace)?;
    text(&t.name)?;
    if let Some(v) = &t.partition_key {
        text(v)?;
    }
    Ok(())
}
/// Validates event identity, payload size, version, and integrity digest.
pub fn validate_event(e: &Event) -> Result<(), Error> {
    text(&e.event_id)?;
    validate_topic(&e.topic)?;
    text(&e.schema)?;
    text(&e.producer)?;
    text(&e.correlation_id)?;
    if e.schema_version != SCHEMA_VERSION {
        return Err(Error::UnsupportedVersion);
    };
    if serde_json::to_vec(&e.payload)
        .map_err(|_| Error::TooLarge)?
        .len()
        > MAX_PAYLOAD
    {
        return Err(Error::TooLarge);
    };
    let mut c = e.clone();
    c.content_hash.clear();
    if e.content_hash != hash(&c)? {
        return Err(Error::Invalid("content hash".into()));
    }
    Ok(())
}
/// Serializes a value and computes its namespaced SHA-256 digest.
pub fn hash<T: Serialize>(v: &T) -> Result<String, Error> {
    let b = serde_json::to_vec(v).map_err(|_| Error::TooLarge)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(b))))
}
/// Returns whether the event satisfies a subscription selector.
pub fn matches(s: &Selector, e: &Event) -> bool {
    match s {
        Selector::Exact(t) => t == &e.topic,
        Selector::NamespacePrefix(v) => e.topic.namespace.starts_with(v),
        Selector::Type(v) => e.schema == *v,
    }
}
/// Computes the next delivery state for a dispatch, ack, nack, or reconcile action.
pub fn transition(
    state: DeliveryState,
    action: &str,
    attempt: u32,
) -> Result<DeliveryState, Error> {
    match (action, state) {
        ("dispatch", DeliveryState::Queued) => Ok(DeliveryState::InFlight),
        ("ack", DeliveryState::InFlight) => Ok(DeliveryState::Acked),
        ("nack", DeliveryState::InFlight) if attempt < MAX_RETRIES => Ok(DeliveryState::Queued),
        ("nack", DeliveryState::InFlight) => Ok(DeliveryState::DeadLetter),
        ("reconcile", DeliveryState::InFlight) => Ok(DeliveryState::Unknown),
        _ => Err(Error::InvalidTransition),
    }
}
/// Requires an exact matching grant for the requested bus capability.
pub fn authorize(required: &str, grants: &[String]) -> Result<(), Error> {
    if grants.iter().any(|g| g == required) {
        Ok(())
    } else {
        Err(Error::CapabilityDenied)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn topic() -> Topic {
        Topic {
            namespace: "workflow".into(),
            name: "done".into(),
            partition_key: Some("p".into()),
        }
    }
    fn event() -> Event {
        let mut e = Event {
            event_id: "e".into(),
            topic: topic(),
            schema: "workflow.done".into(),
            schema_version: 1,
            producer: "core".into(),
            workflow_run_id: None,
            goal_id: None,
            correlation_id: "c".into(),
            causation_id: None,
            created_at_ms: 1,
            payload: serde_json::json!({"ok":true}),
            content_hash: String::new(),
        };
        let mut c = e.clone();
        c.content_hash.clear();
        e.content_hash = hash(&c).unwrap();
        e
    }
    #[test]
    fn selectors_and_delivery() {
        let e = event();
        assert!(matches(&Selector::NamespacePrefix("work".into()), &e));
        assert_eq!(
            transition(DeliveryState::InFlight, "nack", 3).unwrap(),
            DeliveryState::DeadLetter
        )
    }
    #[test]
    fn rejects_unauthorized() {
        assert_eq!(authorize("events.read", &[]), Err(Error::CapabilityDenied));
        assert!(validate_event(&event()).is_ok())
    }
}
