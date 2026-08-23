//! Memory-to-RAG adapter. Memory candidates keep their own citation identity;
//! they are never represented as workspace document chunks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_MEMORY_CANDIDATES: usize = 64;
pub const MAX_MEMORY_REASONS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRetrievalCandidate {
    pub record_id: String,
    pub evidence_ids: Vec<String>,
    pub workspace_scope: String,
    pub privacy: String,
    pub source_text: String,
    pub score_millis: i64,
    pub ranking_freshness: u64,
    pub citation_freshness: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryCitation {
    pub record_id: String,
    pub evidence_id: String,
    pub status: CitationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationStatus {
    Valid,
    Updated,
    Stale,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MemoryRetrievalError {
    #[error("memory candidate has no resolvable evidence")]
    UnknownEvidence,
    #[error("memory candidate belongs to another workspace scope")]
    CrossScope,
    #[error("memory candidate is stale for citation confirmation")]
    StaleCitation,
    #[error("memory retrieval input is invalid")]
    InvalidInput,
}

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
