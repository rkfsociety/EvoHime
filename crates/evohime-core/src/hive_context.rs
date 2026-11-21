//! Core implementations of the remaining Hive boundary traits.
//!
//! Supplies [`CoreWorkspaceProvider`] (backed by `EventJournal` and
//! `SelectedModel`) and implements [`HiveTelemetrySink`] for
//! [`StructuredLogger`].

use evohime_hive_contract::{BoxFuture, HiveTelemetrySink, WorkspaceProvider};

use crate::core_agent::SelectedModel;
use crate::logging::StructuredLogger;
use crate::EventJournal;

/// Core-backed [`WorkspaceProvider`] reading `HiveSettings` from the durable
/// journal and model selection from [`SelectedModel`].
#[derive(Clone)]
pub struct CoreWorkspaceProvider {
    journal: EventJournal,
    selected_model: SelectedModel,
}

impl CoreWorkspaceProvider {
    /// Creates a workspace provider backed by `journal` and `selected_model`.
    pub fn new(journal: EventJournal, selected_model: SelectedModel) -> Self {
        Self {
            journal,
            selected_model,
        }
    }
}

impl WorkspaceProvider for CoreWorkspaceProvider {
    fn eva_workspace_path(&self) -> BoxFuture<'_, Option<String>> {
        Box::pin(async move {
            self.journal
                .hive_settings()
                .await
                .ok()
                .and_then(|settings| settings.primary_workspace_path().map(str::to_owned))
        })
    }

    fn selected_model_name(&self) -> Option<String> {
        self.selected_model.get().map(|model| model.to_string())
    }
}

impl HiveTelemetrySink for StructuredLogger {
    fn log_event(&self, level: &str, event: &str, fields: serde_json::Value) {
        let _ = self.write(level, event, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hive_settings::HiveSettings;

    #[tokio::test]
    async fn core_workspace_provider_reads_model_and_workspace_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let journal = EventJournal::open(dir.path().join("hive-ctx.db")).expect("journal opens");
        let selected = SelectedModel::default();
        selected.set("test-model");

        let provider = CoreWorkspaceProvider::new(journal.clone(), selected);
        assert_eq!(
            provider.selected_model_name().as_deref(),
            Some("test-model")
        );
        assert_eq!(provider.eva_workspace_path().await, None);

        let repo_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&repo_dir).expect("workspace dir");
        let status = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo_dir)
            .status()
            .expect("git init");
        assert!(status.success());

        let settings = HiveSettings {
            eva_workspace_path: Some(repo_dir.to_string_lossy().into_owned()),
            ..HiveSettings::default()
        };
        journal
            .save_hive_settings(&settings)
            .await
            .expect("save settings");

        assert_eq!(
            provider.eva_workspace_path().await,
            Some(repo_dir.to_string_lossy().into_owned())
        );
    }

    #[test]
    fn structured_logger_implements_hive_telemetry_sink() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log_path = dir.path().join("hive.jsonl");
        let logger = StructuredLogger::open(&log_path).expect("logger opens");
        let sink: &dyn HiveTelemetrySink = &logger;
        sink.log_event(
            "info",
            "eva_pulse.all_good",
            serde_json::json!({ "cycles": 1 }),
        );
        let content = std::fs::read_to_string(&log_path).expect("read log");
        assert!(content.contains("eva_pulse.all_good"));
    }
}
