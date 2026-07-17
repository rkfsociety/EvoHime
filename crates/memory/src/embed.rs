//! Hybrid memory embeddings: local feature-hash (default) + optional remote neural encoder.
//!
//! Remote mode uses an OpenAI-compatible `POST {base}/embeddings` endpoint
//! (`EVOHIME_EMBEDDING_MODE=remote`). Bump [`embedding_version`] / revision when the
//! remote model changes so stored vectors are recomputed.

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;

/// Feature-hash dimensionality (version 1).
pub const EMBEDDING_DIM: usize = 96;

/// Feature-hash encoder version.
pub const HASH_EMBEDDING_VERSION: i32 = 1;

/// Base version for remote neural embeddings (`+ EVOHIME_EMBEDDING_REVISION`).
pub const REMOTE_EMBEDDING_BASE_VERSION: i32 = 2;

/// Weight of cosine similarity added on top of lexical score.
pub const SEMANTIC_SCORE_WEIGHT: f64 = 2.5;

/// Minimum cosine to treat as a meaningful semantic hit.
pub const SEMANTIC_MIN_COSINE: f64 = 0.08;

/// Active encoder version written to `memory_items.embedding_version`.
///
/// - `1` — local feature-hash
/// - `2 + revision` — remote neural (`EVOHIME_EMBEDDING_REVISION`, default 0)
pub fn embedding_version() -> i32 {
    match EncoderConfig::from_env().mode {
        EncoderMode::Hash => HASH_EMBEDDING_VERSION,
        EncoderMode::Remote => {
            let revision = std::env::var("EVOHIME_EMBEDDING_REVISION")
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .unwrap_or(0)
                .max(0);
            REMOTE_EMBEDDING_BASE_VERSION.saturating_add(revision)
        }
    }
}

/// Backward-compatible alias used by older call sites / docs.
#[allow(non_upper_case_globals)]
pub const EMBEDDING_VERSION: i32 = HASH_EMBEDDING_VERSION;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderMode {
    Hash,
    Remote,
}

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub mode: EncoderMode,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl EncoderConfig {
    pub fn from_env() -> Self {
        let mode = match std::env::var("EVOHIME_EMBEDDING_MODE")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "remote" | "neural" | "api" => EncoderMode::Remote,
            _ => EncoderMode::Hash,
        };

        let base_url = std::env::var("EVOHIME_EMBEDDING_BASE_URL")
            .or_else(|_| std::env::var("LITEROUTER_BASE_URL"))
            .unwrap_or_else(|_| "https://api.literouter.com/v1".into());
        let api_key = std::env::var("EVOHIME_EMBEDDING_API_KEY")
            .or_else(|_| std::env::var("LITEROUTER_API_KEY"))
            .unwrap_or_default();
        let model = std::env::var("EVOHIME_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-small".into());

        Self {
            mode,
            base_url,
            api_key,
            model,
        }
    }

    pub fn embeddings_url(&self) -> String {
        let trimmed = self.base_url.trim_end_matches('/');
        if trimmed.ends_with("/embeddings") {
            trimmed.to_string()
        } else {
            format!("{trimmed}/embeddings")
        }
    }

    pub fn remote_ready(&self) -> bool {
        self.mode == EncoderMode::Remote
            && !self.api_key.trim().is_empty()
            && !self.base_url.trim().is_empty()
            && !self.model.trim().is_empty()
    }
}

/// Result of an embedding call (vector + version to persist).
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddingResult {
    pub vector: Vec<f32>,
    pub version: i32,
}

/// Embed text with the active encoder (hash by default; remote when configured).
///
/// On remote failure returns an empty vector with `version = 0` so retrieval can
/// retry later without mixing hash and neural spaces in the same index.
pub async fn embed_text(text: &str) -> EmbeddingResult {
    let config = EncoderConfig::from_env();
    if config.remote_ready() {
        match embed_text_remote(text, &config).await {
            Ok(vector) if !vector.is_empty() => {
                return EmbeddingResult {
                    vector,
                    version: embedding_version(),
                };
            }
            Ok(_) => {
                tracing::warn!("remote embedding returned empty vector; deferring");
            }
            Err(error) => {
                tracing::warn!(%error, "remote embedding failed; deferring");
            }
        }
        return EmbeddingResult {
            vector: Vec::new(),
            version: 0,
        };
    }
    EmbeddingResult {
        vector: embed_text_hash(text),
        version: HASH_EMBEDDING_VERSION,
    }
}

