//! Model config / available models HTTP API.
use crate::app::AppState;
use crate::secrets;
use crate::ApiError;
use axum::{
    extract::{Query, State},
    Json,
};
use evohime_model_gateway::providers::ProviderKind;
use evohime_model_gateway::{ModelGateway, ModelRouteConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ModelSettingsRequest {
    default_route: String,
    routes: Vec<ModelRouteRequest>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct ModelRouteRequest {
    name: String,
    provider: String,
    model: String,
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default = "default_billing_mode")]
    billing_mode: String,
}

pub(crate) fn default_billing_mode() -> String {
    "free".to_string()
}

pub(crate) async fn model_config(
    State(state): State<Arc<AppState>>,
) -> Json<evohime_model_gateway::ModelConfigResponse> {
    Json(state.model_config_response().await)
}

#[derive(Debug, Serialize)]
pub(crate) struct AvailableModelsResponse {
    route: String,
    provider: String,
    billing_mode: String,
    models: Vec<String>,
}

pub(crate) async fn available_models(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<AvailableModelsResponse>, ApiError> {
    let default_route = state.model_config.read().await.default_route.clone();
    let route_name = query.get("route").cloned().unwrap_or(default_route);
    let config = state.model_config.read().await;
    let effective_route_name =
        if route_name == "orchestrator" && !config.routes.contains_key(&route_name) {
            config.default_route.clone()
        } else {
            route_name.clone()
        };
    let route = config
        .routes
        .get(&effective_route_name)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown model route: {route_name}")))?;
    let provider = route.provider.as_str().to_string();
    let billing_mode = if route.provider == ProviderKind::LiteRouter
        && route.literouter.model.ends_with(":free")
    {
        "free"
    } else {
        "paid"
    };
    let models = if route.provider == ProviderKind::Mock {
        Vec::new()
    } else {
        let client = reqwest::Client::new();
        let response = client
            .get(format!(
                "{}/models",
                route.literouter.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&route.literouter.api_key)
            .send()
            .await
            .map_err(|error| {
                ApiError::Internal(format!("не удалось получить список моделей: {error}"))
            })?;
        if !response.status().is_success() {
            return Err(ApiError::BadRequest(format!(
                "провайдер вернул ошибку списка моделей: {}",
                response.status()
            )));
        }
        let payload: OpenAiModelsResponse = response.json().await.map_err(|error| {
            ApiError::Internal(format!("некорректный ответ списка моделей: {error}"))
        })?;
        let mut models = payload
            .data
            .into_iter()
            .map(|model| model.id)
            .filter(|model| {
                if billing_mode == "free" {
                    model.ends_with(":free")
                } else {
                    !model.ends_with(":free")
                }
            })
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        models
    };
    Ok(Json(AvailableModelsResponse {
        route: route_name,
        provider,
        billing_mode: billing_mode.to_string(),
        models,
    }))
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiModelsResponse {
    #[serde(default)]
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct OpenAiModelEntry {
    id: String,
}

pub(crate) async fn update_model_config(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ModelSettingsRequest>,
) -> Result<Json<evohime_model_gateway::ModelConfigResponse>, ApiError> {
    let current = state.model_config.read().await;
    let config = build_model_config(request.clone(), &current)?;
    drop(current);
    let gateway = if config.routes.values().all(ModelRouteConfig::configured) {
        Some(Arc::new(
            ModelGateway::from_config(&config)
                .map_err(|error| ApiError::BadRequest(error.to_string()))?,
        ))
    } else {
        None
    };
    let mut persisted_request = request;
    let workspace_root = state.workspace_root.to_string_lossy().to_string();
    for route in &mut persisted_request.routes {
        if route
            .api_key
            .as_deref()
            .map(str::trim)
            .is_none_or(str::is_empty)
        {
            route.api_key = config
                .routes
                .get(&route.name)
                .map(|item| item.literouter.api_key.clone())
                .filter(|key| !key.trim().is_empty());
        }
        // Encrypt API keys before persisting (Phase 5.7)
        if let Some(api_key) = &route.api_key {
            route.api_key = Some(secrets::encrypt_api_key(api_key, &workspace_root));
        }
    }
    let persisted_value = serde_json::to_value(persisted_request)
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    evohime_storage::save_setting(&state.pool, "model_config", &persisted_value)
        .await
        .map_err(|error| ApiError::Internal(error.to_string()))?;
    *state.model_config.write().await = config;
    *state.model_gateway.write().await = gateway;

    Ok(Json(state.model_config_response().await))
}

/// Decrypt API keys in a ModelSettingsRequest (Phase 5.7).
/// Called when loading saved config from database.
pub(crate) fn decrypt_model_config(
    mut request: ModelSettingsRequest,
    workspace_root: &str,
) -> ModelSettingsRequest {
    for route in &mut request.routes {
        if let Some(api_key) = &route.api_key {
            // Try to decrypt; if it fails, key is still plaintext (backward compat)
            let decrypted = secrets::decrypt_api_key(api_key, workspace_root);
            if !decrypted.is_empty() {
                route.api_key = Some(decrypted);
            }
        }
    }
    request
}

pub(crate) fn build_model_config(
    request: ModelSettingsRequest,
    current: &evohime_model_gateway::ModelGatewayConfig,
) -> Result<evohime_model_gateway::ModelGatewayConfig, ApiError> {
    if request.default_route.trim().is_empty() || request.routes.is_empty() {
        return Err(ApiError::BadRequest(
            "Нужен хотя бы один маршрут и маршрут по умолчанию".to_string(),
        ));
    }
    fn inherits_default_key(name: &str) -> bool {
        name == "orchestrator" || name == "synthesizer" || name.starts_with("reviewer_")
    }

    let mut routes = HashMap::new();
    let requested_default_key = request
        .routes
        .iter()
        .find(|route| route.name.trim() == request.default_route)
        .and_then(|route| route.api_key.as_deref())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            current
                .routes
                .get(&request.default_route)
                .map(|route| route.literouter.api_key.clone())
        })
        .unwrap_or_default();
    for route in request.routes {
        let name = route.name.trim().to_string();
        let model = route.model.trim().to_string();
        let base_url = route.base_url.trim().to_string();
        if name.is_empty() || model.is_empty() {
            return Err(ApiError::BadRequest(
                "Название маршрута и модель не могут быть пустыми".to_string(),
            ));
        }
        if routes.contains_key(&name) {
            return Err(ApiError::BadRequest(format!(
                "Маршрут '{name}' указан несколько раз"
            )));
        }
        let provider = ProviderKind::parse(&route.provider).ok_or_else(|| {
            ApiError::BadRequest(format!("Неизвестный провайдер: {}", route.provider))
        })?;
        let billing_mode = if provider == ProviderKind::LiteRouter {
            route.billing_mode.as_str()
        } else {
            "paid"
        };
        if !matches!(billing_mode, "free" | "paid") {
            return Err(ApiError::BadRequest(
                "Режим LiteRouter должен быть free или paid".to_string(),
            ));
        }
        if provider == ProviderKind::LiteRouter {
            let is_free_model = model.ends_with(":free");
            if billing_mode == "free" && !is_free_model {
                return Err(ApiError::BadRequest(
                    "В бесплатном режиме LiteRouter доступны только модели с суффиксом :free"
                        .to_string(),
                ));
            }
            if billing_mode == "paid" && is_free_model {
                return Err(ApiError::BadRequest(
                    "В платном режиме выбери модель без суффикса :free".to_string(),
                ));
            }
        }
        let existing_key = current
            .routes
            .get(&name)
            .map(|item| item.literouter.api_key.clone())
            .or_else(|| {
                inherits_default_key(&name).then(|| {
                    current
                        .routes
                        .get(&current.default_route)
                        .map(|item| item.literouter.api_key.clone())
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or_else(|| requested_default_key.clone())
                })
            })
            .unwrap_or_default();
        let api_key = if inherits_default_key(&name) {
            requested_default_key.clone()
        } else {
            route
                .api_key
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(existing_key)
        };
        let config = match provider {
            ProviderKind::LiteRouter => ModelRouteConfig::literouter(api_key, base_url, model),
            ProviderKind::OpenAICompatible => {
                ModelRouteConfig::openai_compatible(api_key, base_url, model)
            }
            ProviderKind::Mock => ModelRouteConfig::mock(model),
        };
        routes.insert(name, config);
    }
    if !routes.contains_key(&request.default_route) {
        return Err(ApiError::BadRequest(
            "Маршрут по умолчанию должен существовать в списке маршрутов".to_string(),
        ));
    }
    Ok(evohime_model_gateway::ModelGatewayConfig {
        default_route: request.default_route,
        routes,
    })
}

// 7.108: Cost limits / spend caps endpoints

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CostLimitsListResponse {
    limits: Vec<CostLimitInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CostLimitInfo {
    model: String,
    daily_cap_tokens: i64,
    reset_hour: i32,
    enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CostLimitUpdateRequest {
    daily_cap_tokens: i64,
    reset_hour: i32,
    enabled: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CostLimitResponse {
    model: String,
    daily_cap_tokens: i64,
    reset_hour: i32,
    enabled: bool,
}

pub(crate) async fn list_cost_limits(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CostLimitsListResponse>, ApiError> {
    let limits = evohime_storage::list_cost_limits(&state.pool)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(CostLimitsListResponse {
        limits: limits
            .into_iter()
            .map(|l| CostLimitInfo {
                model: l.model,
                daily_cap_tokens: l.daily_cap_tokens,
                reset_hour: l.reset_hour,
                enabled: l.enabled,
            })
            .collect(),
    }))
}

pub(crate) async fn update_cost_limit(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(model): axum::extract::Path<String>,
    Json(req): Json<CostLimitUpdateRequest>,
) -> Result<Json<CostLimitResponse>, ApiError> {
    let update = evohime_storage::CostLimitUpdate {
        daily_cap_tokens: req.daily_cap_tokens,
        reset_hour: req.reset_hour,
        enabled: req.enabled,
    };
    let result = evohime_storage::update_cost_limit(&state.pool, &model, &update)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    Ok(Json(CostLimitResponse {
        model: result.model,
        daily_cap_tokens: result.daily_cap_tokens,
        reset_hour: result.reset_hour,
        enabled: result.enabled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_model_gateway::ModelRouteConfig;
    use std::collections::HashMap;

    #[test]
    fn carries_default_api_key_to_new_orchestrator_route() {
        let current = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::from([(
                "default".to_string(),
                ModelRouteConfig::literouter("", "https://api.literouter.com/v1", "deepseek:free"),
            )]),
        };
        let config = build_model_config(
            ModelSettingsRequest {
                default_route: "default".to_string(),
                routes: vec![
                    ModelRouteRequest {
                        name: "default".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: Some("lr_test_key".to_string()),
                        billing_mode: "free".to_string(),
                    },
                    ModelRouteRequest {
                        name: "orchestrator".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: Some("old_orchestrator_key".to_string()),
                        billing_mode: "free".to_string(),
                    },
                ],
            },
            &current,
        )
        .expect("model config is valid");

        assert_eq!(config.routes["default"].literouter.api_key, "lr_test_key");
        assert_eq!(
            config.routes["orchestrator"].literouter.api_key,
            "lr_test_key"
        );
    }

    #[test]
    fn carries_default_api_key_to_new_reviewer_and_synthesizer_routes() {
        let current = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::from([(
                "default".to_string(),
                ModelRouteConfig::literouter("", "https://api.literouter.com/v1", "deepseek:free"),
            )]),
        };
        let config = build_model_config(
            ModelSettingsRequest {
                default_route: "default".to_string(),
                routes: vec![
                    ModelRouteRequest {
                        name: "default".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: Some("lr_test_key".to_string()),
                        billing_mode: "free".to_string(),
                    },
                    ModelRouteRequest {
                        name: "reviewer_0".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: None,
                        billing_mode: "free".to_string(),
                    },
                    ModelRouteRequest {
                        name: "synthesizer".to_string(),
                        provider: "literouter".to_string(),
                        model: "deepseek:free".to_string(),
                        base_url: "https://api.literouter.com/v1".to_string(),
                        api_key: None,
                        billing_mode: "free".to_string(),
                    },
                ],
            },
            &current,
        )
        .expect("model config is valid");

        assert_eq!(config.routes["reviewer_0"].literouter.api_key, "lr_test_key");
        assert_eq!(config.routes["synthesizer"].literouter.api_key, "lr_test_key");
    }

    #[test]
    fn preserves_saved_api_key_when_autosave_sends_empty_fields() {
        let current = evohime_model_gateway::ModelGatewayConfig {
            default_route: "default".to_string(),
            routes: HashMap::from([(
                "default".to_string(),
                ModelRouteConfig::literouter(
                    "lr_saved_key",
                    "https://api.literouter.com/v1",
                    "deepseek:free",
                ),
            )]),
        };
        let config = build_model_config(
            ModelSettingsRequest {
                default_route: "default".to_string(),
                routes: vec![ModelRouteRequest {
                    name: "default".to_string(),
                    provider: "literouter".to_string(),
                    model: "deepseek:free".to_string(),
                    base_url: "https://api.literouter.com/v1".to_string(),
                    api_key: Some(String::new()),
                    billing_mode: "free".to_string(),
                }],
            },
            &current,
        )
        .expect("model config is valid");

        assert_eq!(config.routes["default"].literouter.api_key, "lr_saved_key");
    }
}
