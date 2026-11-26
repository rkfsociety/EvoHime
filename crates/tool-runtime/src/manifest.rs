use evohime_permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Discriminator identifying the supported tool manifest schema.
pub const MANIFEST_KIND: &str = "tool/manifest/v1";

/// Rewrites a historical dotted tool id to its canonical underscore form.
///
/// The rewrite is lexical: it does not check whether the id is registered.
/// Callers that need a real tool must resolve the result against
/// [`crate::ToolRegistry`], which fails closed for unknown names.
pub fn canonical_tool_id(name: &str) -> String {
    if name.contains('.') {
        name.replace('.', "_")
    } else {
        name.to_owned()
    }
}

/// Returns the namespace segment of a tool id.
///
/// Builtin ids are `<namespace>_<action>[_<detail>]` (`filesystem_read`), and
/// historical records may still carry the dotted spelling (`filesystem.read`).
/// Both forms split on the same boundary, so grouping and filtering by tool
/// family never depends on the separator that happens to be in the record.
pub fn tool_namespace(tool_id: &str) -> &str {
    let end = tool_id.find(['_', '.']).unwrap_or(tool_id.len());
    tool_id.get(..end).unwrap_or(tool_id)
}

/// Canonical input-schema catalog for builtin tools.  The registry and Core
/// both consume this function; tool implementations remain responsible for
/// semantic validation after the structural contract has been checked.
pub fn builtin_input_schema(tool_id: &str) -> Value {
    let string = |description: &str| serde_json::json!({"type":"string","description":description});
    let integer = || serde_json::json!({"type":"integer","minimum":1});
    let object = |properties: Value, required: &[&str]| serde_json::json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    match tool_id {
        // Declared kernel surface keeps one source of truth: the tool module
        // owns the schema, and the manifest derives it instead of a stub.
        "analysis_run" => crate::tools::analysis::input_schema(),
        "eva_workspace_read_file" => object(
            serde_json::json!({"path":string("Path relative to the isolated Eva workspace")}),
            &["path"],
        ),
        "eva_workspace_write_file" => object(
            serde_json::json!({"path":string("Path relative to the isolated Eva workspace"),"content":string("Complete UTF-8 file contents")}),
            &["path", "content"],
        ),
        "eva_workspace_run_command" => object(
            serde_json::json!({"command":string("Single allow-listed command; shell composition is forbidden")}),
            &["command"],
        ),
        "eva_workspace_git" => object(
            serde_json::json!({"action":{"type":"string","enum":["status","diff","commit","push"]},"message":string("Commit message; required only for commit")}),
            &["action"],
        ),
        "workflow_catalog" => object(serde_json::json!({}), &[]),
        "workflow_start" => object(
            serde_json::json!({
                "template_id": string("Stable ID of a fixed Core-owned workflow template"),
                "inputs": {
                    "type": "object",
                    "description": "Template-declared string inputs",
                    "additionalProperties": {"type": "string"}
                },
                "idempotency_key": string("Optional key to deduplicate this request")
            }),
            &["template_id", "inputs"],
        ),
        "workflow_status" | "workflow_cancel" => object(
            serde_json::json!({"run_id":string("Durable workflow run ID in the current workspace")}),
            &["run_id"],
        ),
        "utility_base64_encode"
        | "utility_base64_decode"
        | "utility_hash_sha256"
        | "utility_hash_sha512"
        | "utility_json_format"
        | "utility_json_minify" => object(
            serde_json::json!({"text":string("Bounded UTF-8 input")}),
            &["text"],
        ),
        "utility_uuid_v4" => object(serde_json::json!({}), &[]),
        "utility_token_generate" => object(
            serde_json::json!({"bytes":{"type":"integer","minimum":1,"maximum":128}}),
            &[],
        ),
        "utility_text_case_convert" => object(
            serde_json::json!({"text":string("Bounded UTF-8 input"),"mode":{"type":"string","enum":["lower","upper","snake","kebab","camel"]}}),
            &["text"],
        ),
        "filesystem_read" => object(
            serde_json::json!({"path":string("Workspace-relative file path"),"offset":{"type":"integer","minimum":1,"description":"1-based first line to return (default 1)"},"limit":{"type":"integer","minimum":1,"maximum":2000,"description":"Maximum lines to return (default 200)"}}),
            &["path"],
        ),
        "filesystem_write" => object(
            serde_json::json!({"path":string("Logical namespace/path"),"content":string("Complete UTF-8 content"),"expected_hash":string("SHA-256 hash from filesystem_read")}),
            &["path", "content"],
        ),
        "filesystem_patch" => object(
            serde_json::json!({
                "path": string("Logical namespace/path"),
                "patch": string("Unified diff (optional if old_text and new_text are provided)"),
                "old_text": string("Exact text snippet to replace (simpler alternative to unified diff patch)"),
                "new_text": string("Replacement text for old_text"),
                "expected_hash": string("SHA-256 hash from filesystem_read")
            }),
            &["path", "expected_hash"],
        ),
        "filesystem_delete" => object(
            serde_json::json!({"path":string("Logical namespace/path"),"recursive":{"type":"boolean"},"expected_hash":string("SHA-256 hash of the file before deletion")}),
            &["path"],
        ),
        "git_worktree_create" => object(
            serde_json::json!({"worktree_id":string("Core-generated task worktree identity"),"base_commit":string("Validated Git commit/ref")}),
            &["worktree_id", "base_commit"],
        ),
        "git_worktree_remove" => object(
            serde_json::json!({"worktree_id":string("Core-generated task worktree identity")}),
            &["worktree_id"],
        ),
        "git_worktree_preflight" => object(
            serde_json::json!({"worktree_id":string("Core-generated task worktree identity"),"base_commit":string("Pinned base commit")}),
            &["worktree_id", "base_commit"],
        ),
        "filesystem_move" | "filesystem_copy" => object(
            serde_json::json!({"from":string("Logical namespace/path"),"to":string("Logical namespace/path"),"recursive":{"type":"boolean"},"expected_hash":string("SHA-256 hash of the source file")}),
            &["from", "to"],
        ),
        "filesystem_search" => object(
            serde_json::json!({"query":string("Literal text (or ripgrep regex when regex=true)"),"path":string("Optional workspace path"),"glob":string("Optional glob"),"limit":integer(),"regex":{"type":"boolean","description":"Treat query as a regular expression (requires ripgrep); default false"},"ignore_case":{"type":"boolean","description":"Case-insensitive match; default false"}}),
            &["query"],
        ),
        "filesystem_list" => object(
            serde_json::json!({"path":string("Workspace-relative directory")}),
            &[],
        ),
        "filesystem_stat" | "filesystem_mkdir" => object(
            serde_json::json!({"path":string("Workspace-relative path")}),
            &["path"],
        ),
        "agent_run" => object(
            serde_json::json!({"prompt":string("Task prompt"),"max_steps":integer(),"timeout_ms":integer(),"model_route":string("Optional model route")}),
            &["prompt"],
        ),
        "hive_delegate_agent" => object(
            serde_json::json!({
                "role": string("Worker role (researcher, planner, implementer, tester, reviewer, releaser, architect, or custom role)"),
                "prompt": string("Clear task instructions for the worker"),
                "agent_name": string("Short human-readable Russian name for this agent (e.g. 'Исследователь ядра')"),
                "department": string("Optional sector/department identifier"),
                "plan_summary": string("User-facing summary of the plan required before mutating/test steps"),
                "scope": string("Optional target files or module scope"),
                "acceptance_criteria": {"type": "array", "items": string("Verifiable completion criterion")},
                "max_steps": integer(),
                "timeout_ms": integer(),
                "model_route": string("Optional model route")
            }),
            &["role", "prompt"],
        ),
        "hive_register_role" => object(
            serde_json::json!({
                "role": string("Lowercase role id [a-z0-9_-]"),
                "display_name": string("Human-readable sector/role name"),
                "department": string("Department or sector id"),
                "objective": string("Role objective"),
                "capabilities": {"type": "array", "items": string("Requested capability (workspace.read, workspace.search, workspace.write, test.execute, git_status, git_diff, git_commit)")},
                "strategy": string("Optional strategy description")
            }),
            &["role", "display_name", "department", "objective"],
        ),
        "memory_search" => object(
            serde_json::json!({"query":string("Search query"),"limit":integer()}),
            &["query"],
        ),
        "goal_create" => object(
            serde_json::json!({
                "objective": string("Clear, measurable goal objective"),
                "success_criteria": {
                    "type": "array",
                    "items": string("Concrete verifiable criterion statement")
                },
                "continuation_budget": integer()
            }),
            &["objective"],
        ),
        "goal_verify" => object(
            serde_json::json!({"goal_id":string("Active Core Goal ID"),"criterion_id":string("Criterion ID to mark verified"),"evidence_summary":string("Concise summary of verified tool output")}),
            &["goal_id", "criterion_id", "evidence_summary"],
        ),
        "git_commit" => object(
            serde_json::json!({"message":string("Commit message")}),
            &["message"],
        ),
        // Each Git tool declares its own closed schema. A shared superset used
        // to promise `reference`/`stat_only` on `git_diff`, so a model could
        // build a call the executor then rejected.
        "git_status" | "git_changed_files" => object(serde_json::json!({}), &[]),
        "git_diff" => object(
            serde_json::json!({"path":string("Optional workspace path to limit the diff")}),
            &[],
        ),
        "git_log" => object(
            serde_json::json!({"max_count":integer(),"path":string("Optional workspace path")}),
            &[],
        ),
        "git_show" => object(
            serde_json::json!({"reference":string("Optional revision, defaults to HEAD"),"stat_only":{"type":"boolean"}}),
            &[],
        ),
        "git_blame" => object(
            serde_json::json!({"path":string("Workspace-relative file path"),"start_line":integer(),"end_line":integer()}),
            &["path"],
        ),
        "git_pull" => object(
            serde_json::json!({"remote":string("Optional configured remote"),"branch":string("Optional branch")}),
            &[],
        ),
        "git_push" => object(
            serde_json::json!({"remote":string("Optional configured remote"),"branch":string("Optional branch"),"force":{"type":"boolean"}}),
            &[],
        ),
        "git_branch" => object(
            serde_json::json!({"action":string("create, switch, list, delete, or rename"),"name":string("Branch name"),"from":string("Optional base reference")}),
            &["action"],
        ),
        "git_merge" => object(
            serde_json::json!({"branch":string("Branch to merge"),"strategy":string("Optional merge strategy"),"no_ff":{"type":"boolean"}}),
            &["branch"],
        ),
        "git_reset" => object(
            serde_json::json!({"commit":string("Target revision"),"mode":string("soft, mixed, or hard")}),
            &["commit"],
        ),
        "git_revert" => object(
            serde_json::json!({"commit":string("Commit to revert"),"message":string("Optional commit message")}),
            &["commit"],
        ),
        "git_cherry_pick" => object(
            serde_json::json!({"commit":string("Commit to apply")}),
            &["commit"],
        ),
        "git_rebase" => object(
            serde_json::json!({"onto":string("Base revision to rebase onto"),"interactive":{"type":"boolean"}}),
            &["onto"],
        ),
        "git_tag" => object(
            serde_json::json!({"action":string("create, list, or delete"),"name":string("Tag name"),"commit":string("Optional target revision"),"message":string("Optional annotated tag message")}),
            &["action"],
        ),
        "git_stash" => object(
            serde_json::json!({"action":string("list, save, apply, pop, or drop"),"message":string("Optional stash message"),"index":string("Optional stash index"),"include_untracked":{"type":"boolean"}}),
            &["action"],
        ),
        "git_remote" => object(
            serde_json::json!({"action":string("list, add, or remove"),"name":string("Remote name"),"url":string("Remote URL")}),
            &["action"],
        ),
        "mcp_call" => object(
            serde_json::json!({"server_id":string("Core-owned MCP server identity"),"tool_name":string("Allowlisted MCP tool"),"params":{},"timeout_ms":integer()}),
            &["server_id", "tool_name"],
        ),
        "browser_open" | "http_fetch" => object(
            serde_json::json!({"url":string("URL resolved by policy"),"max_chars":integer(),"timeout_ms":integer()}),
            &["url"],
        ),
        "browser_extract" => object(
            serde_json::json!({"url":string("URL"),"selector":string("CSS selector"),"attribute":string("Optional attribute"),"limit":integer(),"timeout_ms":integer()}),
            &["url", "selector"],
        ),
        "app_open" => object(
            serde_json::json!({"app":string("Application name or alias from the local catalog")}),
            &["app"],
        ),
        "app_list" => object(
            serde_json::json!({"query":string("Optional substring filter"),"limit":integer()}),
            &[],
        ),
        "process_run" => object(
            serde_json::json!({"command":string("Executable name; launched directly without a shell"),"args":{"type":"array","items":{"type":"string"}},"cwd":string("Working directory inside the task workspace"),"timeout_ms":integer()}),
            &["command"],
        ),
        "shell_execute" | "process.spawn" => object(
            serde_json::json!({"program":string("Executable"),"args":{"type":"array","items":{"type":"string"}},"cwd":string("Working directory"),"timeout_ms":integer()}),
            &["program"],
        ),
        "logs_tail" => object(
            serde_json::json!({"path":string("Workspace-relative log path"),"lines":integer()}),
            &["path"],
        ),
        "logs_grep" => object(
            serde_json::json!({"pattern":string("Literal text to find"),"path":string("Optional file or directory"),"case_insensitive":{"type":"boolean"},"invert_match":{"type":"boolean"},"context_lines":integer()}),
            &["pattern"],
        ),
        "cargo_build" => object(
            serde_json::json!({"release":{"type":"boolean"},"package":string("Optional package"),"features":string("Optional comma-separated features")}),
            &[],
        ),
        "cargo_test" => object(
            serde_json::json!({"test_name":string("Optional test filter"),"package":string("Optional package"),"lib":{"type":"boolean"},"doc":{"type":"boolean"}}),
            &[],
        ),
        "cargo_fmt" => object(serde_json::json!({"check":{"type":"boolean"}}), &[]),
        "cargo_clippy" => object(
            serde_json::json!({"package":string("Optional package"),"all_targets":{"type":"boolean"}}),
            &[],
        ),
        "cargo_check" => object(
            serde_json::json!({"package":string("Optional package")}),
            &[],
        ),
        "archive_create" => object(
            serde_json::json!({"source":string("Workspace path to archive"),"destination":string("Output archive path"),"format":string("tar, gz, tar.gz, or zip")}),
            &["source", "destination"],
        ),
        "archive_extract" => object(
            serde_json::json!({"archive":string("Workspace archive path"),"destination":string("Optional output directory")}),
            &["archive"],
        ),
        "archive_list" => object(
            serde_json::json!({"archive":string("Workspace archive path")}),
            &["archive"],
        ),
        // Retired raw CDP names stay registered for diagnostics only: the
        // executor always fails closed with `legacy_disabled`, and the typed
        // Core IPC browser surface is the supported path.
        "browser_session_navigate"
        | "browser_session_read"
        | "browser_session_click"
        | "browser_session_screenshot"
        | "browser_session_type"
        | "browser_session_close" => object(serde_json::json!({}), &[]),
        _ => object(
            serde_json::json!({"input":{"type":"object","description":"Tool-specific structured input"}}),
            &[],
        ),
    }
}

