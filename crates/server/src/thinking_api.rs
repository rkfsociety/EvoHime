//! Thinking settings and usage tracking HTTP API (Wave 3B: Extended Reasoning).

use crate::app::AppState;
use crate::ApiError;
use axum::{extract::State, http::StatusCode, Json};
use evohime_storage::{
    get_monthly_thinking_cost, get_or_create_thinking_settings, update_thinking_settings,
    ThinkingSettings,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ThinkingSettingsResponse {
    pub enabled: bool,
    pub budget_tokens: Option<i32>,
    pub max_budget_tokens: Option<i32>,
    pub show_thinking: bool,
    pub thinking_verbosity: Option<String>,
    pub monthly_cost_limit_usd: Option<f64>,
    pub warning_threshold_percent: Option<i32>,
    pub monthly_spending_usd: Option<f64>,
}

impl From<ThinkingSettings> for ThinkingSettingsResponse {
    fn from(settings: ThinkingSettings) -> Self {
        Self {
            enabled: settings.enabled,
            budget_tokens: settings.budget_tokens,
            max_budget_tokens: settings.max_budget_tokens,
            show_thinking: settings.show_thinking,
            thinking_verbosity: settings.thinking_verbosity,
            monthly_cost_limit_usd: settings
                .monthly_thinking_cost_limit
                .as_ref()
                .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0)),
            warning_threshold_percent: settings.warning_threshold_percent,
            monthly_spending_usd: None, // Will be filled separately
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ThinkingSettingsRequest {
    pub enabled: bool,
    pub budget_tokens: Option<i32>,
    pub show_thinking: Option<bool>,
    pub thinking_verbosity: Option<String>,
}

/// Get global thinking settings
pub(crate) async fn get_thinking_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ThinkingSettingsResponse>, ApiError> {
    let settings = get_or_create_thinking_settings(&state.pool)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let monthly_spending = get_monthly_thinking_cost(&state.pool)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0));

    let mut response = ThinkingSettingsResponse::from(settings);
    response.monthly_spending_usd = monthly_spending;

    Ok(Json(response))
}

/// Update global thinking settings
pub(crate) async fn put_thinking_settings(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ThinkingSettingsRequest>,
) -> Result<(StatusCode, Json<ThinkingSettingsResponse>), ApiError> {
    // Validate budget tokens if provided
    if let Some(budget) = req.budget_tokens {
        if !(1000..=32000).contains(&budget) {
            return Err(ApiError::BadRequest(
                "budget_tokens must be between 1000 and 32000".to_string(),
            ));
        }
    }

    let settings = update_thinking_settings(
        &state.pool,
        req.enabled,
        req.budget_tokens,
        req.show_thinking.unwrap_or(true),
        req.thinking_verbosity,
    )
    .await
    .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let monthly_spending = get_monthly_thinking_cost(&state.pool)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
        .map(|d| d.to_string().parse::<f64>().unwrap_or(0.0));

    let mut response = ThinkingSettingsResponse::from(settings);
    response.monthly_spending_usd = monthly_spending;

    Ok((StatusCode::OK, Json(response)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thinking_settings_response_serialization() {
        let response = ThinkingSettingsResponse {
            enabled: true,
            budget_tokens: Some(10000),
            max_budget_tokens: Some(32000),
            show_thinking: true,
            thinking_verbosity: Some("full".to_string()),
            monthly_cost_limit_usd: Some(50.0),
            warning_threshold_percent: Some(80),
            monthly_spending_usd: Some(12.50),
        };

        let json = serde_json::to_string(&response).expect("serialize");
        let deserialized: ThinkingSettingsResponse =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.enabled, response.enabled);
    }
}
