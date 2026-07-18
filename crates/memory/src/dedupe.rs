use crate::normalize::normalize_content;
use crate::service::ExistingMemory;
use crate::{cosine_similarity, embedding_version};
use evohime_storage::MemoryKind;
use uuid::Uuid;

/// Conservative cosine threshold for semantic duplicate admission.
pub const SEMANTIC_DEDUPE_MIN_COSINE: f64 = 0.58;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateHit {
    pub existing_id: Uuid,
}

/// Stable fingerprint for dedupe (normalized + lowercased).
pub fn content_fingerprint(content: &str) -> String {
    normalize_content(content).to_ascii_lowercase()
}

pub fn detect_duplicate(
    candidate_content: &str,
    existing: &[ExistingMemory],
) -> Option<DuplicateHit> {
    detect_duplicate_with_embedding(candidate_content, None, None, existing)
}

pub fn detect_duplicate_with_embedding(
    candidate_content: &str,
    candidate_kind: Option<MemoryKind>,
    candidate_embedding: Option<&[f32]>,
    existing: &[ExistingMemory],
) -> Option<DuplicateHit> {
    let needle = content_fingerprint(candidate_content);
    if needle.is_empty() {
        return None;
    }
    if let Some(hit) = existing.iter().find_map(|item| {
        if candidate_kind.is_some_and(|kind| item.kind != kind) {
            return None;
        }
        if content_fingerprint(&item.content) == needle {
            Some(DuplicateHit {
                existing_id: item.id,
            })
        } else {
            None
        }
    }) {
        return Some(hit);
    }

    let Some(candidate_embedding) = candidate_embedding else {
        return None;
    };
    let active_version = embedding_version();
    existing.iter().find_map(|item| {
        if candidate_kind.is_some_and(|kind| item.kind != kind) {
            return None;
        }
        if item.embedding_version != active_version {
            return None;
        }
        let cosine = cosine_similarity(candidate_embedding, item.embedding.as_deref()?);
        (cosine >= SEMANTIC_DEDUPE_MIN_COSINE).then_some(DuplicateHit {
            existing_id: item.id,
        })
    })
}
