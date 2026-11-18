use super::*;

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
        if let Err(error) = settings.validate() {
            settings.eva_workspace_path = None;
            self.journal.save_hive_settings(&settings).await?;
            return serde_json::to_vec(&serde_json::json!({
                "accepted": false,
                "eva_workspace_path": serde_json::Value::Null,
                "self_development_enabled": false,
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        let path_is_configured = settings.eva_workspace_path.is_some();
        let error_code = if path_is_configured {
            "".to_owned()
        } else {
            "Путь к отдельному Git-репозиторию не задан; инструменты саморазвития выключены."
                .to_owned()
        };
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": path_is_configured,
            "eva_workspace_path": settings.eva_workspace_path,
            "self_development_enabled": path_is_configured,
            "error_code": error_code,
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
                "eva_workspace_path": settings.eva_workspace_path,
                "self_development_enabled": settings.eva_workspace_path.is_some(),
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
                "eva_workspace_path": settings.eva_workspace_path,
                "self_development_enabled": settings.eva_workspace_path.is_some(),
                "error_code": error.to_string(),
            }))
            .map_err(|serialization_error| serialization_error.to_string());
        }
        self.journal.save_hive_settings(&settings).await?;
        serde_json::to_vec(&serde_json::json!({
            "accepted": true,
            "daily_budget_usd": settings.daily_budget_usd,
            "eva_workspace_path": settings.eva_workspace_path,
            "self_development_enabled": settings.eva_workspace_path.is_some(),
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

    /// Updates a named HiveSettings entry in Core durable storage.
    pub(crate) async fn set_hive_setting(&self, key: &str, value: &str) -> Result<Vec<u8>, String> {
        match key.trim() {
            "eva_workspace_path" => self.save_eva_workspace_path(value).await,
            "daily_budget_usd" => self.save_daily_budget_usd(value).await,
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
        let mut observation = crate::hive_observation::build_hive_observation(
            &self.journal,
            self.gateway_config.as_ref(),
            selected_model.as_deref(),
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
