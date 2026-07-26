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

/// Deterministic embedding generation for semantic search (Phase 2 MVP).
///
/// Uses character-frequency and structure-based features for 384-dim embeddings.
/// Phase 3 will replace with real model (all-MiniLM-L6-v2 via ONNX).
pub struct EmbeddingGenerator;

impl EmbeddingGenerator {
    /// Generate a deterministic 384-dim embedding from chunk text.
    ///
    /// Features (Phase 2 MVP):
    /// - Character frequency distribution (256 dim)
    /// - Token-like patterns (50 dim)
    /// - Structural features (40 dim)
    /// - Semantic patterns (38 dim)
    pub fn generate_embedding(text: &str) -> Vec<f32> {
        let mut embedding = vec![0.0; 384];

        // 1. Character frequency (256 dim)
        let mut char_freq = [0usize; 256];
        for byte in text.as_bytes() {
            char_freq[*byte as usize] += 1;
        }
        let max_freq = char_freq.iter().max().copied().unwrap_or(1).max(1) as f32;
        for i in 0..256 {
            embedding[i] = (char_freq[i] as f32) / max_freq;
        }

        // 2. Token-like patterns (50 dim) - snake_case, camelCase, CONSTANT, etc.
        let snake_case = text.matches('_').count() as f32;
        let camel_case = text.chars().filter(|c| c.is_uppercase()).count() as f32;
        let numbers = text.chars().filter(|c| c.is_numeric()).count() as f32;
        let symbols = text.chars().filter(|c| !c.is_alphanumeric() && !c.is_whitespace()).count() as f32;

        embedding[256] = (snake_case / (text.len().max(1) as f32)).min(1.0);
        embedding[257] = (camel_case / (text.len().max(1) as f32)).min(1.0);
        embedding[258] = (numbers / (text.len().max(1) as f32)).min(1.0);
        embedding[259] = (symbols / (text.len().max(1) as f32)).min(1.0);

        // Fill remaining token pattern dims
        for i in 260..306 {
            embedding[i] = Self::pattern_hash_component(text, i - 260) as f32;
        }

        // 3. Structural features (40 dim)
        let lines = text.lines().count() as f32;
        let avg_line_len = text.lines().map(|l| l.len()).sum::<usize>() as f32 / lines.max(1.0);
        let has_fn = (text.contains("fn ") || text.contains("def ") || text.contains("function")) as i32 as f32;
        let has_class = (text.contains("class ") || text.contains("struct ") || text.contains("impl ")) as i32 as f32;
        let has_comment = (text.contains("//") || text.contains("/*") || text.contains("#")) as i32 as f32;

        embedding[306] = (lines / (text.len().max(1) as f32)).min(1.0);
        embedding[307] = (avg_line_len / 100.0).min(1.0);
        embedding[308] = has_fn;
        embedding[309] = has_class;
        embedding[310] = has_comment;

        // Fill remaining structural dims
        for i in 311..346 {
            embedding[i] = Self::structure_hash_component(text, i - 311) as f32;
        }

        // 4. Semantic patterns (38 dim)
        let keywords = [
            ("async", text.matches("async").count()),
            ("await", text.matches("await").count()),
            ("error", text.matches("error").count()),
            ("result", text.matches("result").count()),
            ("option", text.matches("option").count()),
            ("vec", text.matches("vec").count()),
            ("map", text.matches("map").count()),
            ("filter", text.matches("filter").count()),
            ("loop", text.matches("loop").count()),
            ("if", text.matches("if ").count()),
            ("match", text.matches("match").count()),
            ("return", text.matches("return").count()),
        ];

        for (i, (_, count)) in keywords.iter().enumerate().take(12) {
            embedding[346 + i] = (*count as f32 / (text.len().max(1) as f32)).min(1.0);
        }

        // Fill remaining semantic dims
        for i in 358..384 {
            embedding[i] = Self::semantic_hash_component(text, i - 358) as f32;
        }

        // Normalize to unit vector for cosine similarity
        Self::normalize_vector(&mut embedding);
        embedding
    }

