use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_PROPERTIES: usize = 16;
pub const MAX_QUEUE: usize = 512;
pub const MAX_BYTES: usize = 64 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryCategory {
    Product,
    Operational,
    Diagnostics,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Consent {
    Unknown,
    Denied,
    Granted,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentState {
    pub schema_version: u32,
    pub product: Consent,
    pub operational: Consent,
    pub diagnostics: Consent,
    pub revision: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryEventV1 {
    pub schema_version: u32,
    pub event_id: String,
    pub category: TelemetryCategory,
    pub name: String,
    pub properties: BTreeMap<String, String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryEnqueueRequest {
    pub consent: ConsentState,
    pub event: TelemetryEventV1,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GovernanceError {
    #[error("unsupported telemetry schema")]
    UnsupportedVersion,
    #[error("invalid telemetry value")]
    Invalid,
    #[error("telemetry property is not allowlisted")]
    UnknownProperty,
    #[error("telemetry queue bound exceeded")]
    Bounds,
    #[error("telemetry consent denied")]
    ConsentDenied,
    #[error("sensitive telemetry value rejected")]
    SensitiveValue,
}
const ALLOWED: [&str; 8] = [
    "app_version",
    "platform",
    "action_class",
    "outcome",
    "reason_code",
    "duration_ms",
    "schema_version",
    "count",
];
pub fn validate_consent(c: &ConsentState) -> Result<(), GovernanceError> {
    if c.schema_version != SCHEMA_VERSION {
        return Err(GovernanceError::UnsupportedVersion);
    }
    Ok(())
}
pub fn validate_event(e: &TelemetryEventV1) -> Result<(), GovernanceError> {
    if e.schema_version != SCHEMA_VERSION
        || e.event_id.is_empty()
        || e.event_id.len() > 128
        || e.name.is_empty()
        || e.name.len() > 128
        || e.created_at_ms < 0
        || e.properties.len() > MAX_PROPERTIES
    {
        return Err(GovernanceError::Invalid);
    }
    if e.properties.keys().any(|k| !ALLOWED.contains(&k.as_str())) {
        return Err(GovernanceError::UnknownProperty);
    }
    if e.properties
        .values()
        .any(|value| contains_sensitive_marker(value))
    {
        return Err(GovernanceError::SensitiveValue);
    }
    if serde_json::to_vec(e)
        .map_err(|_| GovernanceError::Invalid)?
        .len()
        > MAX_BYTES
    {
        return Err(GovernanceError::Bounds);
    }
    Ok(())
}

fn contains_sensitive_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "sk-",
        "ghp_",
        "github_pat_",
        "token=",
        "api_key=",
        "password=",
        "secret=",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}
pub fn consent_for(c: &ConsentState, k: TelemetryCategory) -> Consent {
    match k {
        TelemetryCategory::Product => c.product,
        TelemetryCategory::Operational => c.operational,
        TelemetryCategory::Diagnostics => c.diagnostics,
    }
}
pub fn enqueue(c: &ConsentState, e: TelemetryEventV1) -> Result<bool, GovernanceError> {
    validate_consent(c)?;
    validate_event(&e)?;
    if consent_for(c, e.category) != Consent::Granted {
        return Err(GovernanceError::ConsentDenied);
    }
    Ok(true)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn c() -> ConsentState {
        ConsentState {
            schema_version: 1,
            product: Consent::Granted,
            operational: Consent::Denied,
            diagnostics: Consent::Unknown,
            revision: 1,
        }
    }
    #[test]
    fn allowlist_and_consent_are_closed() {
        let mut p = BTreeMap::new();
        p.insert("outcome".into(), "ok".into());
        let e = TelemetryEventV1 {
            schema_version: 1,
            event_id: "e".into(),
            category: TelemetryCategory::Product,
            name: "task.completed".into(),
            properties: p,
            created_at_ms: 1,
        };
        assert!(enqueue(&c(), e).unwrap())
    }
    #[test]
    fn unknown_property_fails() {
        let mut p = BTreeMap::new();
        p.insert("prompt".into(), "secret".into());
        let e = TelemetryEventV1 {
            schema_version: 1,
            event_id: "e".into(),
            category: TelemetryCategory::Product,
            name: "x".into(),
            properties: p,
            created_at_ms: 1,
        };
        assert_eq!(validate_event(&e), Err(GovernanceError::UnknownProperty))
    }

    #[test]
    fn sensitive_value_is_rejected_before_queueing() {
        let mut p = BTreeMap::new();
        p.insert("reason_code".into(), "token=do-not-send".into());
        let e = TelemetryEventV1 {
            schema_version: 1,
            event_id: "e".into(),
            category: TelemetryCategory::Product,
            name: "x".into(),
            properties: p,
            created_at_ms: 1,
        };
        assert_eq!(validate_event(&e), Err(GovernanceError::SensitiveValue));
    }
}