/// Where the tool implementation comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolOrigin {
    /// Tool shipped with this runtime.
    Builtin,
    /// Tool exposed by a configured Model Context Protocol server.
    Mcp,
    /// Tool discovered from an installed catalog package.
    Catalog,
}

/// Declared class of effects a tool may cause.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    /// The tool reads or computes without changing external state.
    ReadOnly,
    /// The tool may change workspace or application state.
    Mutating,
    /// The tool may remove or irreversibly replace data.
    Destructive,
    /// The tool communicates with a network service.
    Network,
}

/// Approval requirement applied before a tool call is dispatched.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// The tool never requires a separate approval.
    Never,
    /// Approval is required when permission policy requests it.
    OnPermission,
    /// Every invocation requires an approval decision.
    Always,
}

/// Versioned description of a tool's interface and execution constraints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolManifest {
    /// Schema discriminator; must equal [`MANIFEST_KIND`].
    pub kind: String,
    /// Stable identifier used to select this tool.
    pub tool_id: String,
    /// Version of the tool implementation.
    pub version: String,
    /// Display label for user-facing surfaces.
    pub display_name: String,
    /// Human-readable description of the tool's purpose.
    pub description: String,
    /// JSON Schema for accepted input; must be an object.
    pub input_schema: Value,
    /// JSON Schema for produced output; must be an object.
    pub output_schema: Value,
    /// Capability category used by policy and discovery.
    pub capability_class: String,
    /// Broad class of effects the tool may perform.
    pub side_effect: SideEffectClass,
    /// Identity of the provider that supplies the implementation.
    pub provider_identity: String,
    /// Permissions required before invocation.
    pub required_permissions: Vec<Permission>,
    /// Approval policy declared by the tool.
    pub approval: ApprovalMode,
    /// Workspace boundary applied to the tool.
    pub workspace_scope: String,
    /// Network hosts the tool may contact.
    pub network_domains: Vec<String>,
    /// Secret identifiers the tool is allowed to reference.
    pub secret_references: Vec<String>,
    /// Maximum call duration in milliseconds.
    pub timeout_ms: u64,
    /// Maximum output size in bytes.
    pub output_size_limit: u64,
    /// Retry behavior category for the implementation.
    pub retry_class: String,
    /// Whether the implementation accepts cancellation.
    pub supports_cancellation: bool,
    /// Origin category of the implementation.
    pub origin: ToolOrigin,
    /// Reference identifying the source of the implementation.
    pub source_reference: String,
    /// Optional package digest for catalog-provided tools.
    pub package_hash: Option<String>,
    /// Optional license identifier for the implementation package.
    pub license: Option<String>,
    /// Core version range supported by this tool.
    pub compatible_core: String,
    /// Tool manifest protocol version.
    pub protocol_version: String,
}

