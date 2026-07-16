use crate::dedupe::content_fingerprint;
use crate::service::ExistingMemory;
use evohime_storage::MemoryKind;
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictHit {
    pub existing_id: Uuid,
    pub reason: String,
}

const OPPOSITE_PAIRS: &[(&str, &str)] = &[
    ("always", "never"),
    ("never", "always"),
    ("do", "don't"),
    ("don't", "do"),
    ("enable", "disable"),
    ("disable", "enable"),
    ("allow", "deny"),
    ("deny", "allow"),
    ("must", "must not"),
    ("must not", "must"),
];

pub fn detect_conflict(
    kind: MemoryKind,
    candidate_content: &str,
    existing: &[ExistingMemory],
) -> Option<ConflictHit> {
    let candidate_fp = content_fingerprint(candidate_content);
    if candidate_fp.is_empty() {
        return None;
    }

    for item in existing {
        if item.kind != kind {
            continue;
        }
        let existing_fp = content_fingerprint(&item.content);
        if existing_fp.is_empty() || existing_fp == candidate_fp {
            continue;
        }

        if opposing_polarity(&candidate_fp, &existing_fp) && topic_overlap(&candidate_fp, &existing_fp) {
            return Some(ConflictHit {
                existing_id: item.id,
                reason: "opposing polarity on overlapping topic".into(),
            });
        }
    }
    None
}

fn opposing_polarity(a: &str, b: &str) -> bool {
    for (left, right) in OPPOSITE_PAIRS {
        let a_has_left = contains_phrase(a, left);
        let a_has_right = contains_phrase(a, right);
        let b_has_left = contains_phrase(b, left);
        let b_has_right = contains_phrase(b, right);
        if (a_has_left && b_has_right) || (a_has_right && b_has_left) {
            return true;
        }
    }
    false
}

fn contains_phrase(haystack: &str, phrase: &str) -> bool {
    let padded = format!(" {haystack} ");
    let needle = format!(" {phrase} ");
    padded.contains(&needle)
}

fn topic_overlap(a: &str, b: &str) -> bool {
    let stop: HashSet<&str> = [
        "a", "an", "the", "to", "for", "of", "and", "or", "in", "on", "at", "be", "is", "are",
        "always", "never", "do", "don't", "enable", "disable", "allow", "deny", "must", "not",
        "run", "use", "with", "before", "after",
    ]
    .into_iter()
    .collect();

    let tokens = |text: &str| -> HashSet<String> {
        text.split_whitespace()
            .map(|t| t.trim_matches(|c: char| !c.is_ascii_alphanumeric()).to_ascii_lowercase())
            .filter(|t| t.len() > 2 && !stop.contains(t.as_str()))
            .collect()
    };

    let left = tokens(a);
    let right = tokens(b);
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let overlap = left.intersection(&right).count();
    let union = left.union(&right).count().max(1);
    (overlap as f64) / (union as f64) >= 0.34
}