/// Local feature-hash encoder (deterministic, offline, CI-safe).
pub fn embed_text_hash(text: &str) -> Vec<f32> {
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

async fn embed_text_remote(text: &str, config: &EncoderConfig) -> Result<Vec<f32>, String> {
    let client = http_client();
    let body = serde_json::json!({
        "model": config.model,
        "input": text,
    });
    let response = client
        .post(config.embeddings_url())
        .bearer_auth(&config.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    let status = response.status();
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    if !status.is_success() {
        let snippet = String::from_utf8_lossy(&bytes);
        return Err(format!("embeddings HTTP {status}: {snippet}"));
    }
    let parsed: EmbeddingsResponse =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let mut vector = parsed
        .data
        .into_iter()
        .min_by_key(|item| item.index)
        .map(|item| item.embedding)
        .ok_or_else(|| "embeddings response missing data[]".to_string())?;
    l2_normalize(&mut vector);
    Ok(vector)
}

fn http_client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client")
    })
}

#[derive(Debug, Deserialize)]
struct EmbeddingsResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    #[serde(default)]
    index: u32,
    embedding: Vec<f32>,
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

/// True when stored embedding should be recomputed for the active encoder.
pub fn needs_reembed(version: i32, embedding: Option<&[f32]>) -> bool {
    let active = embedding_version();
    if version != active {
        return true;
    }
    match embedding {
        None | Some([]) => true,
        Some(vector) if active == HASH_EMBEDDING_VERSION && vector.len() != EMBEDDING_DIM => true,
        Some(_) => false,
    }
}

fn accumulate_feature(vec: &mut [f32], feature: &str) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    feature.hash(&mut hasher);
    let hash = hasher.finish();
    let index = (hash as usize) % EMBEDDING_DIM;
    let sign = if hash & 1 == 0 { 1.0f32 } else { -1.0f32 };
    vec[index] += sign;
}

fn l2_normalize(vec: &mut [f32]) {
    let norm = vec
        .iter()
        .map(|v| f64::from(*v) * f64::from(*v))
        .sum::<f64>()
        .sqrt();
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
        let query = embed_text_hash("prefer git worktrees for parallel agents");
        let close = embed_text_hash("use worktrees when running parallel agents");
        let far = embed_text_hash("postgres connection pool size is 16");
        let close_score = cosine_similarity(&query, &close);
        let far_score = cosine_similarity(&query, &far);
        assert!(close_score > far_score);
        assert!(close_score > SEMANTIC_MIN_COSINE);
    }

    #[test]
    fn embed_is_deterministic_and_unit_ish() {
        let a = embed_text_hash("Always pin critical constraints");
        let b = embed_text_hash("Always pin critical constraints");
        assert_eq!(a, b);
        assert_eq!(a.len(), EMBEDDING_DIM);
        let norm = a
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn needs_reembed_detects_stale() {
        let emb = embed_text_hash("hello world memory");
        assert!(!needs_reembed(HASH_EMBEDDING_VERSION, Some(&emb)));
        assert!(needs_reembed(0, Some(&emb)));
        assert!(needs_reembed(HASH_EMBEDDING_VERSION, None));
        assert!(needs_reembed(HASH_EMBEDDING_VERSION, Some(&[0.1, 0.2])));
    }

    #[test]
    fn embeddings_url_appends_path() {
        let config = EncoderConfig {
            mode: EncoderMode::Remote,
            base_url: "https://api.example.com/v1/".into(),
            api_key: "k".into(),
            model: "m".into(),
        };
        assert_eq!(
            config.embeddings_url(),
            "https://api.example.com/v1/embeddings"
        );
    }

    #[tokio::test]
    async fn remote_embed_parses_openai_shape() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/embeddings"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{
                    "index": 0,
                    "embedding": [0.0, 3.0, 4.0]
                }]
            })))
            .mount(&server)
            .await;

        let config = EncoderConfig {
            mode: EncoderMode::Remote,
            base_url: format!("{}/v1", server.uri()),
            api_key: "test-key".into(),
            model: "mock-embed".into(),
        };
        let vector = embed_text_remote("hello", &config)
            .await
            .expect("remote embed");
        assert_eq!(vector.len(), 3);
        let norm = vector
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum::<f64>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
        assert!((vector[1] - 0.6).abs() < 1e-5);
        assert!((vector[2] - 0.8).abs() < 1e-5);
    }
}
