//! Bounded model discovery for Cloudflare Workers AI.

use crate::{
    config::{LiteRouterConfig, ProviderProfileId},
    read_bounded_response, ModelCatalogEntry, ProviderError, MAX_MODEL_CATALOG_ENTRIES,
    MAX_MODEL_ID_CHARS,
};
use serde_json::Value;
use std::time::{Duration, Instant};

const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = MAX_MODEL_CATALOG_ENTRIES.div_ceil(PAGE_SIZE);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(15);
const API_ROOT: &str = "https://api.cloudflare.com/client/v4";

/// Fetches a bounded Workers AI catalog from the account-scoped model-search API.
pub async fn fetch_model_catalog(
    route: &LiteRouterConfig,
    account_id: &str,
) -> Result<Vec<ModelCatalogEntry>, ProviderError> {
    let Some(expected_base_url) = ProviderProfileId::cloudflare_base_url(account_id) else {
        return Err(ProviderError::Config(
            "provider profile account is invalid".into(),
        ));
    };
    if route.api_key.is_empty() {
        return Err(ProviderError::Config(
            "provider API key is not configured".into(),
        ));
    }
    if route.base_url.trim_end_matches('/') != expected_base_url {
        return Err(ProviderError::Config(
            "provider profile endpoint does not match".into(),
        ));
    }

    let url = format!("{API_ROOT}/accounts/{account_id}/ai/models/search");
    fetch_model_catalog_from_url(route, &expected_base_url, &url).await
}

async fn fetch_model_catalog_from_url(
    route: &LiteRouterConfig,
    expected_base_url: &str,
    url: &str,
) -> Result<Vec<ModelCatalogEntry>, ProviderError> {
    if route.api_key.is_empty() {
        return Err(ProviderError::Config(
            "provider API key is not configured".into(),
        ));
    }
    if route.base_url.trim_end_matches('/') != expected_base_url {
        return Err(ProviderError::Config(
            "provider profile endpoint does not match".into(),
        ));
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(DISCOVERY_TIMEOUT)
        .build()
        .map_err(|_| ProviderError::Http("provider model catalog client failed".into()))?;
    let deadline = Instant::now() + DISCOVERY_TIMEOUT;
    let mut models = Vec::new();

    for page in 1..=MAX_PAGES {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(ProviderError::Http(
                "provider model catalog request timed out".into(),
            ));
        }
        let page_value = page.to_string();
        let page_size_value = PAGE_SIZE.to_string();
        let response = client
            .get(url)
            .bearer_auth(&route.api_key)
            .query(&[
                ("format", "openrouter"),
                ("hide_experimental", "true"),
                ("page", page_value.as_str()),
                ("per_page", page_size_value.as_str()),
            ])
            .timeout(remaining)
            .send()
            .await
            .map_err(|_| ProviderError::Http("provider model catalog request failed".into()))?;
        if !response.status().is_success() {
            return Err(ProviderError::Api(format!(
                "provider model catalog request failed with HTTP {}",
                response.status()
            )));
        }
        let body = read_bounded_response(response, "provider model catalog").await?;
        let payload = serde_json::from_slice::<Value>(&body)
            .map_err(|_| ProviderError::Api("provider model catalog response is invalid".into()))?;
        if payload.get("success").and_then(Value::as_bool) == Some(false) {
            return Err(ProviderError::Api(
                "provider model catalog request was rejected".into(),
            ));
        }
        let entries = catalog_entries(&payload).ok_or_else(|| {
            ProviderError::Api("provider model catalog response is invalid".into())
        })?;
        let page_len = entries.len();
        for entry in entries {
            let id = entry
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            if id.chars().count() > MAX_MODEL_ID_CHARS || id.chars().any(char::is_control) {
                return Err(ProviderError::Api(
                    "provider model catalog contains an invalid model id".into(),
                ));
            }
            let context_tokens = entry
                .get("context_length")
                .and_then(Value::as_u64)
                .and_then(clamp_tokens);
            models.push(ModelCatalogEntry {
                id: id.to_owned(),
                context_tokens,
                max_output_tokens: None,
            });
            if models.len() > MAX_MODEL_CATALOG_ENTRIES {
                return Err(ProviderError::Api(
                    "provider model catalog contains too many entries".into(),
                ));
            }
        }
        if page_len < PAGE_SIZE {
            break;
        }
        if page == MAX_PAGES {
            return Err(ProviderError::Api(
                "provider model catalog contains too many entries".into(),
            ));
        }
    }

    models.sort_unstable_by(|left, right| left.id.cmp(&right.id));
    models.dedup_by(|left, right| left.id == right.id);
    Ok(models)
}

fn catalog_entries(payload: &Value) -> Option<&[Value]> {
    payload
        .get("result")
        .and_then(|result| {
            result
                .as_array()
                .or_else(|| result.get("data").and_then(Value::as_array))
        })
        .or_else(|| payload.get("data").and_then(Value::as_array))
        .map(Vec::as_slice)
}

fn clamp_tokens(value: u64) -> Option<u32> {
    if value == 0 {
        None
    } else {
        Some(u32::try_from(value).unwrap_or(u32::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{
        matchers::{header, method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn requests_the_account_catalog_with_bounded_openrouter_format() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(
                "/accounts/0123456789abcdef0123456789abcdef/ai/models/search",
            ))
            .and(query_param("format", "openrouter"))
            .and(query_param("hide_experimental", "true"))
            .and(query_param("page", "1"))
            .and(query_param("per_page", "100"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "success": true,
                "result": {
                    "data": [
                        { "id": "@cf/meta/llama-3.1-8b-instruct", "context_length": 131072 },
                        { "id": "@cf/qwen/qwen1.5-14b-chat-awq", "context_length": 32768 }
                    ]
                }
            })))
            .mount(&server)
            .await;

        let account_id = "0123456789abcdef0123456789abcdef";
        let route = LiteRouterConfig {
            api_key: "test-token".into(),
            base_url: ProviderProfileId::cloudflare_base_url(account_id).expect("account URL"),
            model: "@cf/meta/llama-3.1-8b-instruct".into(),
        };
        let result = fetch_model_catalog_from_url(
            &route,
            &route.base_url,
            &format!("{}/accounts/{account_id}/ai/models/search", server.uri()),
        )
        .await
        .expect("catalog fetch");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "@cf/meta/llama-3.1-8b-instruct");
        assert_eq!(result[0].context_tokens, Some(131072));
        assert_eq!(result[1].id, "@cf/qwen/qwen1.5-14b-chat-awq");
    }

    #[test]
    fn accepts_only_bounded_catalog_shapes_and_model_identifiers() {
        let payload = serde_json::json!({ "result": { "data": [{ "id": "model-a" }] } });
        assert_eq!(
            catalog_entries(&payload).map(|entries| entries.len()),
            Some(1)
        );
        assert_eq!(
            catalog_entries(&serde_json::json!({ "data": [] })).map(|entries| entries.len()),
            Some(0)
        );
        assert!(catalog_entries(&serde_json::json!({ "result": null })).is_none());
        assert_eq!(clamp_tokens(0), None);
        assert_eq!(clamp_tokens(u64::MAX), Some(u32::MAX));
    }
}
