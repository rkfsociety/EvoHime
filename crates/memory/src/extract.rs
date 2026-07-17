//! Memory extraction: JSON parse + heuristic fallback (roadmap 6.20).

use evohime_storage::{MemoryKind, MemoryScope, NewMemoryItem};
use serde::Deserialize;
use uuid::Uuid;

pub const MAX_CANDIDATES_PER_TASK: usize = 5;
const SUMMARY_LIMIT: usize = 400;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedCandidate {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub content: String,
    pub confidence: f64,
    pub importance: f64,
    pub pinned: bool,
}

impl ExtractedCandidate {
    pub fn into_new_item(
        self,
        scope_key: impl Into<String>,
        session_id: Option<Uuid>,
        task_id: Option<Uuid>,
        source_label: impl Into<String>,
    ) -> NewMemoryItem {
        NewMemoryItem {
            scope: self.scope,
            scope_key: scope_key.into(),
            kind: self.kind,
            status: evohime_storage::MemoryStatus::Candidate,
            content: self.content,
            content_json: None,
            confidence: self.confidence,
            importance: self.importance,
            pinned: self.pinned,
            source_session_id: session_id,
            source_task_id: task_id,
            source_label: Some(source_label.into()),
            supersedes: None,
            valid_until: None,
            validity_hint: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawCandidate {
    scope: String,
    kind: String,
    content: String,
    #[serde(default = "default_confidence")]
    confidence: f64,
    #[serde(default = "default_importance")]
    importance: f64,
    #[serde(default)]
    pinned: bool,
}

fn default_confidence() -> f64 {
    0.5
}

fn default_importance() -> f64 {
    0.5
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn truncate_summary(user_message: &str, final_message: &str) -> String {
    let summary = format!(
        "User: {} | Assistant: {}",
        user_message.trim(),
        final_message.trim()
    );
    let trimmed = summary.trim();
    if trimmed.chars().count() <= SUMMARY_LIMIT {
        return trimmed.to_string();
    }
    trimmed.chars().take(SUMMARY_LIMIT).collect::<String>() + "…"
}

/// Parse model output as a JSON array of memory candidates.
/// Accepts raw JSON or fenced ```json blocks. Returns empty on total failure.
pub fn parse_extraction_json(raw: &str) -> Vec<ExtractedCandidate> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    candidates.extend(try_parse_array(trimmed));
    if candidates.is_empty() {
        for block in extract_json_blocks(trimmed) {
            candidates.extend(try_parse_array(&block));
            if !candidates.is_empty() {
                break;
            }
        }
    }

    candidates.truncate(MAX_CANDIDATES_PER_TASK);
    candidates
}

fn try_parse_array(raw: &str) -> Vec<ExtractedCandidate> {
    let Ok(items) = serde_json::from_str::<Vec<RawCandidate>>(raw) else {
        // Single object fallback
        if let Ok(one) = serde_json::from_str::<RawCandidate>(raw) {
            return normalize_raw(one).into_iter().collect();
        }
        return Vec::new();
    };
    items.into_iter().filter_map(normalize_raw).collect()
}

fn normalize_raw(raw: RawCandidate) -> Option<ExtractedCandidate> {
    let scope = MemoryScope::parse(&raw.scope)?;
    let kind = MemoryKind::parse(&raw.kind)?;
    let content = raw.content.trim().to_string();
    if content.is_empty() {
        return None;
    }
    Some(ExtractedCandidate {
        scope,
        kind,
        content,
        confidence: clamp01(raw.confidence),
        importance: clamp01(raw.importance),
        pinned: raw.pinned,
    })
}

fn extract_json_blocks(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find("```") {
        let after_fence = &rest[start + 3..];
        let body = after_fence
            .strip_prefix("json")
            .or_else(|| after_fence.strip_prefix("JSON"))
            .unwrap_or(after_fence);
        let body = body.strip_prefix('\n').unwrap_or(body);
        if let Some(end) = body.find("```") {
            out.push(body[..end].trim().to_string());
            rest = &body[end + 3..];
        } else {
            break;
        }
    }
    // Also try first [...] slice
    if let (Some(open), Some(close)) = (raw.find('['), raw.rfind(']')) {
        if open < close {
            out.push(raw[open..=close].to_string());
        }
    }
    out
}

/// Deterministic fallback when LLM extract fails or returns nothing.
pub fn heuristic_extract(
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Vec<ExtractedCandidate> {
    let summary = truncate_summary(user_message, final_message);
    if summary.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();

    // Session note — low confidence → usually Ask
    out.push(ExtractedCandidate {
        scope: MemoryScope::Session,
        kind: MemoryKind::Fact,
        content: summary.clone(),
        confidence: 0.55,
        importance: 0.4,
        pinned: false,
    });

    if task_ok {
        out.push(ExtractedCandidate {
            scope: MemoryScope::Workspace,
            kind: MemoryKind::Fact,
            content: summary,
            confidence: 0.75,
            importance: 0.5,
            pinned: false,
        });
    } else {
        out.push(ExtractedCandidate {
            scope: MemoryScope::Workspace,
            kind: MemoryKind::FailurePattern,
            content: format!("Task failed context: {summary}"),
            confidence: 0.45,
            importance: 0.6,
            pinned: false,
        });
    }

    out.truncate(MAX_CANDIDATES_PER_TASK);
    out
}

/// Prefer parsed LLM candidates; fall back to heuristic when empty.
pub fn extract_candidates(
    llm_raw: Option<&str>,
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Vec<ExtractedCandidate> {
    if let Some(raw) = llm_raw {
        let parsed = parse_extraction_json(raw);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    heuristic_extract(user_message, final_message, task_ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_json_array() {
        let raw = r#"[
          {"scope":"workspace","kind":"fact","content":"uses postgres","confidence":0.9,"importance":0.5,"pinned":false},
          {"scope":"global","kind":"preference","content":"prefer typed apis","confidence":1.5,"importance":-1.0}
        ]"#;
        let items = parse_extraction_json(raw);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].scope, MemoryScope::Workspace);
        assert_eq!(items[0].confidence, 0.9);
        assert_eq!(items[1].confidence, 1.0); // clamped
        assert_eq!(items[1].importance, 0.0); // clamped
    }

    #[test]
    fn parses_fenced_json_and_drops_unknown() {
        let raw = r#"Here you go:
```json
[{"scope":"session","kind":"fact","content":"ok"},{"scope":"nope","kind":"fact","content":"bad"}]
```
"#;
        let items = parse_extraction_json(raw);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "ok");
    }

    #[test]
    fn empty_or_garbage_returns_empty() {
        assert!(parse_extraction_json("").is_empty());
        assert!(parse_extraction_json("not json at all").is_empty());
    }

    #[test]
    fn heuristic_success_has_session_and_workspace_fact() {
        let items = heuristic_extract("add tests", "done, added tests", true);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].scope, MemoryScope::Session);
        assert!((items[0].confidence - 0.55).abs() < f64::EPSILON);
        assert_eq!(items[1].scope, MemoryScope::Workspace);
        assert_eq!(items[1].kind, MemoryKind::Fact);
        assert!((items[1].confidence - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn heuristic_failure_uses_failure_pattern_not_global() {
        let items = heuristic_extract("deploy", "failed: timeout", false);
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].kind, MemoryKind::FailurePattern);
        assert!(items.iter().all(|c| c.scope != MemoryScope::Global));
        assert!(items[1].confidence < 0.7);
    }

    #[test]
    fn extract_candidates_prefers_llm_over_heuristic() {
        let raw = r#"[{"scope":"project","kind":"preference","content":"from llm","confidence":0.8}]"#;
        let items = extract_candidates(Some(raw), "u", "a", true);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "from llm");
    }

    #[test]
    fn extract_candidates_falls_back_on_bad_llm() {
        let items = extract_candidates(Some("???"), "hello", "world", true);
        assert_eq!(items.len(), 2);
    }
}