    /// Normalize vector to unit length for cosine similarity.
    fn normalize_vector(vec: &mut Vec<f32>) {
        let magnitude = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for x in vec.iter_mut() {
                *x /= magnitude;
            }
        }
    }

    /// Compute pattern hash component (deterministic).
    fn pattern_hash_component(text: &str, component: usize) -> f32 {
        let seed = component as u32;
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", text, seed).as_bytes());
        let hash = hasher.finalize();
        ((hash[0] as f32) / 255.0).min(1.0)
    }

    /// Compute structure hash component (deterministic).
    fn structure_hash_component(text: &str, component: usize) -> f32 {
        let seed = component as u32 + 100;
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", text, seed).as_bytes());
        let hash = hasher.finalize();
        ((hash[0] as f32) / 255.0).min(1.0)
    }

    /// Compute semantic hash component (deterministic).
    fn semantic_hash_component(text: &str, component: usize) -> f32 {
        let seed = component as u32 + 200;
        let mut hasher = Sha256::new();
        hasher.update(format!("{}:{}", text, seed).as_bytes());
        let hash = hasher.finalize();
        ((hash[0] as f32) / 255.0).min(1.0)
    }

    /// Compute cosine similarity between two embeddings.
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }
}

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

    /// Create embedding with generated vector (Phase 2)
    pub fn new(
        file_path: PathBuf,
        line: usize,
        end_line: usize,
        chunk_text: String,
    ) -> Self {
        let vector = EmbeddingGenerator::generate_embedding(&chunk_text);
        Self {
            chunk_hash: Self::hash_chunk(&chunk_text),
            file_path,
            line,
            end_line,
            chunk_text,
            vector,
            model_version: "v2-deterministic-384d".to_string(),
        }
    }

    /// Create embedding with placeholder vector (for testing)
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

    #[test]
    fn embedding_generator_deterministic() {
        let text = "fn hello() { println!(\"world\"); }";
        let emb1 = EmbeddingGenerator::generate_embedding(text);
        let emb2 = EmbeddingGenerator::generate_embedding(text);
        assert_eq!(emb1, emb2);
    }

    #[test]
    fn embedding_vector_normalized() {
        let text = "fn example() {}";
        let vec = EmbeddingGenerator::generate_embedding(text);

        // Vector should be normalized (magnitude ~= 1.0)
        let magnitude = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((magnitude - 1.0).abs() < 0.01, "Magnitude: {}", magnitude);
    }

    #[test]
    fn cosine_similarity_same_text() {
        let text = "async fn fetch_data() {}";
        let emb = EmbeddingGenerator::generate_embedding(text);
        let similarity = EmbeddingGenerator::cosine_similarity(&emb, &emb);
        assert!((similarity - 1.0).abs() < 0.01, "Same text similarity: {}", similarity);
    }

    #[test]
    fn cosine_similarity_different_text() {
        let text1 = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let text2 = "fn multiply(x: f64, y: f64) -> f64 { x * y }";
        let emb1 = EmbeddingGenerator::generate_embedding(text1);
        let emb2 = EmbeddingGenerator::generate_embedding(text2);
        let similarity = EmbeddingGenerator::cosine_similarity(&emb1, &emb2);

        // Different code should have lower (but nonzero) similarity
        assert!(similarity < 1.0);
        assert!(similarity > 0.0);
    }

    #[test]
    fn semantic_match_with_combined() {
        let path = PathBuf::from("test.rs");
        let m = SemanticMatch::with_combined(
            path.clone(),
            10,
            15,
            "code snippet".to_string(),
            75,
            0.9,
        );

        assert_eq!(m.file_path, path);
        assert_eq!(m.line, 10);
        assert_eq!(m.end_line, 15);
        assert_eq!(m.lexical_score, 75);
        assert!((m.semantic_score - 0.9).abs() < 0.01);
        // 0.4 * (75/100) + 0.6 * 0.9 = 0.3 + 0.54 = 0.84
        assert!((m.combined_score - 0.84).abs() < 0.01);
    }
}
