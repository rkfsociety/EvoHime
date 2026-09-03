//! Bounded, inert-by-default code comment intent markers.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_MARKERS: usize = 128;
pub const MAX_TEXT: usize = 1024;
pub const DEBOUNCE_MS: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentKind {
    EditRequest,
    Question,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    UserTrusted,
    ExistingRepository,
    AgentGenerated,
    ImportedUntrusted,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerStatus {
    Candidate,
    Stale,
    Deduplicated,
    Rejected,
    Proposed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommentRange {
    pub start_line: u32,
    pub end_line: u32,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeIntentMarker {
    pub schema_version: u32,
    pub marker_id: String,
    pub kind: IntentKind,
    pub text: String,
    pub file_path: String,
    pub revision: String,
    pub range_start: u32,
    pub range_end: u32,
    pub symbol: Option<String>,
    pub provenance: Provenance,
    pub status: MarkerStatus,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MarkerError {
    #[error("invalid marker contract")]
    Invalid,
    #[error("marker input exceeds bound")]
    Limit,
    #[error("marker is stale")]
    Stale,
    #[error("untrusted marker cannot auto-trigger")]
    Untrusted,
    #[error("duplicate marker")]
    Duplicate,
}
fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.contains("..")
        && !path.starts_with('/')
        && !path.contains('\\')
}
fn hash<T: Serialize>(v: &T) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(v).unwrap_or_default()))
}
pub fn parse_comment_ranges(
    path: &str,
    revision: &str,
    ranges: &[CommentRange],
    provenance: Provenance,
) -> Result<Vec<CodeIntentMarker>, MarkerError> {
    if !valid_path(path) || revision.is_empty() || ranges.len() > MAX_MARKERS {
        return Err(MarkerError::Invalid);
    }
    let mut out = Vec::new();
    for range in ranges {
        if range.start_line == 0 || range.end_line < range.start_line || range.text.len() > MAX_TEXT
        {
            return Err(MarkerError::Limit);
        }
        for (kind, prefix) in [
            (IntentKind::EditRequest, "EVA!"),
            (IntentKind::Question, "EVA?"),
        ] {
            if let Some(pos) = range.text.find(prefix) {
                let text = range.text[pos + prefix.len()..].trim();
                if text.is_empty() || text.len() > MAX_TEXT {
                    return Err(MarkerError::Invalid);
                }
                let marker_id = hash(&(path, revision, range.start_line, range.end_line, text));
                out.push(CodeIntentMarker {
                    schema_version: SCHEMA_VERSION,
                    marker_id,
                    kind,
                    text: text.into(),
                    file_path: path.into(),
                    revision: revision.into(),
                    range_start: range.start_line,
                    range_end: range.end_line,
                    symbol: None,
                    provenance,
                    status: MarkerStatus::Candidate,
                    content_hash: hash(&(
                        path,
                        revision,
                        range.start_line,
                        range.end_line,
                        &range.text,
                    )),
                });
            }
        }
    }
    if out.len() > MAX_MARKERS {
        return Err(MarkerError::Limit);
    }
    Ok(out)
}
pub fn validate_marker(
    marker: &CodeIntentMarker,
    current_revision: &str,
) -> Result<(), MarkerError> {
    if marker.schema_version != SCHEMA_VERSION
        || !valid_path(&marker.file_path)
        || marker.text.is_empty()
        || marker.text.len() > MAX_TEXT
        || marker.range_start == 0
        || marker.range_end < marker.range_start
        || marker.content_hash.is_empty()
    {
        return Err(MarkerError::Invalid);
    }
    if marker.revision != current_revision {
        return Err(MarkerError::Stale);
    }
    Ok(())
}
pub fn can_auto_propose(marker: &CodeIntentMarker) -> Result<(), MarkerError> {
    match marker.provenance {
        Provenance::UserTrusted => Ok(()),
        _ => Err(MarkerError::Untrusted),
    }
}
pub fn deduplicate(markers: &mut [CodeIntentMarker]) {
    markers.sort_by(|a, b| a.marker_id.cmp(&b.marker_id));
    for i in 1..markers.len() {
        if markers[i].marker_id == markers[i - 1].marker_id {
            markers[i].status = MarkerStatus::Deduplicated;
        }
    }
}

/// Ephemeral gate for watcher/scan deliveries. It deliberately is not
/// persisted: after a restart the source revision and explicit user action
/// still decide whether a marker may proceed.
#[derive(Debug, Default)]
pub struct MarkerGate {
    last_seen_ms: HashMap<String, u64>,
}

impl MarkerGate {
    pub fn admit_scan(&mut self, markers: &mut [CodeIntentMarker], now_ms: u64) {
        for marker in markers {
            if marker.provenance != Provenance::ExistingRepository {
                marker.status = MarkerStatus::Rejected;
                continue;
            }
            if self
                .last_seen_ms
                .get(&marker.marker_id)
                .is_some_and(|last| now_ms.saturating_sub(*last) < DEBOUNCE_MS)
            {
                marker.status = MarkerStatus::Deduplicated;
            } else {
                self.last_seen_ms.insert(marker.marker_id.clone(), now_ms);
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn range(text: &str) -> CommentRange {
        CommentRange {
            start_line: 2,
            end_line: 2,
            text: text.into(),
        }
    }
    #[test]
    fn parses_only_typed_comment_ranges() {
        let x = parse_comment_ranges(
            "src/lib.rs",
            "rev-1",
            &[range("// EVA! add a test")],
            Provenance::UserTrusted,
        )
        .unwrap();
        assert_eq!(x[0].kind, IntentKind::EditRequest);
        assert!(can_auto_propose(&x[0]).is_ok());
    }
    #[test]
    fn imported_and_stale_markers_fail_closed() {
        let x = parse_comment_ranges(
            "src/lib.rs",
            "rev-1",
            &[range("// EVA? why")],
            Provenance::ImportedUntrusted,
        )
        .unwrap();
        assert_eq!(can_auto_propose(&x[0]), Err(MarkerError::Untrusted));
        assert_eq!(validate_marker(&x[0], "rev-2"), Err(MarkerError::Stale));
    }

    #[test]
    fn scan_gate_debounces_and_rejects_non_repository_provenance() {
        let mut gate = MarkerGate::default();
        let mut first = parse_comment_ranges(
            "src/lib.rs",
            "rev-1",
            &[range("// EVA! add a test")],
            Provenance::ExistingRepository,
        )
        .unwrap();
        gate.admit_scan(&mut first, 100);
        assert_eq!(first[0].status, MarkerStatus::Candidate);
        gate.admit_scan(&mut first, 500);
        assert_eq!(first[0].status, MarkerStatus::Deduplicated);
    }
}
