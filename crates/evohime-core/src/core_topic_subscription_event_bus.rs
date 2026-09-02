//! Local Core-owned typed topic/subscription bus (plan 72).
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_TEXT: usize = 128;
pub const MAX_PAYLOAD: usize = 64 * 1024;
pub const MAX_SUBSCRIPTIONS: usize = 256;
pub const MAX_IN_FLIGHT: usize = 1024;
pub const MAX_RETRIES: u32 = 3;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Topic {
    pub namespace: String,
    pub name: String,
    pub partition_key: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Selector {
    Exact(Topic),
    NamespacePrefix(String),
    Type(String),
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Delivery {
    Ephemeral,
    Durable,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Ordering {
    Partition,
    Subscription,
    None,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subscription {
    pub id: String,
    pub subscriber: String,
    pub selector: Selector,
    pub delivery: Delivery,
    pub ordering: Ordering,
    pub retry: RetryPolicy,
    pub capability: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Event {
    pub event_id: String,
    pub topic: Topic,
    pub schema: String,
    pub schema_version: u32,
    pub producer: String,
    pub workflow_run_id: Option<String>,
    pub goal_id: Option<String>,
    pub correlation_id: String,
    pub causation_id: Option<String>,
    pub created_at_ms: i64,
    pub payload: serde_json::Value,
    pub content_hash: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeliveryState {
    Queued,
    InFlight,
    Acked,
    DeadLetter,
    Unknown,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("unsupported bus schema version")]
    UnsupportedVersion,
    #[error("event bus input exceeds bounds")]
    TooLarge,
    #[error("invalid event bus value: {0}")]
    Invalid(String),
    #[error("capability denied")]
    CapabilityDenied,
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
pub fn validate_topic(t: &Topic) -> Result<(), Error> {
    text(&t.namespace)?;
    text(&t.name)?;
    if let Some(v) = &t.partition_key {
        text(v)?;
    }
    Ok(())
}
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
pub fn hash<T: Serialize>(v: &T) -> Result<String, Error> {
    let b = serde_json::to_vec(v).map_err(|_| Error::TooLarge)?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(b))))
}
pub fn matches(s: &Selector, e: &Event) -> bool {
    match s {
        Selector::Exact(t) => t == &e.topic,
        Selector::NamespacePrefix(v) => e.topic.namespace.starts_with(v),
        Selector::Type(v) => e.schema == *v,
    }
}
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
