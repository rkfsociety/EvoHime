//! Local feature-hash embeddings for hybrid memory retrieval (6.25).
//!
//! Deterministic, dependency-free encoder so CI and offline installs stay simple.
//! Swap for a neural encoder later by bumping [`EMBEDDING_VERSION`].

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Fixed embedding dimensionality.
pub const EMBEDDING_DIM: usize = 96;

/// Bump whenever the hash pipeline changes (forces re-embed).
pub const EMBEDDING_VERSION: i32 = 1;

/// Weight of cosine similarity added on top of lexical score.
pub const SEMANTIC_SCORE_WEIGHT: f64 = 2.5;

/// Minimum cosine to treat as a meaningful semantic hit.
pub const SEMANTIC_MIN_COSINE: f64 = 0.08;

/// Embed text into a unit L2 vector via signed feature hashing (tokens + char trigrams).
pub fn embed_text(text: &str) -> Vec<f32> {
    let mut vec = vec![0.0f32; EMBEDDING_DIM];
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return vec;
    }

    for token in tokenize(&normalized) {
        accumulate_feature(&mut vec, &token);
    }

    let compact: String = normalized
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_whitespace())
        .collect();
    let chars: Vec<char> = compact.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() >= 3 {
        for window in chars.windows(3) {
            let gram: String = window.iter().collect();
            accumulate_feature(&mut vec, &format!("#{}", gram));
        }
    }

    l2_normalize(&mut vec);
    vec
}

/// Cosine similarity for unit (or near-unit) vectors; returns 0 for empty/mismatch.
pub fn cosine_similarity(left: &[f32], right: &[f32]) -> f64 {
    if left.is_empty() || right.is_empty() || left.len() != right.len() {
        return 0.0;
    }
    let mut dot = 0.0f64;
    let mut left_norm = 0.0f64;
    let mut right_norm = 0.0f64;
    for (a, b) in left.iter().zip(right.iter()) {
        let a = f64::from(*a);
        let b = f64::from(*b);
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm <= f64::EPSILON || right_norm <= f64::EPSILON {
        return 0.0;
    }
    (dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(-1.0, 1.0)
}

/// Semantic contribution for hybrid ranking.
pub fn semantic_score(query_embedding: &[f32], item_embedding: Option<&[f32]>) -> f64 {
    let Some(item) = item_embedding else {
        return 0.0;
    };
    let cosine = cosine_similarity(query_embedding, item);
    if cosine < SEMANTIC_MIN_COSINE {
        return 0.0;
    }
    cosine * SEMANTIC_SCORE_WEIGHT
}

/// True when stored embedding should be recomputed.
pub fn needs_reembed(version: i32, embedding: Option<&[f32]>) -> bool {
    version != EMBEDDING_VERSION
        || embedding.map(|v| v.len() != EMBEDDING_DIM).unwrap_or(true)
}

fn accumulate_feature(vec: &mut [f32], feature: &str) {
    let mut hasher = DefaultHasher::new();
    feature.hash(&mut hasher);
    let hash = hasher.finish();
    let index = (hash as usize) % EMBEDDING_DIM;
    let sign = if hash & 1 == 0 { 1.0f32 } else { -1.0f32 };
    vec[index] += sign;
}

fn l2_normalize(vec: &mut [f32]) {
    let norm = vec.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>().sqrt();
    if norm <= f64::EPSILON {
        return;
    }
    for value in vec.iter_mut() {
        *value = (f64::from(*value) / norm) as f32;
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .map(|part| part.to_ascii_lowercase())
        .filter(|part| part.len() > 2)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_texts_have_higher_cosine_than_unrelated() {
        let query = embed_text("prefer git worktrees for parallel agents");
        let close = embed_text("use worktrees when running parallel agents");
        let far = embed_text("postgres connection pool size is 16");
        let close_score = cosine_similarity(&query, &close);
        let far_score = cosine_similarity(&query, &far);
        assert!(close_score > far_score);
        assert!(close_score > SEMANTIC_MIN_COSINE);
    }

    #[test]
    fn embed_is_deterministic_and_unit_ish() {
        let a = embed_text("Always pin critical constraints");
        let b = embed_text("Always pin critical constraints");
        assert_eq!(a, b);
        assert_eq!(a.len(), EMBEDDING_DIM);
        let norm = a.iter().map(|v| f64::from(*v) * f64::from(*v)).sum::<f64>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn needs_reembed_detects_stale() {
        let emb = embed_text("hello world memory");
        assert!(!needs_reembed(EMBEDDING_VERSION, Some(&emb)));
        assert!(needs_reembed(0, Some(&emb)));
        assert!(needs_reembed(EMBEDDING_VERSION, None));
        assert!(needs_reembed(EMBEDDING_VERSION, Some(&[0.1, 0.2])));
    }
}
