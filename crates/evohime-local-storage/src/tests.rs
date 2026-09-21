use super::{
    DiagnosticsSummary, ImportedTask, LocalDatabase, ModelRouteSnapshot, PolicySnapshot,
    RecoveryState, RecoveryTransitionInput, RoleRef, RunCheckpointRecord, RunEffectRecord,
    RunRecord, RunSnapshots, SkillRef, StorageError, ToolMetricInput, WorkItemRecord,
    SCHEMA_VERSION,
};
use std::path::PathBuf;

fn temp_database_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("evohime-test-{name}-{}.db", std::process::id()))
}

#[path = "tests_schema.rs"]
mod schema;

#[path = "tests_runtime.rs"]
mod runtime;
