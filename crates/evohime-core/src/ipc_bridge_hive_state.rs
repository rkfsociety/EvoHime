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
        let latest = self.journal.latest_sequence().await;
        let after = latest.saturating_sub(511);
        let records = self.journal.replay(after, 512).await.unwrap_or_default();
        let mut by_child = std::collections::BTreeMap::new();
        for record in records {
            if record.event_type != "child.workflow" {
                continue;
            }
            if let Ok(projection) =
                serde_json::from_slice::<crate::child_workflow::ChildProjection>(&record.payload)
            {
                by_child.insert(projection.child_task_id.clone(), projection);
            }
        }
        let projections: Vec<_> = by_child.into_values().collect();
        let roles = self.persistent_agent_roles().await;
        let observation = self.hive_observation().await;
        crate::hive_state::HiveStateSnapshot::from_observation(
            &settings,
            &projections,
            &roles,
            &observation,
        )
    }

    /// Collects configured provider limits for the model selected on the active route.
    ///
    /// The projection is additive: a missing route configuration leaves caps
    /// unset; an unreadable provenance row preserves known caps but marks the
    /// counters unavailable so the UI cannot mistake missing history for zero.
    async fn hive_observation(&self) -> crate::hive_state::HiveObservation {
        let mut observation = crate::hive_state::HiveObservation {
            now_ms: crate::task_memory::now_millis() as i64,
            ..crate::hive_state::HiveObservation::default()
        };
        let Some(config) = self.gateway_config.as_ref() else {
            return observation;
        };
        let Some(route) = config.routes.get(&config.default_route) else {
            return observation;
        };
        // The ledger records the transport kind, so dispatch counters must be
        // queried by that exact value. The vendor profile is the readable
        // identity and decides whether the DeepSeek tariff applies.
        let transport = route.provider.as_str();
        let profile = route.provider_profile_id;
        let provider = profile
            .map_or(transport, |profile| profile.as_str())
            .to_owned();
        // The user-selected model overrides the configured default, so the caps
        // and counters must follow the model that will actually be dispatched.
        let selected_model = self
            .selected_model
            .get()
            .map(|value| value.to_string())
            .filter(|value| !value.trim().is_empty());
        let model = crate::core_agent::effective_model_name(
            &route.literouter.model,
            selected_model.as_deref(),
        );
        let limits = route.model_rate_limits.get(&model);
        observation.provider = provider;
        observation.model = model.clone();
        if profile == Some(evohime_model_gateway::ProviderProfileId::DeepSeek) {
            let day_start_ms = observation
                .now_ms
                .div_euclid(crate::hive_state::HIVE_DAY_WINDOW_MS)
                * crate::hive_state::HIVE_DAY_WINDOW_MS;
            if let Ok(usage) = self
                .journal
                .deepseek_usage_since(day_start_ms, transport, &model)
                .await
            {
                observation.daily_cost_nano_usd =
                    crate::hive_state::deepseek_daily_cost_nano_usd(&model, &usage);
                observation.daily_cost_available = observation.daily_cost_nano_usd.is_some();
            }
        }
        observation.minute_cap = limits
            .filter(|limits| limits.enabled)
            .map_or(0, |limits| limits.rpm);
        observation.hour_cap = limits
            .filter(|limits| limits.enabled)
            .map_or(0, |limits| limits.rph);
        observation.day_cap = limits
            .filter(|limits| limits.enabled)
            .map_or(0, |limits| limits.rpd);
        if let Ok(windows) = self
            .journal
            .model_dispatch_windows(
                transport,
                &model,
                observation.now_ms,
                crate::hive_state::HIVE_MINUTE_WINDOW_MS,
                crate::hive_state::HIVE_HOUR_WINDOW_MS,
                crate::hive_state::HIVE_DAY_WINDOW_MS,
            )
            .await
        {
            observation.history_available = true;
            observation.requests_last_minute = windows.last_minute;
            observation.requests_last_hour = windows.last_hour;
            observation.requests_last_day = windows.last_day;
            observation.oldest_minute_dispatch_ms = windows.oldest_minute_ms;
            observation.oldest_hour_dispatch_ms = windows.oldest_hour_ms;
            observation.oldest_day_dispatch_ms = windows.oldest_day_ms;
        }
        observation
    }

    /// Lists active role profiles from the persistent agent registry.
    ///
    /// Raw agent records never leave Core; only the bounded role identifier is
    /// used to keep an idle department visible in the snapshot.
    async fn persistent_agent_roles(&self) -> Vec<String> {
        let database = self.journal.database().lock().await;
        let Ok(agents) =
            evohime_local_storage::domains::agents::load_agents(database.connection(), 256)
        else {
            return Vec::new();
        };
        let mut roles: Vec<String> = agents
            .iter()
            .filter_map(|bytes| {
                let agent: crate::persistent_agent_registry::PersistentAgent =
                    serde_json::from_slice(bytes).ok()?;
                if agent.status != crate::persistent_agent_registry::LifecycleStatus::Active {
                    return None;
                }
                Some(agent.role_profile_ref.id)
            })
            .collect();
        roles.sort();
        roles.dedup();
        roles
    }
}