/// Validation failures for a tool manifest.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    /// The discriminator does not match the supported manifest schema.
    #[error("manifest kind must be {MANIFEST_KIND}")]
    WrongKind,
    /// A required string field is empty.
    #[error("manifest field is empty: {0}")]
    EmptyField(&'static str),
    /// An input or output schema is not a JSON object.
    #[error("schema must be a JSON object: {0}")]
    InvalidSchema(&'static str),
    /// A schema allows undeclared properties.
    #[error("manifest contains permissive additionalProperties schema")]
    PermissiveSchema,
    /// Timeout or maximum output size is zero.
    #[error("invalid timeout or output limit")]
    InvalidLimits,
}

impl ToolManifest {
    /// Checks required fields, object schemas, strict properties, and limits.
    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.kind != MANIFEST_KIND {
            return Err(ManifestError::WrongKind);
        }
        for (n, v) in [
            ("tool_id", &self.tool_id),
            ("version", &self.version),
            ("provider_identity", &self.provider_identity),
            ("compatible_core", &self.compatible_core),
            ("protocol_version", &self.protocol_version),
        ] {
            if v.trim().is_empty() {
                return Err(ManifestError::EmptyField(n));
            }
        }
        for (name, schema) in [
            ("input", &self.input_schema),
            ("output", &self.output_schema),
        ] {
            if !schema.is_object() {
                return Err(ManifestError::InvalidSchema(name));
            }
            if schema.get("additionalProperties") == Some(&Value::Bool(true)) {
                return Err(ManifestError::PermissiveSchema);
            }
        }
        if self.timeout_ms == 0 || self.output_size_limit == 0 {
            return Err(ManifestError::InvalidLimits);
        }
        Ok(())
    }

    /// Serializes this manifest to JSON bytes.
    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Computes a SHA-256 digest over the serialized manifest.
    pub fn canonical_hash(&self) -> Result<String, serde_json::Error> {
        let mut h = Sha256::new();
        h.update(self.canonical_json()?);
        Ok(format!("sha256:{}", hex::encode(h.finalize())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> ToolManifest {
        ToolManifest {
            kind: MANIFEST_KIND.into(),
            tool_id: "test.read".into(),
            version: "1.0.0".into(),
            display_name: "Read".into(),
            description: "Read".into(),
            input_schema: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}),
            output_schema: serde_json::json!({"type":"object"}),
            capability_class: "filesystem_read".into(),
            side_effect: SideEffectClass::ReadOnly,
            provider_identity: "builtin".into(),
            required_permissions: vec![Permission::FilesystemRead],
            approval: ApprovalMode::OnPermission,
            workspace_scope: "workspace".into(),
            network_domains: vec![],
            secret_references: vec![],
            timeout_ms: 1000,
            output_size_limit: 1024,
            retry_class: "none".into(),
            supports_cancellation: true,
            origin: ToolOrigin::Builtin,
            source_reference: "builtin".into(),
            package_hash: None,
            license: Some("MIT".into()),
            compatible_core: ">=0.1".into(),
            protocol_version: "1".into(),
        }
    }
    #[test]
    fn round_trip_and_hash_are_stable() {
        let m = manifest();
        m.validate().unwrap();
        assert_eq!(m.canonical_hash().unwrap(), m.canonical_hash().unwrap());
        assert_eq!(
            serde_json::from_slice::<ToolManifest>(&m.canonical_json().unwrap()).unwrap(),
            m
        );
    }
    #[test]
    fn permissive_schema_is_rejected() {
        let mut m = manifest();
        m.input_schema = serde_json::json!({"type":"object","additionalProperties":true});
        assert_eq!(m.validate(), Err(ManifestError::PermissiveSchema));
    }

    #[test]
    fn process_run_schema_matches_the_direct_executor_input() {
        let schema = builtin_input_schema("process_run");

        assert_eq!(schema["required"], serde_json::json!(["command"]));
        assert_eq!(schema["properties"]["command"]["type"], "string");
        assert_eq!(schema["properties"]["args"]["type"], "array");
        assert_eq!(schema["properties"]["cwd"]["type"], "string");
        assert!(schema["properties"].get("input").is_none());
    }

    #[test]
    fn workflow_tool_schemas_are_explicit_and_closed() {
        for (tool, required) in [
            ("workflow_catalog", serde_json::json!([])),
            (
                "workflow_start",
                serde_json::json!(["template_id", "inputs"]),
            ),
            ("workflow_status", serde_json::json!(["run_id"])),
            ("workflow_cancel", serde_json::json!(["run_id"])),
        ] {
            let schema = builtin_input_schema(tool);
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(schema["required"], required);
        }
        let start = builtin_input_schema("workflow_start");
        assert_eq!(
            start["properties"]["inputs"]["additionalProperties"]["type"],
            "string"
        );
    }

    #[test]
    fn self_development_schemas_are_explicit_and_closed() {
        for (tool, required) in [
            ("eva_workspace_read_file", serde_json::json!(["path"])),
            (
                "eva_workspace_write_file",
                serde_json::json!(["path", "content"]),
            ),
            ("eva_workspace_run_command", serde_json::json!(["command"])),
            ("eva_workspace_git", serde_json::json!(["action"])),
        ] {
            let schema = builtin_input_schema(tool);
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["additionalProperties"], false);
            assert_eq!(schema["required"], required);
        }
        assert_eq!(
            builtin_input_schema("eva_workspace_git")["properties"]["action"]["enum"],
            serde_json::json!(["status", "diff", "commit", "push"])
        );
    }

    #[test]
    fn patch_schema_requires_the_revision_precondition_the_executor_reads() {
        // tools/patch.rs deserializes expected_hash as a required field. The
        // declared schema must say the same, otherwise the catalog probe is
        // rejected and filesystem_patch silently leaves the model catalog.
        let schema = builtin_input_schema("filesystem_patch");

        assert_eq!(
            schema["required"],
            serde_json::json!(["path", "expected_hash"])
        );
        assert_eq!(schema["properties"]["expected_hash"]["type"], "string");
        assert_eq!(schema["properties"]["old_text"]["type"], "string");
        assert_eq!(schema["properties"]["new_text"]["type"], "string");
    }

    #[test]
    fn no_registered_builtin_tool_falls_back_to_the_generic_input_schema() {
        // The fallback advertises a lone `input` field, so a model faithfully
        // sends `{"input":{}}` to a tool whose executor expects `path` or no
        // arguments at all. Every registered tool must declare its own schema.
        let fallback = builtin_input_schema("__no_such_tool__");
        assert!(fallback["properties"].get("input").is_some());

        for manifest in crate::ToolRegistry::bootstrap().manifests() {
            assert_ne!(
                manifest.input_schema, fallback,
                "{} declares no explicit input schema and would advertise a generic `input` field",
                manifest.tool_id
            );
        }
    }

    #[test]
    fn canonical_tool_id_rewrites_only_the_historical_separator() {
        assert_eq!(canonical_tool_id("git.status"), "git_status");
        assert_eq!(
            canonical_tool_id("git.worktree.create"),
            "git_worktree_create"
        );
        assert_eq!(canonical_tool_id("git_status"), "git_status");
        assert_eq!(canonical_tool_id("plain"), "plain");
    }

    #[test]
    fn tool_namespace_splits_both_canonical_and_historical_ids() {
        assert_eq!(tool_namespace("filesystem_read"), "filesystem");
        assert_eq!(tool_namespace("filesystem.read"), "filesystem");
        assert_eq!(tool_namespace("git_worktree_create"), "git");
        assert_eq!(tool_namespace("git.worktree.create"), "git");
        assert_eq!(tool_namespace("shell_execute"), "shell");
        // An id without a separator is its own namespace.
        assert_eq!(tool_namespace("plain"), "plain");
    }

    #[test]
    fn git_readers_declare_exactly_the_fields_their_executors_accept() {
        let status = builtin_input_schema("git_status");
        assert_eq!(status["additionalProperties"], false);
        assert!(
            status["properties"]
                .as_object()
                .is_none_or(|properties| properties.is_empty()),
            "git_status takes no arguments"
        );

        let diff = builtin_input_schema("git_diff");
        assert_eq!(diff["properties"]["path"]["type"], "string");
        assert!(
            diff["properties"].get("reference").is_none(),
            "git_diff must not advertise the git_show `reference` field"
        );

        let show = builtin_input_schema("git_show");
        assert_eq!(show["properties"]["reference"]["type"], "string");
        assert_eq!(show["properties"]["stat_only"]["type"], "boolean");
    }
}
