//! Core-owned, bounded diagnostics snapshot contract.
//!
//! The snapshot is deliberately ephemeral. It contains only typed health
//! results and selected run references; Electron main owns archive assembly.

use serde::Serialize;
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 2;
pub const MAX_ID_CHARS: usize = 128;
pub const MAX_EVENTS: u32 = 200;
pub const MAX_LOG_BYTES: u32 = 64 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct SnapshotResult {
    pub schema_version: u32,
    pub scope: &'static str,
    pub conversation_id: String,
    pub run_id: String,
    pub health: Vec<HealthResult>,
    pub selected_run: SelectedRun,
    pub bounds: Bounds,
    pub redaction: RedactionSummary,
    pub issue_draft: String,
    pub snapshot_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthResult {
    pub id: String,
    pub status: String,
    pub reason_code: String,
    pub safe_summary: String,
    pub remediation_hint: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SelectedRun {
    pub selected: bool,
    pub conversation_id: String,
    pub run_id: String,
    pub run_status: String,
    pub context_status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bounds {
    pub max_event_count: u32,
    pub max_log_bytes: u32,
    pub events_included: u32,
    pub logs_included: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RedactionSummary {
    pub rules_version: &'static str,
    pub total_matches: u32,
    pub blocked_sections: Vec<&'static str>,
    pub raw_payloads_included: bool,
}

pub fn validate_id(value: &str) -> Result<(), &'static str> {
    if value.chars().count() > MAX_ID_CHARS {
        return Err("id_too_long");
    }
    if value.chars().any(|c| c.is_control()) {
        return Err("id_contains_control");
    }
    Ok(())
}

pub fn build_snapshot(
    doctor_json: &[u8],
    conversation_id: String,
    run_id: String,
    run_status: String,
    max_event_count: u32,
    max_log_bytes: u32,
    duration_ms: u64,
) -> Result<Vec<u8>, String> {
    validate_id(&conversation_id).map_err(str::to_owned)?;
    validate_id(&run_id).map_err(str::to_owned)?;
    if max_event_count > MAX_EVENTS || max_log_bytes > MAX_LOG_BYTES {
        return Err("limit_exceeded".into());
    }
    let doctor: serde_json::Value =
        serde_json::from_slice(doctor_json).map_err(|_| "invalid_doctor_report".to_owned())?;
    let health = doctor
        .get("checks")
        .and_then(serde_json::Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .take(16)
                .map(|check| HealthResult {
                    id: check
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_owned(),
                    status: map_status(
                        check
                            .get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("SKIPPED"),
                    ),
                    reason_code: reason_code(check),
                    safe_summary: bounded(
                        check
                            .get("summary")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Проверка недоступна"),
                        256,
                    ),
                    remediation_hint: check
                        .get("action")
                        .and_then(|v| v.as_str())
                        .map(|v| bounded(v, 256)),
                    duration_ms,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let selected = !conversation_id.is_empty() || !run_id.is_empty();
    let selected_run = SelectedRun {
        selected,
        conversation_id: conversation_id.clone(),
        run_id: run_id.clone(),
        run_status,
        context_status: if selected {
            "metadata_only"
        } else {
            "application_only"
        },
    };
    let draft = issue_draft(&health);
    let result = SnapshotResult {
        schema_version: SCHEMA_VERSION,
        scope: if selected {
            "selected_run"
        } else {
            "application"
        },
        conversation_id,
        run_id,
        health,
        selected_run,
        bounds: Bounds {
            max_event_count,
            max_log_bytes,
            events_included: 0,
            logs_included: false,
            truncated: false,
        },
        redaction: RedactionSummary {
            rules_version: "sensitive-data-guardrails-v1",
            total_matches: 0,
            blocked_sections: vec![
                "credentials",
                "raw_prompts",
                "workspace_files",
                "tool_payloads",
            ],
            raw_payloads_included: false,
        },
        issue_draft: draft,
        snapshot_hash: String::new(),
    };
    let mut bytes = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    hash.update(&bytes);
    let digest = hex::encode(hash.finalize());
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    value["snapshot_hash"] = serde_json::Value::String(digest);
    bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if bytes.len() > 256 * 1024 {
        return Err("snapshot_too_large".into());
    }
    Ok(bytes)
}

fn map_status(value: &str) -> String {
    match value {
        "OK" => "PASS",
        "BLOCKED" => "SKIPPED",
        "WARN" => "WARN",
        "FAIL" => "FAIL",
        _ => "SKIPPED",
    }
    .into()
}

fn reason_code(check: &serde_json::Value) -> String {
    check
        .get("id")
        .and_then(|v| v.as_str())
        .map(|id| format!("health.{id}"))
        .unwrap_or_else(|| "health.unknown".into())
}

fn bounded(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn issue_draft(health: &[HealthResult]) -> String {
    let failures = health.iter().filter(|item| item.status == "FAIL").count();
    format!("### Problem\nEvoHime diagnostics snapshot: {failures} failed health checks.\n\n### Environment\nSee manifest and redacted health section.\n\n### Reproduction context\nOnly bounded metadata references are included.\n\n### Error classes\nTyped health result codes are listed in health.json.\n\n### Diagnostics\nRaw prompts, files, credentials and tool payloads are excluded.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_is_bounded_and_secret_free() {
        let bytes = build_snapshot(
            br#"{"checks":[{"id":"storage","status":"OK","summary":"safe","action":"none"}]}"#,
            "conv-1".into(),
            "run-1".into(),
            "failed".into(),
            20,
            100,
            3,
        )
        .unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"schema_version\":2"));
        assert!(text.contains("PASS"));
        assert!(!text.contains("secret"));
        assert!(text.contains("\"raw_payloads_included\":false"));
    }

    #[test]
    fn limits_and_control_ids_fail_closed() {
        assert_eq!(
            build_snapshot(
                br#"{"checks":[]}"#,
                "bad\n".into(),
                String::new(),
                String::new(),
                0,
                0,
                0
            )
            .unwrap_err(),
            "id_contains_control"
        );
        assert_eq!(
            build_snapshot(
                br#"{"checks":[]}"#,
                String::new(),
                String::new(),
                String::new(),
                MAX_EVENTS + 1,
                0,
                0
            )
            .unwrap_err(),
            "limit_exceeded"
        );
    }
}
