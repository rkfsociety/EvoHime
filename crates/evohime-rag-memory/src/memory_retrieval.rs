//! Memory-to-RAG adapter. Memory candidates keep their own citation identity;
//! they are never represented as workspace document chunks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum number of memory retrieval candidates accepted in one ranking request.
pub const MAX_MEMORY_CANDIDATES: usize = 64;
/// Maximum number of reason strings associated with a memory retrieval item.
pub const MAX_MEMORY_REASONS: usize = 32;

/// Memory record candidate considered during retrieval ranking and citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRetrievalCandidate {
    /// Stable memory record identifier.
    pub record_id: String,
    /// Evidence identifiers backing this memory candidate.
    pub evidence_ids: Vec<String>,
    /// Workspace scope owning the memory record.
    pub workspace_scope: String,
    /// Privacy classification label for the candidate.
    pub privacy: String,
    /// Source text projected for retrieval ranking.
    pub source_text: String,
    /// Ranking score in thousandths.
    pub score_millis: i64,
    /// Freshness timestamp used during ranking.
    pub ranking_freshness: u64,
    /// Freshness timestamp used when confirming citations.
    pub citation_freshness: u64,
}

/// Verified citation binding a memory record to an evidence item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCitation {
    /// Cited memory record identifier.
    pub record_id: String,
    /// Cited evidence identifier.
    pub evidence_id: String,
    /// Lifecycle status of the citation.
    pub status: CitationStatus,
}

/// Lifecycle state of a memory citation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationStatus {
    /// Citation matches the current generation.
    Valid,
    /// Citation was refreshed from newer evidence.
    Updated,
    /// Citation references an outdated generation.
    Stale,
}

/// Errors produced during memory candidate ranking and citation validation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryRetrievalError {
    /// Candidate does not contain the requested evidence identifier.
    #[error("memory candidate has no resolvable evidence")]
    UnknownEvidence,
    /// Candidate belongs to a different workspace scope or misses required fields.
    #[error("memory candidate belongs to another workspace scope")]
    CrossScope,
    /// Candidate generation does not match the current generation.
    #[error("memory candidate is stale for citation confirmation")]
    StaleCitation,
    /// Ranking input exceeds bounds or specifies an empty workspace scope.
    #[error("memory retrieval input is invalid")]
    InvalidInput,
}

/// Deterministically ranks bounded memory candidates within a workspace scope.
pub fn rank_candidates(
    mut candidates: Vec<MemoryRetrievalCandidate>,
    workspace_scope: &str,
    now: u64,
) -> Result<Vec<MemoryRetrievalCandidate>, MemoryRetrievalError> {
    if workspace_scope.is_empty() || candidates.len() > MAX_MEMORY_CANDIDATES {
        return Err(MemoryRetrievalError::InvalidInput);
    }
    if candidates.iter().any(|candidate| {
        candidate.workspace_scope != workspace_scope
            || candidate.record_id.is_empty()
            || candidate.source_text.is_empty()
            || candidate.evidence_ids.is_empty()
    }) {
        return Err(MemoryRetrievalError::CrossScope);
    }
    for candidate in &mut candidates {
        candidate.citation_freshness = candidate.citation_freshness.min(now);
    }
    candidates.sort_by(|left, right| {
        right
            .score_millis
            .cmp(&left.score_millis)
            .then_with(|| right.ranking_freshness.cmp(&left.ranking_freshness))
            .then_with(|| left.record_id.as_bytes().cmp(right.record_id.as_bytes()))
    });
    Ok(candidates)
}

/// Validates a memory citation against its candidate evidence list and generation.
pub fn citation(
    candidate: &MemoryRetrievalCandidate,
    evidence_id: &str,
    current_generation: u64,
    candidate_generation: u64,
) -> Result<MemoryCitation, MemoryRetrievalError> {
    if !candidate.evidence_ids.iter().any(|id| id == evidence_id) {
        return Err(MemoryRetrievalError::UnknownEvidence);
    }
    if candidate_generation != current_generation {
        return Err(MemoryRetrievalError::StaleCitation);
    }
    Ok(MemoryCitation {
        record_id: candidate.record_id.clone(),
        evidence_id: evidence_id.into(),
        status: CitationStatus::Valid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, score: i64, freshness: u64) -> MemoryRetrievalCandidate {
        MemoryRetrievalCandidate {
            record_id: id.into(),
            evidence_ids: vec![format!("evidence-{id}")],
            workspace_scope: "workspace-a".into(),
            privacy: "private".into(),
            source_text: id.into(),
            score_millis: score,
            ranking_freshness: freshness,
            citation_freshness: freshness,
        }
    }

    #[test]
    fn deterministic_order_is_score_then_freshness_then_id() {
        let result = rank_candidates(
            vec![candidate("b", 10, 1), candidate("a", 10, 2)],
            "workspace-a",
            10,
        )
        .expect("rank");
        assert_eq!(result[0].record_id, "a");
    }

    #[test]
    fn citations_reject_unknown_evidence_and_stale_generation() {
        let item = candidate("a", 1, 1);
        assert_eq!(
            citation(&item, "missing", 1, 1),
            Err(MemoryRetrievalError::UnknownEvidence)
        );
        assert_eq!(
            citation(&item, "evidence-a", 2, 1),
            Err(MemoryRetrievalError::StaleCitation)
        );
    }
}
