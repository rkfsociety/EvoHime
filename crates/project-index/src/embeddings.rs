//! Wave 4: Semantic embeddings for code context (7.112).
//!
//! Provides embedding-based semantic search to complement lexical search.
//! - Chunk embeddings cached locally
//! - Hash-based deduplication
//! - Hybrid BM25 + semantic scoring

use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Semantic embedding for a code chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embedding {
    /// SHA256 hash of chunk text (for dedup)
    pub chunk_hash: String,
    /// Path to file containing this chunk
    pub file_path: PathBuf,
    /// 1-based start line
    pub line: usize,
    /// 1-based end line
    pub end_line: usize,
    /// Chunk text for re-embedding if model changes
    pub chunk_text: String,
    /// Embedding vector (384-dim for all-MiniLM-L6-v2)
    /// Stored as JSON for now; could be binary for performance
    pub vector: Vec<f32>,
    /// Model version hash for compatibility
    pub model_version: String,
}

impl Embedding {
    /// Calculate chunk hash for deduplication
    pub fn hash_chunk(text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Create embedding with placeholder vector (Phase 1 MVP)
    pub fn placeholder(
        file_path: PathBuf,
        line: usize,
        end_line: usize,
        chunk_text: String,
    ) -> Self {
        Self {
            chunk_hash: Self::hash_chunk(&chunk_text),
            file_path,
            line,
            end_line,
            chunk_text,
            // Placeholder vector (will be replaced in Phase 2)
            vector: vec![0.0; 384],
            model_version: "v1-placeholder".to_string(),
        }
    }
}

/// Cache for embeddings with deduplication.
#[derive(Debug, Default, Clone)]
pub struct EmbeddingCache {
    /// Map from chunk_hash to embedding
    cache: HashMap<String, Embedding>,
}

impl EmbeddingCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add embedding to cache (replaces if hash exists)
    pub fn insert(&mut self, embedding: Embedding) {
        self.cache.insert(embedding.chunk_hash.clone(), embedding);
    }

    /// Get embedding by chunk hash
    pub fn get(&self, chunk_hash: &str) -> Option<&Embedding> {
        self.cache.get(chunk_hash)
    }

    /// Get all cached embeddings
    pub fn all(&self) -> Vec<&Embedding> {
        self.cache.values().collect()
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            total_embeddings: self.cache.len(),
            unique_chunks: self.cache.len(),
            approx_memory_bytes: self.cache.len() * (384 * 4 + 256), // rough estimate
        }
    }
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_embeddings: usize,
    pub unique_chunks: usize,
    pub approx_memory_bytes: usize,
}

/// Semantic search result combining lexical and semantic scores.
#[derive(Debug, Clone)]
pub struct SemanticMatch {
    pub file_path: PathBuf,
    pub line: usize,
    pub end_line: usize,
    pub snippet: String,
    pub lexical_score: u32,
    pub semantic_score: f32,  // cosine similarity 0.0-1.0
    pub combined_score: f32,  // weighted combination
}

impl SemanticMatch {
    /// Compute combined score from lexical and semantic components.
    /// Formula: 0.4 * lexical_norm + 0.6 * semantic_score
    pub fn combine_scores(lexical_score: u32, semantic_score: f32) -> f32 {
        let lexical_norm = (lexical_score as f32) / 100.0; // normalize to 0-1 range
        0.4 * lexical_norm.min(1.0) + 0.6 * semantic_score.max(0.0).min(1.0)
    }

    pub fn with_combined(
        file_path: PathBuf,
        line: usize,
        end_line: usize,
        snippet: String,
        lexical_score: u32,
        semantic_score: f32,
    ) -> Self {
        let combined_score = Self::combine_scores(lexical_score, semantic_score);
        Self {
            file_path,
            line,
            end_line,
            snippet,
            lexical_score,
            semantic_score,
            combined_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_hash_deterministic() {
        let text = "fn hello() { println!(\"world\"); }";
        let hash1 = Embedding::hash_chunk(text);
        let hash2 = Embedding::hash_chunk(text);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn chunk_hash_unique() {
        let text1 = "fn hello() {}";
        let text2 = "fn world() {}";
        assert_ne!(
            Embedding::hash_chunk(text1),
            Embedding::hash_chunk(text2)
        );
    }

    #[test]
    fn embedding_cache_dedup() {
        let mut cache = EmbeddingCache::new();
        let embedding = Embedding::placeholder(
            PathBuf::from("test.rs"),
            1,
            5,
            "fn test() {}".to_string(),
        );
        let hash = embedding.chunk_hash.clone();

        cache.insert(embedding);
        cache.insert(Embedding::placeholder(
            PathBuf::from("test.rs"),
            10,
            15,
            "fn test() {}".to_string(), // Same text = same hash
        ));

        // Should have only 1 unique chunk due to dedup
        assert_eq!(cache.stats().unique_chunks, 1);
        assert!(cache.get(&hash).is_some());
    }

    #[test]
    fn combine_scores() {
        let combined = SemanticMatch::combine_scores(50, 0.8);
        // 0.4 * (50/100) + 0.6 * 0.8 = 0.2 + 0.48 = 0.68
        assert!((combined - 0.68).abs() < 0.01);
    }
}
