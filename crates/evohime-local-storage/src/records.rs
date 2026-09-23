use serde::{Deserialize, Serialize};

/// One append-only journal event stored for a task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRecord {
    /// Monotonic database sequence assigned to the event.
    pub sequence_id: i64,
    /// Task identifier associated with the event.
    pub task_id: String,
    /// Event kind used by journal consumers.
    pub event_type: String,
    /// Serialized event payload.
    pub payload: Vec<u8>,
    /// Database creation timestamp.
    pub created_at: String,
}

/// Recorded outcome and recovery metadata for a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolMetricRecord {
    /// Database row identifier.
    pub id: i64,
    /// Task that invoked the tool.
    pub task_id: String,
    /// Tool name.
    pub tool_name: String,
    /// Iteration number within the task.
    pub iteration: i64,
    /// Whether the invocation succeeded.
    pub ok: bool,
    /// Failure category when the invocation failed.
    pub failure_kind: Option<String>,
    /// Whether a recovery hint was available.
    pub recovery_hint: bool,
    /// Whether the failure was escalated.
    pub escalated: bool,
    /// Database creation timestamp.
    pub created_at: String,
}

/// Persisted project identity and workspace configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRecord {
    /// Stable project identifier.
    pub id: String,
    /// User-facing project title.
    pub title: String,
    /// Filesystem path of the project's workspace.
    pub workspace_path: String,
    /// Optional source-control reference associated with the project.
    pub source_ref: Option<String>,
    /// Optimistic-lock version of the project record.
    pub version: i64,
}

/// Versioned project policy stored as serialized JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectPolicyRecord {
    /// Project owning this policy.
    pub project_id: String,
    /// Serialized policy document.
    pub policy_json: Vec<u8>,
    /// Optimistic-lock version of the policy.
    pub version: i64,
    /// Timestamp at which the policy was last updated.
    pub updated_at: String,
}

/// Persisted project work item, including its scheduling and lifecycle fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemRecord {
    /// Stable work-item identifier.
    pub id: String,
    /// Project that owns this work item.
    pub project_id: String,
    /// Optional parent work-item identifier.
    pub parent_id: Option<String>,
    /// Short work-item title.
    pub title: String,
    /// Detailed work-item description.
    pub description: String,
    /// Optional originating source reference.
    pub source_ref: Option<String>,
    /// Conditions that define successful completion.
    pub acceptance_criteria: String,
    /// Work explicitly excluded from this item.
    pub non_goals: String,
    /// Current lifecycle state.
    pub status: String,
    /// Scheduling priority; larger values are considered first.
    pub priority: i64,
    /// Optional estimate in the unit defined by the caller.
    pub estimate: Option<i64>,
    /// Optional complexity classification.
    pub complexity: Option<String>,
    /// Number of execution attempts recorded.
    pub attempt_count: i64,
    /// Optimistic-lock version used when updating status.
    pub version: i64,
}

/// Task imported from an external source before project-specific metadata is attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedTask {
    /// Stable imported-task identifier.
    pub id: String,
    /// Imported title.
    pub title: String,
    /// Imported description.
    pub description: String,
    /// Reference to the source task or document.
    pub source_ref: String,
    /// Imported completion criteria.
    pub acceptance_criteria: String,
}

/// Serialized provenance evidence associated with a stored entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceRecord {
    /// Stable provenance-record identifier.
    pub id: String,
    /// Provenance category.
    pub kind: String,
    /// Origin that supplied or established this evidence.
    pub source: String,
    /// Serialized evidence payload.
    pub payload: Vec<u8>,
}

/// Captured run snapshot associated with a workspace content hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRecord {
    /// Stable snapshot identifier.
    pub id: String,
    /// Run that produced the snapshot.
    pub run_id: String,
    /// Hash identifying the workspace state represented by the snapshot.
    pub workspace_hash: String,
    /// Serialized snapshot payload.
    pub payload: Vec<u8>,
    /// Snapshot creation timestamp.
    pub created_at: String,
}
