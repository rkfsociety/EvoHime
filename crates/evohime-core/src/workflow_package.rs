//! Portable, non-executable Workflow Package contract.
//!
//! The package is a projection of the existing `workflow/v1` graph. It never
//! becomes a capability registry and it never contains runtime state or
//! credential values. Import validation is intentionally pure: preview cannot
//! write storage or start a workflow.

use crate::workflow::{NodeType, WorkflowGraph};
use evohime_local_storage::{workflow_package_store, LocalDatabase};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

pub const PACKAGE_FORMAT: &str = "evohime-workflow";
pub const PACKAGE_FORMAT_VERSION: u32 = 1;
pub const MAX_PACKAGE_BYTES: usize = 1024 * 1024;
pub const PACKAGE_EXTENSION: &str = ".evohime-workflow.json";
const MAX_ID_CHARS: usize = 128;
const MAX_TEXT_CHARS: usize = 2048;
const MAX_ITEMS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPackageDependency {
    pub kind: String,
    pub logical_id: String,
    #[serde(default)]
    pub required_version: Option<u32>,
    #[serde(default)]
    pub schema_hash: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPackageCredentialSlot {
    pub id: String,
    pub provider: String,
    pub auth_kind: String,
    #[serde(default)]
    pub required_scopes: Vec<String>,
    #[serde(default)]
    pub used_by_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkflowPackageProvenance {
    #[serde(default)]
    pub original_workflow_id: Option<String>,
    #[serde(default)]
    pub original_version: Option<u64>,
    #[serde(default)]
    pub forked_from_hash: Option<String>,
    #[serde(default)]
    pub lineage: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPackage {
    pub format: String,
    pub format_version: u32,
    pub workflow_id: String,
    pub workflow_version: u64,
    pub name: String,
    pub description: String,
    pub graph: WorkflowGraph,
    #[serde(default)]
    pub input_schema: Option<String>,
    #[serde(default)]
    pub output_schema: Option<String>,
    pub dependencies: Vec<WorkflowPackageDependency>,
    pub required_capabilities: Vec<String>,
    pub credential_slots: Vec<WorkflowPackageCredentialSlot>,
    #[serde(default)]
    pub context_requirements: Vec<String>,
    #[serde(default)]
    pub recommended_schedule: Option<String>,
    pub provenance: WorkflowPackageProvenance,
    pub created_at: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkflowPackageExportPolicy {
    /// Argument names explicitly approved as portable literals.
    pub portable_argument_keys: BTreeSet<String>,
    /// Argument name -> credential slot id. Values are never read into the package.
    pub credential_argument_slots: BTreeMap<String, String>,
    pub input_schema_portable: bool,
    pub output_schema_portable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowPackagePreview {
    pub package: WorkflowPackage,
    pub stripped_fields: Vec<String>,
    pub package_hash: String,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum WorkflowPackageError {
    #[error("unsupported package format: {0}")]
    UnsupportedFormat(String),
    #[error("package is too large")]
    TooLarge,
    #[error("package field is invalid: {0}")]
    InvalidField(String),
    #[error("workflow graph is invalid")]
    InvalidGraph,
    #[error("argument has no portable metadata: {0}")]
    UnclassifiedArgument(String),
    #[error("credential slot is missing: {0}")]
    MissingCredentialSlot(String),
    #[error("package path is not allowed")]
    InvalidPath,
    #[error("package file does not match the supplied content")]
    SourceMismatch,
    #[error("package I/O failed: {0}")]
    Io(String),
    #[error("package JSON failed: {0}")]
    Json(String),
}

fn bounded(
    name: &str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), WorkflowPackageError> {
    if required && value.trim().is_empty() {
        return Err(WorkflowPackageError::InvalidField(name.into()));
    }
    if value.chars().count() > max {
        return Err(WorkflowPackageError::InvalidField(name.into()));
    }
    Ok(())
}

fn credential_slot(id: &str, slots: &[WorkflowPackageCredentialSlot]) -> bool {
    slots.iter().any(|slot| slot.id == id)
}

fn redact_graph(
    graph: &mut WorkflowGraph,
    policy: &WorkflowPackageExportPolicy,
    slots: &[WorkflowPackageCredentialSlot],
) -> Result<Vec<String>, WorkflowPackageError> {
    let mut stripped = Vec::new();
    for node in &mut graph.nodes {
        let mut redact_args = |node_id: &str,
                               args: &mut BTreeMap<String, String>|
         -> Result<(), WorkflowPackageError> {
            for (key, value) in args.iter_mut() {
                if let Some(slot_id) = policy.credential_argument_slots.get(key) {
                    if !credential_slot(slot_id, slots) {
                        return Err(WorkflowPackageError::MissingCredentialSlot(slot_id.clone()));
                    }
                    *value = format!("$credential_slot:{slot_id}");
                    stripped.push(format!("{node_id}.{key}"));
                } else if !policy.portable_argument_keys.contains(key) {
                    return Err(WorkflowPackageError::UnclassifiedArgument(format!(
                        "{node_id}.{key}"
                    )));
                }
            }
            Ok(())
        };
        match &mut node.node_type {
            NodeType::Tool { tool } => redact_args(&node.id, &mut tool.arguments)?,
            NodeType::McpTool { mcp } => redact_args(&node.id, &mut mcp.arguments)?,
            _ => {}
        }
    }
    Ok(stripped)
}

fn dependencies(graph: &WorkflowGraph) -> Vec<WorkflowPackageDependency> {
    let mut result = BTreeMap::<(String, String), WorkflowPackageDependency>::new();
    for node in &graph.nodes {
        let entry = match &node.node_type {
            NodeType::Tool { tool } => Some(("tool", tool.tool_name.clone())),
            NodeType::McpTool { mcp } => Some(("mcp_server", mcp.server_id.clone())),
            NodeType::Child { child } => Some(("child_role", child.role.clone())),
            NodeType::ContextProvider { provider } => {
                Some(("context_provider", provider.provider_id.clone()))
            }
            _ => None,
        };
        if let Some((kind, logical_id)) = entry {
            result
                .entry((kind.into(), logical_id.clone()))
                .or_insert(WorkflowPackageDependency {
                    kind: kind.into(),
                    logical_id,
                    required_version: node.block.as_ref().map(|b| b.block_version),
                    schema_hash: None,
                    optional: false,
                    notes: None,
                });
        }
        if let Some(block) = &node.block {
            result
                .entry(("block".into(), block.block_id.clone()))
                .or_insert(WorkflowPackageDependency {
                    kind: "block".into(),
                    logical_id: block.block_id.clone(),
                    required_version: Some(block.block_version),
                    schema_hash: None,
                    optional: false,
                    notes: None,
                });
        }
    }
    result.into_values().collect()
}

fn content_value(package: &WorkflowPackage) -> serde_json::Value {
    serde_json::json!({
        "format": package.format, "format_version": package.format_version,
        "workflow_id": package.workflow_id, "workflow_version": package.workflow_version,
        "name": package.name, "description": package.description, "graph": package.graph,
        "input_schema": package.input_schema, "output_schema": package.output_schema,
        "dependencies": package.dependencies, "required_capabilities": package.required_capabilities,
        "credential_slots": package.credential_slots, "context_requirements": package.context_requirements,
        "recommended_schedule": package.recommended_schedule,
    })
}

fn canonical_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<_> = map.keys().collect();
            keys.sort();
            let body = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::Value::String(key.clone()),
                        canonical_json(&map[key])
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{body}}}")
        }
        serde_json::Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(canonical_json)
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => other.to_string(),
    }
}

pub fn content_hash(package: &WorkflowPackage) -> String {
    let mut hasher = Sha256::new();
    hasher.update(canonical_json(&content_value(package)).as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn export_preview(
    graph: &WorkflowGraph,
    name: String,
    description: String,
    policy: &WorkflowPackageExportPolicy,
    credential_slots: Vec<WorkflowPackageCredentialSlot>,
    created_at: String,
) -> Result<WorkflowPackagePreview, WorkflowPackageError> {
    bounded("name", &name, MAX_TEXT_CHARS, true)?;
    bounded("description", &description, MAX_TEXT_CHARS, false)?;
    bounded("created_at", &created_at, MAX_TEXT_CHARS, true)?;
    if graph.validate().is_err() {
        return Err(WorkflowPackageError::InvalidGraph);
    }
    if credential_slots.len() > MAX_ITEMS {
        return Err(WorkflowPackageError::InvalidField(
            "credential_slots".into(),
        ));
    }
    let mut portable_graph = graph.clone();
    let stripped_fields = redact_graph(&mut portable_graph, policy, &credential_slots)?;
    let package = WorkflowPackage {
        format: PACKAGE_FORMAT.into(),
        format_version: PACKAGE_FORMAT_VERSION,
        workflow_id: graph.graph_id.clone(),
        workflow_version: graph.version,
        name,
        description,
        graph: portable_graph,
        input_schema: None,
        output_schema: None,
        dependencies: dependencies(graph),
        required_capabilities: graph
            .nodes
            .iter()
            .filter_map(|n| n.block.as_ref().map(|b| b.block_id.clone()))
            .collect(),
        credential_slots,
        context_requirements: Vec::new(),
        recommended_schedule: None,
        provenance: WorkflowPackageProvenance {
            original_workflow_id: Some(graph.graph_id.clone()),
            original_version: Some(graph.version),
            ..Default::default()
        },
        created_at,
        content_hash: String::new(),
    };
    let mut package = package;
    package.content_hash = content_hash(&package);
    let size = serde_json::to_vec(&package)
        .map_err(|e| WorkflowPackageError::Json(e.to_string()))?
        .len();
    if size > MAX_PACKAGE_BYTES {
        return Err(WorkflowPackageError::TooLarge);
    }
    Ok(WorkflowPackagePreview {
        package_hash: package.content_hash.clone(),
        package,
        stripped_fields,
    })
}

pub fn preview_from_json(
    graph_json: &[u8],
    name: String,
    description: String,
    portable_argument_keys: Vec<String>,
    credential_slots_json: &[u8],
    created_at: String,
) -> Result<WorkflowPackagePreview, WorkflowPackageError> {
    if graph_json.len() > MAX_PACKAGE_BYTES || credential_slots_json.len() > MAX_PACKAGE_BYTES {
        return Err(WorkflowPackageError::TooLarge);
    }
    let graph: WorkflowGraph = serde_json::from_slice(graph_json)
        .map_err(|error| WorkflowPackageError::Json(error.to_string()))?;
    let credential_slots: Vec<WorkflowPackageCredentialSlot> = if credential_slots_json.is_empty() {
        Vec::new()
    } else {
        serde_json::from_slice(credential_slots_json)
            .map_err(|error| WorkflowPackageError::Json(error.to_string()))?
    };
    let policy = WorkflowPackageExportPolicy {
        portable_argument_keys: portable_argument_keys.into_iter().collect(),
        ..Default::default()
    };
    export_preview(
        &graph,
        name,
        description,
        &policy,
        credential_slots,
        created_at,
    )
}

pub fn rebind_package(
    package: &WorkflowPackage,
    slot_id: &str,
    local_credential_reference: &str,
) -> Result<serde_json::Value, WorkflowPackageError> {
    validate_import(package)?;
    bounded("slot_id", slot_id, MAX_ID_CHARS, true)?;
    bounded(
        "local_credential_reference",
        local_credential_reference,
        MAX_ID_CHARS,
        true,
    )?;
    if !package
        .credential_slots
        .iter()
        .any(|slot| slot.id == slot_id)
    {
        return Err(WorkflowPackageError::MissingCredentialSlot(slot_id.into()));
    }
    // The reference is acknowledged as an opaque Core-owned binding. It is
    // never returned to the renderer or written into the portable package.
    Ok(serde_json::json!({
        "slot_id": slot_id,
        "bound": true,
        "reference_present": true,
        "package_hash": package.content_hash,
    }))
}

pub fn persist_rebind(
    database: &LocalDatabase,
    package: &WorkflowPackage,
    slot_id: &str,
    local_credential_reference: &str,
    now_ms: i64,
) -> Result<serde_json::Value, WorkflowPackageError> {
    let result = rebind_package(package, slot_id, local_credential_reference)?;
    workflow_package_store::save_binding(
        database.connection(),
        &package.content_hash,
        slot_id,
        local_credential_reference,
        now_ms,
    )
    .map_err(|error| WorkflowPackageError::Io(error.to_string()))?;
    Ok(result)
}

pub fn validate_import(package: &WorkflowPackage) -> Result<(), WorkflowPackageError> {
    if package.format != PACKAGE_FORMAT || package.format_version != PACKAGE_FORMAT_VERSION {
        return Err(WorkflowPackageError::UnsupportedFormat(
            package.format.clone(),
        ));
    }
    bounded("workflow_id", &package.workflow_id, MAX_ID_CHARS, true)?;
    bounded("name", &package.name, MAX_TEXT_CHARS, true)?;
    bounded("description", &package.description, MAX_TEXT_CHARS, false)?;
    if package.dependencies.len() > MAX_ITEMS || package.credential_slots.len() > MAX_ITEMS {
        return Err(WorkflowPackageError::TooLarge);
    }
    if package.graph.validate().is_err() {
        return Err(WorkflowPackageError::InvalidGraph);
    }
    if package.content_hash != content_hash(package) {
        return Err(WorkflowPackageError::InvalidField("content_hash".into()));
    }
    if serde_json::to_vec(package)
        .map_err(|e| WorkflowPackageError::Json(e.to_string()))?
        .len()
        > MAX_PACKAGE_BYTES
    {
        return Err(WorkflowPackageError::TooLarge);
    }
    Ok(())
}

pub fn parse_bounded(bytes: &[u8]) -> Result<WorkflowPackage, WorkflowPackageError> {
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(WorkflowPackageError::TooLarge);
    }
    let package: WorkflowPackage =
        serde_json::from_slice(bytes).map_err(|e| WorkflowPackageError::Json(e.to_string()))?;
    validate_import(&package)?;
    Ok(package)
}

pub fn validate_package_path(path: &Path) -> Result<PathBuf, WorkflowPackageError> {
    if path.extension().and_then(|value| value.to_str()) != Some("json")
        || !path.to_string_lossy().ends_with(PACKAGE_EXTENSION)
    {
        return Err(WorkflowPackageError::InvalidPath);
    }
    let parent = path.parent().ok_or(WorkflowPackageError::InvalidPath)?;
    let parent = parent
        .canonicalize()
        .map_err(|_| WorkflowPackageError::InvalidPath)?;
    let file_name = path.file_name().ok_or(WorkflowPackageError::InvalidPath)?;
    Ok(parent.join(file_name))
}

pub fn read_package(path: &Path) -> Result<WorkflowPackage, WorkflowPackageError> {
    let path = validate_package_path(path)?;
    let bytes = std::fs::read(path).map_err(|e| WorkflowPackageError::Io(e.to_string()))?;
    parse_bounded(&bytes)
}

fn source_fingerprint(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Writes a preview package through an atomic temp-to-final operation. The
/// destination is canonicalized and the package remains a bounded JSON file.
pub fn write_package(path: &Path, package: &WorkflowPackage) -> Result<(), WorkflowPackageError> {
    validate_import(package)?;
    let path = validate_package_path(path)?;
    let bytes = serde_json::to_vec_pretty(package)
        .map_err(|error| WorkflowPackageError::Json(error.to_string()))?;
    if bytes.len() > MAX_PACKAGE_BYTES {
        return Err(WorkflowPackageError::TooLarge);
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, &bytes).map_err(|error| WorkflowPackageError::Io(error.to_string()))?;
    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(WorkflowPackageError::Io(error.to_string()));
    }
    Ok(())
}

/// Commits only metadata. The source package stays outside SQLite and is
/// re-read/revalidated on every import attempt.
pub fn commit_import(
    database: &LocalDatabase,
    source_path: &Path,
    package: &WorkflowPackage,
    idempotency_key: &str,
    now_ms: i64,
) -> Result<workflow_package_store::PackageImportRecord, WorkflowPackageError> {
    validate_import(package)?;
    bounded("idempotency_key", idempotency_key, MAX_ID_CHARS, true)?;
    let source_path = validate_package_path(source_path)?;
    let source_package = read_package(&source_path)?;
    if source_package.content_hash != package.content_hash {
        return Err(WorkflowPackageError::SourceMismatch);
    }
    if let Some(existing) =
        workflow_package_store::find_committed_by_hash(database.connection(), &package.content_hash)
            .map_err(|error| WorkflowPackageError::Io(error.to_string()))?
    {
        return Ok(existing);
    }
    let record = workflow_package_store::PackageImportRecord {
        import_id: uuid::Uuid::new_v4().to_string(),
        package_hash: package.content_hash.clone(),
        source_fingerprint: source_fingerprint(&source_path),
        local_workflow_id: format!("{}-imported", package.workflow_id),
        local_workflow_version: package.workflow_version,
        phase: workflow_package_store::ImportPhase::Pending,
        provenance_json: serde_json::to_string(&package.provenance)
            .map_err(|error| WorkflowPackageError::Json(error.to_string()))?,
        redaction_summary_json:
            serde_json::json!({"credential_slots": package.credential_slots.len()}).to_string(),
        updated_at_ms: now_ms,
    };
    workflow_package_store::insert_pending(database.connection(), &record)
        .map_err(|error| WorkflowPackageError::Io(error.to_string()))?;
    workflow_package_store::finish(
        database.connection(),
        &record.import_id,
        workflow_package_store::ImportPhase::Committed,
        now_ms,
    )
    .map_err(|error| WorkflowPackageError::Io(error.to_string()))?;
    Ok(workflow_package_store::PackageImportRecord {
        phase: workflow_package_store::ImportPhase::Committed,
        ..record
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{ExecutionPolicy, NodeType, ToolActionProfile, WorkflowNode};

    fn graph() -> WorkflowGraph {
        WorkflowGraph {
            contract: crate::workflow::WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "demo".into(),
            version: 1,
            entry_node: "source".into(),
            nodes: vec![WorkflowNode::new(
                "source",
                NodeType::Tool {
                    tool: ToolActionProfile {
                        tool_name: "safe".into(),
                        arguments: BTreeMap::new(),
                    },
                },
                ExecutionPolicy {
                    retry: crate::workflow::RetryPolicy {
                        max_attempts: 1,
                        backoff_ms: 0,
                        retryable_errors: vec![],
                    },
                    timeout_ms: 1,
                    cancellation: crate::workflow::CancellationPolicy::Cooperative,
                    approval: crate::workflow::ApprovalPolicy {
                        required: false,
                        reason: None,
                    },
                },
            )],
            edges: vec![],
            budget: Default::default(),
        }
    }

    #[test]
    fn hash_ignores_creation_and_provenance_metadata() {
        let mut preview = export_preview(
            &graph(),
            "Demo".into(),
            "".into(),
            &Default::default(),
            vec![],
            "one".into(),
        )
        .unwrap();
        let first = preview.package_hash.clone();
        preview.package.created_at = "two".into();
        preview.package.provenance.lineage.push("fork".into());
        assert_eq!(first, content_hash(&preview.package));
    }

    #[test]
    fn unclassified_arguments_fail_closed() {
        let mut graph = graph();
        if let NodeType::Tool { tool } = &mut graph.nodes[0].node_type {
            tool.arguments.insert("secret".into(), "value".into());
        }
        let error = export_preview(
            &graph,
            "Demo".into(),
            "".into(),
            &Default::default(),
            vec![],
            "now".into(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            WorkflowPackageError::UnclassifiedArgument(_)
        ));
    }

    #[test]
    fn parsed_package_is_bounded_and_validated() {
        let preview = export_preview(
            &graph(),
            "Demo".into(),
            "".into(),
            &Default::default(),
            vec![],
            "now".into(),
        )
        .unwrap();
        let bytes = serde_json::to_vec(&preview.package).unwrap();
        assert_eq!(
            parse_bounded(&bytes).unwrap().content_hash,
            preview.package_hash
        );
    }
}
