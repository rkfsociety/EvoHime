use crate::normalize::normalize_content;
use crate::service::ExistingMemory;
use uuid::Uuid;

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
    let needle = content_fingerprint(candidate_content);
    if needle.is_empty() {
        return None;
    }
    existing.iter().find_map(|item| {
        if content_fingerprint(&item.content) == needle {
            Some(DuplicateHit {
                existing_id: item.id,
            })
        } else {
            None
        }
    })
}
