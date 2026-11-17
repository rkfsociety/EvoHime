use super::*;

impl IpcBridge {
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
