use super::*;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HiveRequestLimitsUpdate {
    primary_limits: evohime_model_gateway::ProviderLimits,
    fallback_limits: evohime_model_gateway::ProviderLimits,
    primary_token_quota: u64,
    fallback_token_quota: u64,
    primary_quota_reset_utc_midnight: bool,
    fallback_quota_reset_utc_midnight: bool,
}

impl IpcBridge {
    /// Validates and persists the isolated checkout used by self-development.
    pub(crate) async fn save_eva_workspace_path(&self, path: &str) -> Result<Vec<u8>, String> {
        let normalized = path.trim();
        let mut settings = self.journal.hive_settings().await?;
        settings.eva_workspace_path = if normalized.is_empty() {
            None
        } else {
            Some(normalized.to_owned())
        };
        if !normalized.is_empty() {
            settings
                .watched_repositories
                .retain(|repo| !crate::hive_settings::paths_refer_to_same_repo(repo, normalized));
        }
        if let Err(error) = settings.validate() {
            settings.eva_workspace_path = None;
            self.journal.save_hive_settings(&settings).await?;
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "setting_key": "eva_workspace_path",
                "eva_workspace_path": serde_json::Value::Null,
                "watched_repositories": settings.external_watched_repositories(),
                "self_development_enabled": false,
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        let path_is_configured = settings.eva_workspace_path().is_some();
        let error_code = if path_is_configured {
            "".to_owned()
        } else {
            "Путь к отдельному Git-репозиторию не задан; инструменты саморазвития выключены."
                .to_owned()
        };
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": path_is_configured,
            "setting_key": "eva_workspace_path",
            "eva_workspace_path": settings.eva_workspace_path(),
            "watched_repositories": settings.external_watched_repositories(),
            "self_development_enabled": path_is_configured,
            "error_code": error_code,
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates and persists the list of external Git repositories watched by Hive.
    pub(crate) async fn save_watched_repositories(&self, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let previous = settings.external_watched_repositories();
        let eva_path = settings.eva_workspace_path().map(str::to_owned);
        let trimmed = value.trim();
        let parsed: Vec<String> = if trimmed.is_empty() {
            Vec::new()
        } else {
            match serde_json::from_str::<Vec<String>>(trimmed) {
                Ok(items) => items,
                Err(error) => {
                    return serde_json::to_vec(&serde_json::json!({
                        "accepted": false,
                        "setting_key": "watched_repositories",
                        "watched_repositories": previous,
                        "eva_workspace_path": settings.eva_workspace_path(),
                        "self_development_enabled": settings.eva_workspace_path().is_some(),
                        "error_code": format!("invalid watched_repositories payload: {error}"),
                    }))
                    .map_err(|serialization_error| serialization_error.to_string());
                }
            }
        };
        let mut normalized = Vec::with_capacity(parsed.len());
        for item in parsed {
            let candidate = item.trim();
            if candidate.is_empty()
                || normalized.iter().any(|existing: &String| {
                    crate::hive_settings::paths_refer_to_same_repo(existing, candidate)
                })
            {
                continue;
            }
            if eva_path
                .as_deref()
                .is_some_and(|eva| crate::hive_settings::paths_refer_to_same_repo(eva, candidate))
            {
                return serde_json::to_vec(&serde_json::json!({
                    "accepted": false,
                    "setting_key": "watched_repositories",
                    "watched_repositories": previous,
                    "eva_workspace_path": settings.eva_workspace_path(),
                    "self_development_enabled": settings.eva_workspace_path().is_some(),
                    "error_code": "Папка исходников Евы (eva_workspace_path) настраивается отдельно и не должна добавляться в сторонние проекты Улья.",
                }))
                .map_err(|serialization_error| serialization_error.to_string());
            }
            normalized.push(candidate.to_owned());
        }
        settings.watched_repositories = normalized;
        if let Err(error) = settings.validate() {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "setting_key": "watched_repositories",
                "watched_repositories": previous,
                "eva_workspace_path": settings.eva_workspace_path(),
                "self_development_enabled": settings.eva_workspace_path().is_some(),
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": "watched_repositories",
            "watched_repositories": settings.external_watched_repositories(),
            "eva_workspace_path": settings.eva_workspace_path(),
            "self_development_enabled": settings.eva_workspace_path().is_some(),
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates and persists the isolated provider profile used by Hive.
    pub(crate) async fn save_hive_provider_profile(&self, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let trimmed = value.trim();
        settings.hive_provider_profile = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
        if let Err(error) = settings.validate() {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "setting_key": "hive_provider_profile",
                "hive_provider_profile": settings.hive_provider_profile,
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": "hive_provider_profile",
            "hive_provider_profile": settings.hive_provider_profile,
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates and persists the isolated model override used by Hive.
    pub(crate) async fn save_hive_model(&self, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let trimmed = value.trim();
        settings.hive_model = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
        if let Err(error) = settings.validate() {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "setting_key": "hive_model",
                "hive_model": settings.hive_model,
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": "hive_model",
            "hive_model": settings.hive_model,
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates and persists the isolated fallback provider profile used by Hive.
    pub(crate) async fn save_hive_fallback_provider_profile(
        &self,
        value: &str,
    ) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let trimmed = value.trim();
        settings.hive_fallback_provider_profile = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
        if let Err(error) = settings.validate() {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "setting_key": "hive_fallback_provider_profile",
                "hive_fallback_provider_profile": settings.hive_fallback_provider_profile,
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": "hive_fallback_provider_profile",
            "hive_fallback_provider_profile": settings.hive_fallback_provider_profile,
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates and persists the isolated fallback model override used by Hive.
    pub(crate) async fn save_hive_fallback_model(&self, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let trimmed = value.trim();
        settings.hive_fallback_model = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
        if let Err(error) = settings.validate() {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "setting_key": "hive_fallback_model",
                "hive_fallback_model": settings.hive_fallback_model,
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": "hive_fallback_model",
            "hive_fallback_model": settings.hive_fallback_model,
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates a candidate Eva self-development path without persisting it.
    pub(crate) fn validate_eva_workspace_path_payload(
        &self,
        path: &str,
    ) -> Result<Vec<u8>, String> {
        let normalized = path.trim();
        let payload = match crate::hive_settings::validate_eva_workspace_path(normalized) {
            Ok(()) => serde_json::json!({
                "valid": true,
                "eva_workspace_path": normalized,
                "error_code": "",
            }),
            Err(error) => serde_json::json!({
                "valid": false,
                "eva_workspace_path": serde_json::Value::Null,
                "error_code": error.to_string(),
            }),
        };
        serde_json::to_vec(&payload).map_err(|error| error.to_string())
    }

    /// Validates and persists the daily background spend ceiling in USD.
    pub(crate) async fn save_daily_budget_usd(&self, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let parsed = value.trim().parse::<f64>();
        let valid_value = match parsed {
            Ok(amount)
                if amount.is_finite()
                    && (0.0..=crate::hive_settings::MAX_DAILY_BUDGET_USD).contains(&amount) =>
            {
                Some(amount)
            }
            _ => None,
        };
        let Some(amount) = valid_value else {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "daily_budget_usd": settings.daily_budget_usd,
                "eva_workspace_path": settings.eva_workspace_path(),
                "self_development_enabled": settings.eva_workspace_path().is_some(),
                "error_code": format!(
                    "daily_budget_usd must be a number between 0 and {}",
                    crate::hive_settings::MAX_DAILY_BUDGET_USD
                ),
            }))
            .map_err(|error| error.to_string());
        };
        settings.daily_budget_usd = amount;
        if let Err(error) = settings.validate() {
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "daily_budget_usd": serde_json::Value::Null,
                "eva_workspace_path": settings.eva_workspace_path(),
                "self_development_enabled": settings.eva_workspace_path().is_some(),
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "daily_budget_usd": settings.daily_budget_usd,
            "eva_workspace_path": settings.eva_workspace_path(),
            "self_development_enabled": settings.eva_workspace_path().is_some(),
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    /// Validates and persists the proactive evolution toggle.
    pub(crate) async fn save_proactive_evolution_enabled(
        &self,
        value: &str,
    ) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let enabled = match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => {
                return serde_json::to_vec(&serde_json::json!({
                    "accepted": false,
                    "setting_key": "proactive_evolution_enabled",
                    "proactive_evolution_enabled": settings.proactive_evolution_enabled,
                    "error_code": "proactive_evolution_enabled must be true or false",
                }))
                .map_err(|error| error.to_string());
            }
        };
        settings.proactive_evolution_enabled = enabled;
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": "proactive_evolution_enabled",
            "proactive_evolution_enabled": settings.proactive_evolution_enabled,
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }

    async fn save_hive_request_limits_batch(&self, value: &str) -> Result<Vec<u8>, String> {
        let key = "hive_request_limits";
        let Ok(update) = serde_json::from_str::<HiveRequestLimitsUpdate>(value) else {
            return self.hive_limit_rejected(key, "invalid request limits payload");
        };
        let mut settings = self.journal.hive_settings().await?;
        settings.hive_provider_limits = update.primary_limits;
        settings.hive_fallback_provider_limits = update.fallback_limits;
        settings.hive_daily_token_quota = update.primary_token_quota;
        settings.hive_fallback_daily_token_quota = update.fallback_token_quota;
        settings.hive_quota_reset_utc_midnight = update.primary_quota_reset_utc_midnight;
        settings.hive_fallback_quota_reset_utc_midnight = update.fallback_quota_reset_utc_midnight;
        // Zero means "no quota": a configured flag must never claim a limit
        // that nothing enforces.
        settings.hive_token_quota_configured = settings.hive_daily_token_quota > 0;
        settings.hive_fallback_token_quota_configured =
            settings.hive_fallback_daily_token_quota > 0;
        if let Err(error) = settings.validate() {
            return self.hive_limit_rejected(key, &error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true, "setting_key": key, "error_code": ""
        }))
        .map_err(|error| error.to_string())
    }

    async fn save_hive_request_limits(&self, key: &str, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let parsed = serde_json::from_str::<evohime_model_gateway::ProviderLimits>(value);
        let Ok(limits) = parsed else {
            return self.hive_limit_rejected(key, "request limits must contain rpm, rph and rpd");
        };
        let (setting_key, output_key) = if key == "hive_provider_limits" {
            settings.hive_provider_limits = limits;
            (key, "hive_provider_limits")
        } else {
            settings.hive_fallback_provider_limits = limits;
            (key, "hive_fallback_provider_limits")
        };
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": setting_key,
            output_key: limits,
            "error_code": ""
        }))
        .map_err(|error| error.to_string())
    }

    async fn save_hive_token_quota(&self, key: &str, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let quota = value
            .trim()
            .parse::<u64>()
            .ok()
            .filter(|quota| *quota <= 1_000_000_000_000);
        let Some(quota) = quota else {
            return self
                .hive_limit_rejected(key, "daily token quota must be between 0 and 1000000000000");
        };
        let (setting_key, configured_key, output_key) = if key == "hive_daily_token_quota" {
            settings.hive_daily_token_quota = quota;
            settings.hive_token_quota_configured = quota > 0;
            (key, "hive_token_quota_configured", "hive_daily_token_quota")
        } else {
            settings.hive_fallback_daily_token_quota = quota;
            settings.hive_fallback_token_quota_configured = quota > 0;
            (
                key,
                "hive_fallback_token_quota_configured",
                "hive_fallback_daily_token_quota",
            )
        };
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": setting_key,
            output_key: quota,
            configured_key: quota > 0,
            "error_code": ""
        }))
        .map_err(|error| error.to_string())
    }

    async fn save_hive_quota_reset(&self, key: &str, value: &str) -> Result<Vec<u8>, String> {
        let mut settings = self.journal.hive_settings().await?;
        let reset = match value.trim() {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => return self.hive_limit_rejected(key, "quota reset flag must be true or false"),
        };
        let (setting_key, output_key) = if key == "hive_quota_reset_utc_midnight" {
            settings.hive_quota_reset_utc_midnight = reset;
            (key, "hive_quota_reset_utc_midnight")
        } else {
            settings.hive_fallback_quota_reset_utc_midnight = reset;
            (key, "hive_fallback_quota_reset_utc_midnight")
        };
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "setting_key": setting_key,
            output_key: reset,
            "error_code": ""
        }))
        .map_err(|error| error.to_string())
    }

    fn hive_limit_rejected(&self, key: &str, error: &str) -> Result<Vec<u8>, String> {
        serde_json::to_vec(&serde_json::json!({
            "accepted": false,
            "setting_key": key,
            "error_code": error
        }))
        .map_err(|serialization_error| serialization_error.to_string())
    }

    /// Updates a named HiveSettings entry in Core durable storage.
    pub(crate) async fn set_hive_setting(&self, key: &str, value: &str) -> Result<Vec<u8>, String> {
        match key.trim() {
            "eva_workspace_path" => self.save_eva_workspace_path(value).await,
            "watched_repositories" => self.save_watched_repositories(value).await,
            "hive_provider_profile" => self.save_hive_provider_profile(value).await,
            "hive_model" => self.save_hive_model(value).await,
            "hive_fallback_provider_profile" => {
                self.save_hive_fallback_provider_profile(value).await
            }
            "hive_fallback_model" => self.save_hive_fallback_model(value).await,
            "daily_budget_usd" => self.save_daily_budget_usd(value).await,
            "hive_request_limits" => self.save_hive_request_limits_batch(value).await,
            "hive_provider_limits" | "hive_fallback_provider_limits" => {
                self.save_hive_request_limits(key, value).await
            }
            "hive_daily_token_quota" | "hive_fallback_daily_token_quota" => {
                self.save_hive_token_quota(key, value).await
            }
            "hive_quota_reset_utc_midnight" | "hive_fallback_quota_reset_utc_midnight" => {
                self.save_hive_quota_reset(key, value).await
            }
            "proactive_evolution_enabled" => self.save_proactive_evolution_enabled(value).await,
            other => serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "eva_workspace_path": serde_json::Value::Null,
                "self_development_enabled": false,
                "error_code": format!("unknown hive setting: {other}"),
            }))
            .map_err(|error| error.to_string()),
        }
    }

    /// Builds the read-only hive snapshot from the durable child projection
    /// window and the validated hive settings.
    ///
    /// Every read is bounded and fallible input degrades to a safe default:
    /// an unreadable journal produces an empty snapshot instead of an error,
    /// because the panel polls this command on a timer.
    pub(super) async fn hive_state_snapshot(&self) -> crate::hive_state::HiveStateSnapshot {
        let settings = self.journal.hive_settings().await.unwrap_or_default();
        let projections = crate::hive_observation::replay_hive_projections(&self.journal).await;
        let roles = crate::hive_observation::persistent_agent_roles(&self.journal).await;
        let selected_model = crate::hive_observation::selected_model_name(&self.selected_model);
        let mut adjusted_config = self.gateway_config.clone();
        if let Some(config) = adjusted_config.as_mut() {
            config.apply_hive_route_overrides(
                settings.hive_provider_profile.as_deref(),
                settings.hive_model.as_deref(),
            );
            config.apply_hive_fallback_route_overrides(
                settings.hive_fallback_provider_profile.as_deref(),
                settings.hive_fallback_model.as_deref(),
            );
            config.apply_hive_route_limits(
                "hive",
                settings.hive_provider_limits,
                settings.hive_daily_token_quota,
                settings.hive_quota_reset_utc_midnight,
            );
            config.apply_hive_route_limits(
                "hive_fallback",
                settings.hive_fallback_provider_limits,
                settings.hive_fallback_daily_token_quota,
                settings.hive_fallback_quota_reset_utc_midnight,
            );
        }
        let mut observation = crate::hive_observation::build_hive_observation_for_hive(
            &self.journal,
            adjusted_config.as_ref(),
            selected_model.as_deref(),
            settings.hive_provider_profile.as_deref(),
            settings.hive_model.as_deref(),
            crate::task_memory::now_millis() as i64,
        )
        .await;
        if let Some(pulse) = self.pulse.as_ref() {
            let status = pulse.status();
            observation.pulse_status = status.status;
            observation.pulse_next_wake_at_ms = status.next_wake_at_ms;
        }
        crate::hive_state::HiveStateSnapshot::from_observation(
            &settings,
            &projections,
            &roles,
            &observation,
        )
    }

    /// Applies the UI's EvaPulse toggle and returns the new daemon projection.
    ///
    /// The toggle is not an authorization decision: it only enables or parks
    /// the Core-owned background loop. Budget and provider caps are still
    /// enforced by the daemon itself on every cycle.
    pub(super) fn toggle_pulse(&self, enabled: bool) -> Result<Vec<u8>, String> {
        let Some(pulse) = self.pulse.as_ref() else {
            return serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "accepted": false,
                "enabled": false,
                "status": crate::hive_state::HIVE_PULSE_DISABLED,
                "next_wake_at_ms": serde_json::Value::Null,
                "error_code": "pulse_unavailable",
            }))
            .map_err(|error| error.to_string());
        };
        pulse.set_enabled(enabled);
        let status = pulse.status();
        serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "accepted": true,
            "enabled": enabled,
            "status": status.status,
            "next_wake_at_ms": status.next_wake_at_ms,
            "error_code": "",
        }))
        .map_err(|error| error.to_string())
    }
}
