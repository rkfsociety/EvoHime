use axum::Json;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct FeatureFlags {
    pub sites: bool,
    pub scheduled: bool,
    pub otlp: bool,
    pub cloud_sync: bool,
}

pub fn enabled(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(value) => match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

pub fn current() -> FeatureFlags {
    FeatureFlags {
        sites: enabled("EVOHIME_FEATURE_SITES", true),
        scheduled: enabled("EVOHIME_FEATURE_SCHEDULED", true),
        otlp: enabled("EVOHIME_FEATURE_OTLP", true),
        cloud_sync: enabled("EVOHIME_FEATURE_CLOUD_SYNC", true),
    }
}

pub async fn list() -> Json<FeatureFlags> {
    Json(current())
}
