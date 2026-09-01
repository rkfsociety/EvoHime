use serde::Serialize;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_TABS: usize = 8;
pub const MAX_PROJECTION_BYTES: usize = 128 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct TabDescriptor {
    pub id: &'static str,
    pub label: &'static str,
    pub availability: &'static str,
    pub reason: &'static str,
    pub badge_source: &'static str,
    pub persistence: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Projection {
    pub schema_version: u32,
    pub conversation_id: String,
    pub workspace_id: String,
    pub run_id: String,
    pub backend_snapshot_hash: String,
    pub capability_snapshot_hash: String,
    pub event_cursor: u64,
    pub event_count: usize,
    pub task_count: usize,
    pub usage_input_tokens: u64,
    pub usage_output_tokens: u64,
    pub tabs: Vec<TabDescriptor>,
    pub redaction: &'static str,
}

pub fn validate_scope(
    conversation_id: &str,
    workspace_id: &str,
    run_id: &str,
    backend_snapshot_hash: &str,
    capability_snapshot_hash: &str,
    after_sequence: u64,
    limit: usize,
) -> Result<(), &'static str> {
    if conversation_id.is_empty() || conversation_id.len() > 128 {
        return Err("invalid_conversation_scope");
    }
    if workspace_id.is_empty() || workspace_id.len() > 32 * 1024 || workspace_id.contains('\n') {
        return Err("invalid_workspace_scope");
    }
    if run_id.len() > 128
        || backend_snapshot_hash.len() > 128
        || capability_snapshot_hash.len() > 128
    {
        return Err("invalid_snapshot");
    }
    if limit == 0 || limit > 200 || after_sequence == u64::MAX {
        return Err("invalid_bounds");
    }
    Ok(())
}

pub fn tab_descriptors() -> Vec<TabDescriptor> {
    vec![
        TabDescriptor {
            id: "files",
            label: "Files",
            availability: "unavailable",
            reason: "revision_safe_files_not_installed",
            badge_source: "core_capability",
            persistence: "presentation_only",
        },
        TabDescriptor {
            id: "diff",
            label: "Diff",
            availability: "unavailable",
            reason: "revision_safe_files_not_installed",
            badge_source: "core_capability",
            persistence: "presentation_only",
        },
        TabDescriptor {
            id: "tasks",
            label: "Tasks",
            availability: "available",
            reason: "",
            badge_source: "conversation_events",
            persistence: "presentation_only",
        },
        TabDescriptor {
            id: "terminal",
            label: "Terminal",
            availability: "unavailable",
            reason: "workbench_is_read_only",
            badge_source: "core_policy",
            persistence: "presentation_only",
        },
        TabDescriptor {
            id: "browser",
            label: "Browser",
            availability: "unavailable",
            reason: "agentic_browser_session_not_installed",
            badge_source: "core_capability",
            persistence: "presentation_only",
        },
        TabDescriptor {
            id: "usage",
            label: "Usage",
            availability: "available",
            reason: "",
            badge_source: "conversation_events",
            persistence: "presentation_only",
        },
    ]
}

pub fn build_projection(
    conversation_id: String,
    workspace_id: String,
    run_id: String,
    backend_snapshot_hash: String,
    capability_snapshot_hash: String,
    event_cursor: u64,
    events: &[evohime_local_storage::conversation_event_log_store::StoredConversationEvent],
) -> Projection {
    let mut tasks = std::collections::BTreeSet::new();
    let mut input = 0;
    let mut output = 0;
    for event in events {
        if let Some(task_id) = event.task_id.as_deref() {
            if !task_id.is_empty() {
                tasks.insert(task_id.to_string());
            }
        }
        if event.kind == "usage_snapshot" {
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&event.renderer_payload)
            {
                input += value
                    .get("input_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                output += value
                    .get("output_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
            }
        }
    }
    Projection {
        schema_version: CONTRACT_VERSION,
        conversation_id,
        workspace_id,
        run_id,
        backend_snapshot_hash,
        capability_snapshot_hash,
        event_cursor,
        event_count: events.len(),
        task_count: tasks.len(),
        usage_input_tokens: input,
        usage_output_tokens: output,
        tabs: tab_descriptors(),
        redaction: "renderer_metadata_only",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_is_bounded_and_fail_closed() {
        assert_eq!(
            validate_scope("", "workspace", "", "", "", 0, 1),
            Err("invalid_conversation_scope")
        );
        assert_eq!(
            validate_scope("conversation", "workspace", "", "", "", 0, 201),
            Err("invalid_bounds")
        );
        assert!(validate_scope(
            "conversation",
            "workspace",
            "run",
            "backend",
            "capability",
            0,
            1
        )
        .is_ok());
    }

    #[test]
    fn registry_contains_separate_capability_aware_tabs() {
        let tabs = tab_descriptors();
        assert_eq!(tabs.len(), 6);
        assert!(tabs
            .iter()
            .any(|tab| tab.id == "tasks" && tab.availability == "available"));
        assert!(tabs
            .iter()
            .any(|tab| tab.id == "browser" && tab.availability == "unavailable"));
        assert!(tabs
            .iter()
            .all(|tab| tab.persistence == "presentation_only"));
    }
}
