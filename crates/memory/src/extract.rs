//! Memory extraction: JSON parse + heuristic + experience patterns (6.20–6.21).

use crate::experience::{parse_playbook_payload, PlaybookPayload};
use evohime_storage::{MemoryKind, MemoryScope, NewMemoryItem};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

pub const MAX_CANDIDATES_PER_TASK: usize = 5;
const SUMMARY_LIMIT: usize = 400;
const TRIGGER_LIMIT: usize = 160;
const STEP_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedCandidate {
    pub scope: MemoryScope,
    pub kind: MemoryKind,
    pub content: String,
    pub content_json: Option<Value>,
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
            content_json: self.content_json,
            confidence: self.confidence,
            importance: self.importance,
            pinned: self.pinned,
            source_session_id: session_id,
            source_task_id: task_id,
            source_label: Some(source_label.into()),
            supersedes: None,
            valid_until: None,
            validity_hint: None,
            embedding: None,
            embedding_version: 0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawCandidate {
    scope: String,
    kind: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    content_json: Option<Value>,
    #[serde(default)]
    playbook: Option<PlaybookPayload>,
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

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn truncate_summary(user_message: &str, final_message: &str) -> String {
    let summary = format!(
        "User: {} | Assistant: {}",
        user_message.trim(),
        final_message.trim()
    );
    truncate_chars(&summary, SUMMARY_LIMIT)
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

    let (content, content_json) = if kind == MemoryKind::Playbook {
        if let Some(playbook) = raw.playbook.filter(|p| p.validate()) {
            (playbook.to_content_text(), Some(playbook.to_content_json()))
        } else if let Some(json) = raw.content_json.as_ref().and_then(parse_playbook_payload) {
            (json.to_content_text(), Some(json.to_content_json()))
        } else {
            let content = raw.content.trim().to_string();
            if content.is_empty() {
                return None;
            }
            (content, raw.content_json)
        }
    } else {
        let content = raw.content.trim().to_string();
        if content.is_empty() {
            return None;
        }
        (content, raw.content_json)
    };

    Some(ExtractedCandidate {
        scope,
        kind,
        content,
        content_json,
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
    if let (Some(open), Some(close)) = (raw.find('['), raw.rfind(']')) {
        if open < close {
            out.push(raw[open..=close].to_string());
        }
    }
    out
}

/// Transient provider / infra failures must not become experience memory.
/// Example: LiteRouter tier cooldown (`403` / rate limit) is noise, not a playbook.
pub fn is_transient_infra_failure(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    let rate_limited = lower.contains("rate limit")
        || lower.contains("ratelimit")
        || lower.contains("cooldown")
        || lower.contains("too many requests")
        || lower.contains("429");
    let provider_http = (lower.contains("model error") || lower.contains("api error"))
        && (lower.contains("403")
            || lower.contains("429")
            || lower.contains("502")
            || lower.contains("503")
            || lower.contains("504")
            || lower.contains("forbidden"));
    rate_limited || provider_http
}

/// One-shot task dumps (dir listings, step transcripts) are not durable memory.
pub fn is_ephemeral_task_dump(content: &str) -> bool {
    let text = content.trim();
    if text.is_empty() {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    // Do not match heuristic summaries (`User: … | Assistant: …`) — only step/dir dumps.
    if lower.contains("step-1") || lower.contains("step‑1") || lower.contains("step 1:") {
        return true;
    }
    let root_markers = [
        ".git",
        "cargo.toml",
        "package.json",
        "node_modules",
        "readme.md",
        "migrations",
    ];
    let marker_hits = root_markers
        .iter()
        .filter(|marker| lower.contains(*marker))
        .count();
    if marker_hits >= 3 {
        return true;
    }
    // Fenced block that looks like a raw directory listing.
    if let Some(fence) = text.find("```") {
        let after = &text[fence + 3..];
        let body = after
            .strip_prefix('\n')
            .or_else(|| after.strip_prefix("text\n"))
            .unwrap_or(after);
        if let Some(end) = body.find("```") {
            let listing = &body[..end];
            let lines = listing
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>();
            if lines.len() >= 8 {
                let fileish = lines
                    .iter()
                    .filter(|line| {
                        !line.contains(' ')
                            && (line.contains('.') || line.starts_with('.') || !line.contains('/'))
                    })
                    .count();
                if fileish * 2 >= lines.len() {
                    return true;
                }
            }
        }
    }
    false
}

/// Success/failure/verification/playbook drafts for the experience scope.
pub fn experience_patterns(
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Vec<ExtractedCandidate> {
    // Success playbooks need structured LLM extract — dumping the whole reply is noise.
    if task_ok {
        return Vec::new();
    }
    if is_transient_infra_failure(final_message) || is_ephemeral_task_dump(final_message) {
        return Vec::new();
    }
    let trigger = truncate_chars(user_message, TRIGGER_LIMIT);
    if trigger.is_empty() {
        return Vec::new();
    }
    let outcome = truncate_chars(final_message, STEP_LIMIT);
    if outcome.is_empty() || is_ephemeral_task_dump(&outcome) {
        return Vec::new();
    }

    let mut out = Vec::new();
    out.push(ExtractedCandidate {
        scope: MemoryScope::Experience,
        kind: MemoryKind::FailurePattern,
        content: format!("When '{trigger}' failed: {outcome}"),
        content_json: None,
        confidence: 0.68,
        importance: 0.65,
        pinned: false,
    });
    let playbook = PlaybookPayload {
        trigger: trigger.clone(),
        steps: vec![format!("Investigate failure: {outcome}")],
        verify: Some("Failure no longer reproduces".into()),
        rollback_hint: Some("Revert partial changes from the failed attempt".into()),
    };
    out.push(ExtractedCandidate {
        scope: MemoryScope::Experience,
        kind: MemoryKind::Playbook,
        content: playbook.to_content_text(),
        content_json: Some(playbook.to_content_json()),
        confidence: 0.55,
        importance: 0.6,
        pinned: false,
    });
    out
}

fn merge_unique(
    mut base: Vec<ExtractedCandidate>,
    extra: Vec<ExtractedCandidate>,
) -> Vec<ExtractedCandidate> {
    for candidate in extra {
        if base.len() >= MAX_CANDIDATES_PER_TASK {
            break;
        }
        let duplicate = base.iter().any(|existing| {
            existing.kind == candidate.kind
                && existing.scope == candidate.scope
                && existing.content.eq_ignore_ascii_case(&candidate.content)
        });
        if !duplicate {
            base.push(candidate);
        }
    }
    base.truncate(MAX_CANDIDATES_PER_TASK);
    base
}

/// Deterministic fallback when LLM extract fails or returns nothing.
pub fn heuristic_extract(
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Vec<ExtractedCandidate> {
    // Do not turn provider outages / rate limits into session facts or playbooks.
    if !task_ok && is_transient_infra_failure(final_message) {
        return Vec::new();
    }

    let summary = truncate_summary(user_message, final_message);
    if summary.is_empty() || is_ephemeral_task_dump(&summary) || is_ephemeral_task_dump(final_message)
    {
        // Still allow real failure patterns when the failure text is usable.
        return experience_patterns(user_message, final_message, task_ok);
    }

    let mut out = Vec::new();

    // Session dumps at 0.55 only spam ask-on-uncertainty; keep short workspace facts.
    if task_ok {
        out.push(ExtractedCandidate {
            scope: MemoryScope::Workspace,
            kind: MemoryKind::Fact,
            content: summary,
            content_json: None,
            confidence: 0.75,
            importance: 0.5,
            pinned: false,
        });
    } else {
        out.push(ExtractedCandidate {
            scope: MemoryScope::Session,
            kind: MemoryKind::Fact,
            content: summary,
            content_json: None,
            confidence: 0.55,
            importance: 0.4,
            pinned: false,
        });
    }

    merge_unique(
        out,
        experience_patterns(user_message, final_message, task_ok),
    )
}

/// Prefer parsed LLM candidates; always try to supplement with experience patterns.
pub fn extract_candidates(
    llm_raw: Option<&str>,
    user_message: &str,
    final_message: &str,
    task_ok: bool,
) -> Vec<ExtractedCandidate> {
    let mut out = Vec::new();
    if let Some(raw) = llm_raw {
        out.extend(parse_extraction_json(raw));
    }
    out.retain(|c| {
        !is_transient_infra_failure(&c.content) && !is_ephemeral_task_dump(&c.content)
    });
    if !task_ok && is_transient_infra_failure(final_message) {
        return out;
    }
    if out.is_empty() {
        return heuristic_extract(user_message, final_message, task_ok);
    }
    merge_unique(
        out,
        experience_patterns(user_message, final_message, task_ok),
    )
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
        assert_eq!(items[1].confidence, 1.0);
        assert_eq!(items[1].importance, 0.0);
    }

    #[test]
    fn parses_playbook_object() {
        let raw = r#"[{
          "scope":"experience",
          "kind":"playbook",
          "confidence":0.8,
          "playbook":{
            "trigger":"flake tests",
            "steps":["re-run once","check timing"],
            "verify":"suite green"
          }
        }]"#;
        let items = parse_extraction_json(raw);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, MemoryKind::Playbook);
        assert!(items[0].content.contains("When flake tests"));
        assert!(items[0].content_json.is_some());
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
    fn heuristic_success_keeps_workspace_fact_without_experience_dump() {
        let items = heuristic_extract("add tests", "done, added tests", true);
        assert!(items.iter().all(|c| c.scope != MemoryScope::Session));
        assert!(items
            .iter()
            .any(|c| c.scope == MemoryScope::Workspace && c.kind == MemoryKind::Fact));
        assert!(items.iter().all(|c| c.scope != MemoryScope::Experience));
        assert!(items.len() <= MAX_CANDIDATES_PER_TASK);
    }

    #[test]
    fn heuristic_failure_uses_experience_failure_not_global() {
        let items = heuristic_extract("deploy", "failed: timeout", false);
        assert!(items
            .iter()
            .any(|c| c.kind == MemoryKind::FailurePattern && c.scope == MemoryScope::Experience));
        assert!(items.iter().all(|c| c.scope != MemoryScope::Global));
        assert!(!items
            .iter()
            .any(|c| c.scope == MemoryScope::Workspace && c.kind == MemoryKind::FailurePattern));
    }

    #[test]
    fn extract_candidates_keeps_llm_without_blind_success_experience() {
        let raw =
            r#"[{"scope":"project","kind":"preference","content":"from llm","confidence":0.8}]"#;
        let items = extract_candidates(Some(raw), "add auth", "wired jwt", true);
        assert!(items.iter().any(|c| c.content == "from llm"));
        assert!(items
            .iter()
            .all(|c| c.kind != MemoryKind::SuccessPattern && c.kind != MemoryKind::Playbook));
        assert!(items.len() <= MAX_CANDIDATES_PER_TASK);
    }

    #[test]
    fn extract_candidates_falls_back_on_bad_llm() {
        let items = extract_candidates(Some("???"), "hello", "world", true);
        assert!(!items.is_empty());
        assert!(items.iter().any(|c| c.scope == MemoryScope::Workspace));
        assert!(items.iter().all(|c| c.scope != MemoryScope::Experience));
    }

    #[test]
    fn skips_literouter_rate_limit_as_experience() {
        let msg = r#"model error: api error: 403 Forbidden: {"error":"[LiteRouter] Rate limit exceeded for your tier (5 seconds between messages)."}"#;
        assert!(is_transient_infra_failure(msg));
        assert!(experience_patterns("razberis v kode", msg, false).is_empty());
        assert!(heuristic_extract("razberis v kode", msg, false).is_empty());
        let items = extract_candidates(None, "razberis v kode", msg, false);
        assert!(items.is_empty());
    }

    #[test]
    fn skips_directory_listing_playbook_noise() {
        let reply = r#"**Step-1: root listing**
```
.agents
.codex
.cursor
.git
.github
Cargo.toml
README.md
crates
docs
frontend
node_modules
package.json
migrations
```
Then read README."#;
        assert!(is_ephemeral_task_dump(reply));
        assert!(experience_patterns("explore codebase", reply, true).is_empty());
        assert!(heuristic_extract("explore codebase", reply, true).is_empty());
        let llm = r#"[{"scope":"experience","kind":"playbook","content":"When explore: **Step-1: root** ``` .git Cargo.toml README.md package.json migrations ```","confidence":0.6}]"#;
        let items = extract_candidates(Some(llm), "explore codebase", reply, true);
        assert!(items.is_empty());
    }

    #[test]
    fn real_task_failure_still_yields_failure_pattern() {
        let items = experience_patterns("deploy", "failed: timeout waiting for pod", false);
        assert!(items.iter().any(|c| c.kind == MemoryKind::FailurePattern));
    }
}
