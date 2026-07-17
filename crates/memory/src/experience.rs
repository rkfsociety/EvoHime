//! Experience memory and playbooks (roadmap 6.21).
//!
//! Playbooks carry `content_json` with `trigger → steps → verify → rollback_hint`.

use evohime_storage::MemoryKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybookPayload {
    pub trigger: String,
    pub steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verify: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_hint: Option<String>,
}

impl PlaybookPayload {
    pub fn validate(&self) -> bool {
        !self.trigger.trim().is_empty() && self.steps.iter().any(|step| !step.trim().is_empty())
    }

    pub fn to_content_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Human-readable one-liner used as `content` for retrieval/dedupe.
    pub fn to_content_text(&self) -> String {
        let steps = self
            .steps
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" → ");
        let mut text = format!("When {}: {}", self.trigger.trim(), steps);
        if let Some(verify) = self.verify.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            text.push_str(&format!(" | verify: {verify}"));
        }
        if let Some(rollback) = self
            .rollback_hint
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            text.push_str(&format!(" | rollback: {rollback}"));
        }
        text
    }
}

pub fn parse_playbook_payload(value: &Value) -> Option<PlaybookPayload> {
    let payload: PlaybookPayload = serde_json::from_value(value.clone()).ok()?;
    if payload.validate() {
        Some(payload)
    } else {
        None
    }
}

/// Format a memory line with kind label for experience patterns in the prompt.
pub fn format_experience_line(kind: MemoryKind, content: &str) -> String {
    let label = match kind {
        MemoryKind::SuccessPattern => "success_pattern",
        MemoryKind::FailurePattern => "failure_pattern",
        MemoryKind::VerificationRule => "verification_rule",
        MemoryKind::Playbook => "playbook",
        other => other.as_str(),
    };
    format!("{label}: {}", content.trim())
}

pub fn is_experience_kind(kind: MemoryKind) -> bool {
    matches!(
        kind,
        MemoryKind::SuccessPattern
            | MemoryKind::FailurePattern
            | MemoryKind::VerificationRule
            | MemoryKind::Playbook
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn playbook_round_trips_and_formats() {
        let payload = PlaybookPayload {
            trigger: "tests fail".into(),
            steps: vec!["read error".into(), "fix code".into(), "re-run".into()],
            verify: Some("cargo test passes".into()),
            rollback_hint: Some("git restore".into()),
        };
        assert!(payload.validate());
        let text = payload.to_content_text();
        assert!(text.contains("When tests fail"));
        assert!(text.contains("read error → fix code → re-run"));
        assert!(text.contains("verify: cargo test passes"));
        let parsed = parse_playbook_payload(&payload.to_content_json()).expect("parse");
        assert_eq!(parsed, payload);
    }

    #[test]
    fn rejects_empty_playbook() {
        let bad = PlaybookPayload {
            trigger: "".into(),
            steps: vec![],
            verify: None,
            rollback_hint: None,
        };
        assert!(!bad.validate());
        assert!(parse_playbook_payload(&json!({"trigger":"","steps":[]})).is_none());
    }
}
