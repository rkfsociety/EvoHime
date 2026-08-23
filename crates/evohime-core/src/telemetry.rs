//! Bounded local telemetry projection for offline evaluation.
//!
//! This module deliberately stores only derived, redacted metadata. Core event
//! journal, receipts and model-request provenance remain the source of truth.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const TELEMETRY_SCHEMA_VERSION: u32 = 1;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_EVENTS: usize = 512;
pub const MAX_REPORT_BYTES: usize = 256 * 1024;
pub const MAX_REASON_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryVerdict {
    Pass,
    Fail,
    Unknown,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub event_id: String,
    pub event_kind: String,
    pub trace_id: String,
    pub run_id: String,
    pub attempt: u32,
    pub created_at_ms: i64,
    pub outcome: String,
    pub reason_code: Option<String>,
    pub manifest_hash: Option<String>,
    pub model_request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryReport {
    pub schema_version: u32,
    pub evaluation_run_id: String,
    pub fixture_hash: String,
    pub verdict: TelemetryVerdict,
    pub reason_code: Option<String>,
    pub source_events: Vec<TelemetryEvent>,
    pub judge_signal: Option<String>,
    pub redaction_applied: bool,
    pub metrics: BTreeMap<String, i64>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TelemetryError {
    #[error("telemetry field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("telemetry event limit exceeded")]
    TooManyEvents,
    #[error("telemetry report exceeds bounded size")]
    ReportTooLarge,
    #[error("telemetry report contains forbidden payload")]
    ForbiddenPayload,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES && !value.chars().any(char::is_control)
}

fn bounded_reason(value: &str) -> Option<String> {
    let value: String = value.chars().take(MAX_REASON_BYTES).collect();
    (!value.is_empty() && !value.chars().any(char::is_control)).then_some(value)
}

impl TelemetryEvent {
    pub fn validate(&self) -> Result<(), TelemetryError> {
        for (name, value) in [
            ("event_id", self.event_id.as_str()),
            ("event_kind", self.event_kind.as_str()),
            ("trace_id", self.trace_id.as_str()),
            ("run_id", self.run_id.as_str()),
            ("outcome", self.outcome.as_str()),
        ] {
            if !valid_id(value) {
                return Err(TelemetryError::InvalidField(name));
            }
        }
        if self.attempt == 0 || self.created_at_ms < 0 {
            return Err(TelemetryError::InvalidField("attempt_or_timestamp"));
        }
        if self
            .reason_code
            .as_deref()
            .is_some_and(|v| bounded_reason(v).is_none())
        {
            return Err(TelemetryError::InvalidField("reason_code"));
        }
        Ok(())
    }
}

impl TelemetryReport {
    pub fn validate(&self) -> Result<(), TelemetryError> {
        if self.schema_version != TELEMETRY_SCHEMA_VERSION
            || !valid_id(&self.evaluation_run_id)
            || !valid_id(&self.fixture_hash)
            || self.source_events.len() > MAX_EVENTS
            || !self.redaction_applied
        {
            return Err(TelemetryError::InvalidField("report_metadata"));
        }
        for event in &self.source_events {
            event.validate()?;
        }
        let encoded = serde_json::to_vec(self).map_err(|_| TelemetryError::ForbiddenPayload)?;
        if encoded.len() > MAX_REPORT_BYTES {
            return Err(TelemetryError::ReportTooLarge);
        }
        Ok(())
    }

    pub fn canonical_json(&self) -> Result<String, TelemetryError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|_| TelemetryError::ForbiddenPayload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str) -> TelemetryEvent {
        TelemetryEvent {
            event_id: id.into(),
            event_kind: "model.completed".into(),
            trace_id: "trace-1".into(),
            run_id: "run-1".into(),
            attempt: 1,
            created_at_ms: 1,
            outcome: "ok".into(),
            reason_code: None,
            manifest_hash: Some("manifest-1".into()),
            model_request_id: Some("request-1".into()),
        }
    }

    #[test]
    fn report_is_bounded_and_requires_redaction() {
        let report = TelemetryReport {
            schema_version: TELEMETRY_SCHEMA_VERSION,
            evaluation_run_id: "eval-1".into(),
            fixture_hash: "fixture-1".into(),
            verdict: TelemetryVerdict::Pass,
            reason_code: None,
            source_events: vec![event("event-1")],
            judge_signal: Some("advisory".into()),
            redaction_applied: true,
            metrics: BTreeMap::new(),
        };
        assert!(report.canonical_json().is_ok());
    }

    #[test]
    fn invalid_event_and_unredacted_report_are_rejected() {
        let mut invalid = event("event\n1");
        assert_eq!(
            invalid.validate(),
            Err(TelemetryError::InvalidField("event_id"))
        );
        invalid.event_id = "event-1".into();
        let report = TelemetryReport {
            schema_version: TELEMETRY_SCHEMA_VERSION,
            evaluation_run_id: "eval-1".into(),
            fixture_hash: "fixture-1".into(),
            verdict: TelemetryVerdict::Pass,
            reason_code: None,
            source_events: vec![invalid],
            judge_signal: None,
            redaction_applied: false,
            metrics: BTreeMap::new(),
        };
        assert_eq!(
            report.validate(),
            Err(TelemetryError::InvalidField("report_metadata"))
        );
    }
}
