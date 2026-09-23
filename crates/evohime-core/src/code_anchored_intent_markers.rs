//! Bounded, inert-by-default code comment intent markers.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Version of the serialized code intent marker contract.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of markers accepted in a single parse operation.
pub const MAX_MARKERS: usize = 128;
/// Maximum marker or comment text size in bytes.
pub const MAX_TEXT: usize = 1024;
/// Minimum delay between repeated scan deliveries of the same marker.
pub const DEBOUNCE_MS: u64 = 1_000;

/// Action intent encoded by a recognized source comment marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentKind {
    /// The comment requests a code change.
    EditRequest,
    /// The comment asks a question about the code.
    Question,
}
/// Trust classification of the source that supplied a marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Explicitly supplied or confirmed by the user.
    UserTrusted,
    /// Found in existing repository content.
    ExistingRepository,
    /// Created by an agent.
    AgentGenerated,
    /// Imported from an untrusted external source.
    ImportedUntrusted,
}
/// Processing state of a parsed code intent marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerStatus {
    /// Parsed and awaiting stale, trust, and deduplication checks.
    Candidate,
    /// Source revision no longer matches the current file revision.
    Stale,
    /// Duplicate delivery was suppressed.
    Deduplicated,
    /// Marker was rejected by a gate.
    Rejected,
    /// Marker has been turned into a proposal.
    Proposed,
}
/// Source line range and comment text supplied to the marker parser.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommentRange {
    /// First one-based source line covered by the comment.
    pub start_line: u32,
    /// Last one-based source line covered by the comment.
    pub end_line: u32,
    /// Complete bounded comment text.
    pub text: String,
}
/// Parsed marker bound to a source path, revision, and provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeIntentMarker {
    /// Serialized contract version.
    pub schema_version: u32,
    /// Stable digest-derived marker identifier.
    pub marker_id: String,
    /// Requested action kind.
    pub kind: IntentKind,
    /// Text following the recognized marker prefix.
    pub text: String,
    /// Workspace-relative path containing the source comment.
    pub file_path: String,
    /// Source revision at parse time.
    pub revision: String,
    /// First one-based line containing the comment.
    pub range_start: u32,
    /// Last one-based line containing the comment.
    pub range_end: u32,
    /// Optional symbol associated with this source range.
    pub symbol: Option<String>,
    /// Trust classification of the marker source.
    pub provenance: Provenance,
    /// Current gated processing state.
    pub status: MarkerStatus,
    /// Digest of the source comment range.
    pub content_hash: String,
}
/// Invalid, oversized, stale, untrusted, or duplicate marker operation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MarkerError {
    /// Marker fields or source range violate the contract.
    #[error("invalid marker contract")]
    Invalid,
    /// Text or marker count exceeds its configured bound.
    #[error("marker input exceeds bound")]
    Limit,
    /// The marker's source revision differs from the current revision.
    #[error("marker is stale")]
    Stale,
    /// Provenance is not permitted to create a proposal automatically.
    #[error("untrusted marker cannot auto-trigger")]
    Untrusted,
    /// The marker duplicates another entry.
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
/// Extracts recognized `EVA!` edit requests and `EVA?` questions from bounded comment ranges.
///
/// Returned markers remain inert candidates; callers must validate the source
/// revision and apply provenance policy before proposing an action.
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
/// Checks marker bounds and that its recorded revision matches the current source revision.
///
/// This validates the presence of a content digest but does not recompute it
/// from the original source text, which is not an argument to this function.
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
/// Allows automatic proposal only for markers explicitly classified as user trusted.
pub fn can_auto_propose(marker: &CodeIntentMarker) -> Result<(), MarkerError> {
    match marker.provenance {
        Provenance::UserTrusted => Ok(()),
        _ => Err(MarkerError::Untrusted),
    }
}
/// Sorts markers by stable identifier and marks later duplicate entries.
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
    /// Debounces repeated repository scans and rejects other provenance classes.
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
