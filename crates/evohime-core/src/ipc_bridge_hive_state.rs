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
        crate::hive_state::HiveStateSnapshot::from_projection(&settings, &projections, &roles)
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
