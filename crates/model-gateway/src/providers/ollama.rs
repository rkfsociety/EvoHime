//! Ollama provider and device-aware local model catalogue.

use super::{ChatFuture, ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream};
use crate::config::LiteRouterConfig;
use crate::providers::literouter::LiteRouterProvider;
use crate::retry::RetryPolicy;
use crate::tools::ToolSpec;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::time::Duration;

pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:11434/v1";
pub const MAX_PULL_MODEL_CHARS: usize = 128;
pub const MAX_RECOMMENDATIONS: usize = 32;

#[derive(Debug)]
pub struct OllamaProvider {
    inner: LiteRouterProvider,
}

impl OllamaProvider {
    pub fn new(config: LiteRouterConfig) -> Result<Self, ProviderError> {
        validate_loopback(&config.base_url)?;
        Ok(Self {
            inner: LiteRouterProvider::without_auth(config, RetryPolicy::from_env())?,
        })
    }
}

impl ModelProvider for OllamaProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Ollama
    }

    fn model_name(&self) -> &str {
        self.inner.model_name()
    }

    fn base_url(&self) -> &str {
        self.inner.base_url()
    }

    fn stream_chat(&self, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat(messages)
    }

    fn stream_chat_with_model(&self, model: &str, messages: &[ChatMessage]) -> TokenStream {
        self.inner.stream_chat_with_model(model, messages)
    }

    fn chat_with_tools(
        &self,
        model: Option<&str>,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> ChatFuture {
        self.inner.chat_with_tools(model, messages, tools)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OllamaDeviceProfile {
    pub cpu_threads: u16,
    pub ram_bytes: u64,
    pub disk_free_bytes: u64,
    pub accelerator_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OllamaModelRecommendation {
    pub id: String,
    pub description: String,
    pub size_bytes: u64,
    pub required_ram_bytes: u64,
    pub required_vram_bytes: Option<u64>,
    pub fits_device: bool,
    pub installed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct OllamaCatalog {
    pub installed: Vec<crate::ModelCatalogEntry>,
    pub recommendations: Vec<OllamaModelRecommendation>,
    pub device: OllamaDeviceProfile,
}

#[derive(Debug, Clone, Copy)]
struct CuratedModel {
    id: &'static str,
    description: &'static str,
    size_bytes: u64,
    required_ram_bytes: u64,
    required_vram_bytes: Option<u64>,
    min_cpu_threads: u16,
}

const GIB: u64 = 1024 * 1024 * 1024;

// Conservative Q4-sized estimates used only for download advice. Ollama
// remains the source of truth for the actual manifest and final size.
const CURATED_MODELS: &[CuratedModel] = &[
    CuratedModel {
        id: "qwen3:0.6b",
        description: "быстрая базовая модель",
        size_bytes: 522 * 1024 * 1024,
        required_ram_bytes: 2 * GIB,
        required_vram_bytes: Some(GIB),
        min_cpu_threads: 2,
    },
    CuratedModel {
        id: "qwen3:1.7b",
        description: "компактный универсальный вариант",
        size_bytes: 1_400 * 1024 * 1024,
        required_ram_bytes: 3 * GIB,
        required_vram_bytes: Some(2 * GIB),
        min_cpu_threads: 2,
    },
    CuratedModel {
        id: "llama3.2:3b",
        description: "сбалансированная компактная модель",
        size_bytes: 2 * GIB,
        required_ram_bytes: 5 * GIB,
        required_vram_bytes: Some(3 * GIB),
        min_cpu_threads: 4,
    },
    CuratedModel {
        id: "qwen3:4b",
        description: "рекомендуемый общий вариант",
        size_bytes: 2_500 * 1024 * 1024,
        required_ram_bytes: 6 * GIB,
        required_vram_bytes: Some(4 * GIB),
        min_cpu_threads: 4,
    },
    CuratedModel {
        id: "gemma3:4b",
        description: "компактная мультимодальная модель",
        size_bytes: 3_300 * 1024 * 1024,
        required_ram_bytes: 7 * GIB,
        required_vram_bytes: Some(4 * GIB),
        min_cpu_threads: 4,
    },
    CuratedModel {
        id: "qwen3:8b",
        description: "более сильная модель для рабочих задач",
        size_bytes: 5 * GIB,
        required_ram_bytes: 10 * GIB,
        required_vram_bytes: Some(6 * GIB),
        min_cpu_threads: 6,
    },
    CuratedModel {
        id: "gemma3:12b",
        description: "тяжёлый вариант для мощных ПК",
        size_bytes: 8 * GIB,
        required_ram_bytes: 14 * GIB,
        required_vram_bytes: Some(10 * GIB),
        min_cpu_threads: 8,
    },
    CuratedModel {
        id: "qwen3:14b",
        description: "максимум качества в среднем классе",
        size_bytes: 9 * GIB,
        required_ram_bytes: 16 * GIB,
        required_vram_bytes: Some(12 * GIB),
        min_cpu_threads: 8,
    },
];

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<InstalledModel>,
}

#[derive(Debug, Deserialize)]
struct InstalledModel {
    #[serde(default)]
    name: String,
    #[serde(default)]
    model: String,
}

#[derive(Debug, Deserialize)]
struct PullProgress {
    #[serde(default)]
    status: String,
    #[serde(default)]
    error: Option<String>,
}

pub fn validate_loopback(base_url: &str) -> Result<(), ProviderError> {
    let url = reqwest::Url::parse(base_url)
        .map_err(|_| ProviderError::Config("Ollama URL must be loopback HTTP".into()))?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "http" || !matches!(host, "localhost" | "127.0.0.1" | "::1") {
        return Err(ProviderError::Config(
            "Ollama URL must be loopback HTTP".into(),
        ));
    }
    Ok(())
}

fn native_base_url(base_url: &str) -> Result<String, ProviderError> {
    validate_loopback(base_url)?;
    Ok(base_url
        .trim_end_matches('/')
        .strip_suffix("/v1")
        .unwrap_or(base_url.trim_end_matches('/'))
        .to_string())
}

pub async fn fetch_installed_models(
    config: &LiteRouterConfig,
) -> Result<Vec<crate::ModelCatalogEntry>, ProviderError> {
    let base_url = native_base_url(&config.base_url)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    let response = client
        .get(format!("{base_url}/api/tags"))
        .send()
        .await
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::Api(format!("{status}: {body}")));
    }
    let payload = response
        .json::<TagsResponse>()
        .await
        .map_err(|error| ProviderError::Api(error.to_string()))?;
    let mut models: Vec<_> = payload
        .models
        .into_iter()
        .filter_map(|model| {
            let id = if model.name.trim().is_empty() {
                model.model
            } else {
                model.name
            };
            (!id.trim().is_empty()).then_some(crate::ModelCatalogEntry {
                id,
                context_tokens: None,
                max_output_tokens: None,
            })
        })
        .collect();
    models.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

pub fn recommend_models(
    device: &OllamaDeviceProfile,
    installed: &[crate::ModelCatalogEntry],
) -> Vec<OllamaModelRecommendation> {
    let installed = installed
        .iter()
        .map(|model| model.id.as_str())
        .collect::<BTreeSet<_>>();
    let safe_ram = device.ram_bytes.saturating_mul(70) / 100;
    CURATED_MODELS
        .iter()
        .take(MAX_RECOMMENDATIONS)
        .filter_map(|model| {
            let safe_vram = device
                .accelerator_bytes
                .map(|bytes| bytes.saturating_mul(80) / 100);
            let enough_ram = safe_ram >= model.required_ram_bytes;
            let enough_vram = model
                .required_vram_bytes
                .is_some_and(|required| safe_vram.is_some_and(|available| available >= required));
            let enough_disk = device.disk_free_bytes >= model.size_bytes.saturating_mul(11) / 10;
            let enough_cpu = device.cpu_threads >= model.min_cpu_threads;
            // With a detected accelerator, require both memory domains to fit.
            // This keeps the download list limited to models that can start
            // with the advertised GPU path instead of silently falling back
            // to a much slower CPU-only run.
            let fits_device = if device.accelerator_bytes.is_some() {
                enough_ram && enough_vram && enough_disk && enough_cpu
            } else {
                enough_ram && enough_disk && enough_cpu
            };
            if !fits_device {
                return None;
            }
            let reason = if device.accelerator_bytes.is_some() {
                "подходит; хватает безопасного запаса ОЗУ и VRAM для запуска с GPU".into()
            } else {
                "подходит для текущего устройства".into()
            };
            Some(OllamaModelRecommendation {
                id: model.id.into(),
                description: model.description.into(),
                size_bytes: model.size_bytes,
                required_ram_bytes: model.required_ram_bytes,
                required_vram_bytes: model.required_vram_bytes,
                fits_device,
                installed: installed.contains(model.id),
                reason,
            })
        })
        .collect()
}

pub async fn fetch_catalog(
    config: &LiteRouterConfig,
    device: OllamaDeviceProfile,
) -> Result<OllamaCatalog, ProviderError> {
    let installed = fetch_installed_models(config).await?;
    let recommendations = recommend_models(&device, &installed);
    Ok(OllamaCatalog {
        installed,
        recommendations,
        device,
    })
}

pub async fn pull_model(base_url: &str, model: &str) -> Result<(), ProviderError> {
    validate_model_id(model)?;
    let base_url = native_base_url(base_url)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    let response = client
        .post(format!("{base_url}/api/pull"))
        .json(&serde_json::json!({ "model": model, "stream": true }))
        .send()
        .await
        .map_err(|error| ProviderError::Http(error.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(ProviderError::Api(format!("{status}: {body}")));
    }
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| ProviderError::Stream(error.to_string()))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(position) = buffer.find('\n') {
            let line = buffer.drain(..=position).collect::<String>();
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let progress = serde_json::from_str::<PullProgress>(line)
                .map_err(|error| ProviderError::Api(error.to_string()))?;
            if let Some(error) = progress.error {
                return Err(ProviderError::Api(error));
            }
            if progress.status.eq_ignore_ascii_case("error") {
                return Err(ProviderError::Api("Ollama pull failed".into()));
            }
        }
    }
    if !buffer.trim().is_empty() {
        let progress = serde_json::from_str::<PullProgress>(buffer.trim())
            .map_err(|error| ProviderError::Api(error.to_string()))?;
        if let Some(error) = progress.error {
            return Err(ProviderError::Api(error));
        }
    }
    Ok(())
}

fn validate_model_id(model: &str) -> Result<(), ProviderError> {
    if model.trim().is_empty()
        || model.len() > MAX_PULL_MODEL_CHARS
        || model
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(ProviderError::Config("invalid Ollama model id".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(ram_gib: u64) -> OllamaDeviceProfile {
        OllamaDeviceProfile {
            cpu_threads: 8,
            ram_bytes: ram_gib * GIB,
            disk_free_bytes: 100 * GIB,
            accelerator_bytes: None,
        }
    }

    #[test]
    fn recommendations_follow_ram_and_disk_limits() {
        let small = recommend_models(&device(4), &[]);
        assert!(small
            .iter()
            .any(|item| item.id == "qwen3:0.6b" && item.fits_device));
        assert!(!small.iter().any(|item| item.id == "qwen3:8b"));

        let installed = vec![crate::ModelCatalogEntry {
            id: "qwen3:1.7b".into(),
            context_tokens: None,
            max_output_tokens: None,
        }];
        let marked = recommend_models(&device(16), &installed);
        assert!(marked
            .iter()
            .any(|item| item.id == "qwen3:1.7b" && item.installed));
    }

    #[test]
    fn recommendations_use_vram_as_a_gpu_fit_signal() {
        let gpu = OllamaDeviceProfile {
            cpu_threads: 8,
            ram_bytes: 16 * GIB,
            disk_free_bytes: 100 * GIB,
            accelerator_bytes: Some(8 * GIB),
        };
        let recommendations = recommend_models(&gpu, &[]);
        let qwen = recommendations
            .iter()
            .find(|item| item.id == "qwen3:8b")
            .expect("qwen3:8b recommendation");
        assert!(qwen.fits_device);
        assert_eq!(qwen.required_vram_bytes, Some(6 * GIB));
        assert!(qwen.reason.contains("GPU"));
    }

    #[test]
    fn recommendations_require_ram_and_vram_when_gpu_is_detected() {
        let device = OllamaDeviceProfile {
            cpu_threads: 8,
            ram_bytes: 32 * GIB,
            disk_free_bytes: 200 * GIB,
            accelerator_bytes: Some(4 * GIB),
        };
        let recommendations = recommend_models(&device, &[]);

        assert!(!recommendations.iter().any(|item| item.id == "qwen3:14b"));
        assert!(!recommendations.iter().any(|item| item.id == "qwen3:8b"));
        assert!(!recommendations.iter().any(|item| item.id == "qwen3:4b"));
        assert!(recommendations.iter().any(|item| item.id == "llama3.2:3b"));
        assert!(recommendations.iter().all(|item| item.fits_device));
    }

    #[test]
    fn rejects_remote_ollama_endpoints() {
        assert!(validate_loopback("http://10.0.0.2:11434/v1").is_err());
        assert!(validate_loopback(DEFAULT_BASE_URL).is_ok());
        assert!(validate_model_id("two words").is_err());
    }
}
