use evohime_permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Discriminator identifying the supported tool manifest schema.
pub const MANIFEST_KIND: &str = "tool/manifest/v1";

/// Canonical input-schema catalog for builtin tools.  The registry and Core
/// both consume this function; tool implementations remain responsible for
/// semantic validation after the structural contract has been checked.
pub fn builtin_input_schema(tool_id: &str) -> Value {
    let string = |description: &str| serde_json::json!({"type":"string","description":description});
    let integer = || serde_json::json!({"type":"integer","minimum":1});
    let object = |properties: Value, required: &[&str]| serde_json::json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    match tool_id {
        "utility.base64.encode"
        | "utility.base64.decode"
        | "utility.hash.sha256"
        | "utility.hash.sha512"
        | "utility.json.format"
        | "utility.json.minify" => object(
            serde_json::json!({"text":string("Bounded UTF-8 input")}),
            &["text"],
        ),
        "utility.uuid.v4" => object(serde_json::json!({}), &[]),
        "utility.token.generate" => object(
            serde_json::json!({"bytes":{"type":"integer","minimum":1,"maximum":128}}),
            &[],
        ),
        "utility.text.case_convert" => object(
            serde_json::json!({"text":string("Bounded UTF-8 input"),"mode":{"type":"string","enum":["lower","upper","snake","kebab","camel"]}}),
            &["text"],
        ),
        "filesystem.read" => object(
            serde_json::json!({"path":string("Workspace-relative file path")}),
            &["path"],
        ),
        "filesystem.write" => object(
            serde_json::json!({"path":string("Logical namespace/path"),"content":string("Complete UTF-8 content"),"expected_hash":string("SHA-256 hash from filesystem.read")}),
            &["path", "content"],
        ),
        "filesystem.patch" => object(
            serde_json::json!({"path":string("Logical namespace/path"),"patch":string("Unified diff"),"expected_hash":string("SHA-256 hash from filesystem.read")}),
            &["path", "patch"],
        ),
        "filesystem.delete" => object(
            serde_json::json!({"path":string("Logical namespace/path"),"recursive":{"type":"boolean"},"expected_hash":string("SHA-256 hash of the file before deletion")}),
            &["path"],
        ),
        "git.worktree.create" => object(
            serde_json::json!({"worktree_id":string("Core-generated task worktree identity"),"base_commit":string("Validated Git commit/ref")}),
            &["worktree_id", "base_commit"],
        ),
        "git.worktree.remove" => object(
            serde_json::json!({"worktree_id":string("Core-generated task worktree identity")}),
            &["worktree_id"],
        ),
        "git.worktree.preflight" => object(
            serde_json::json!({"worktree_id":string("Core-generated task worktree identity"),"base_commit":string("Pinned base commit")}),
            &["worktree_id", "base_commit"],
        ),
        "filesystem.move" | "filesystem.copy" => object(
            serde_json::json!({"from":string("Logical namespace/path"),"to":string("Logical namespace/path"),"recursive":{"type":"boolean"},"expected_hash":string("SHA-256 hash of the source file")}),
            &["from", "to"],
        ),
        "filesystem.search" => object(
            serde_json::json!({"query":string("Text or pattern"),"path":string("Optional workspace path"),"glob":string("Optional glob"),"limit":integer()}),
            &["query"],
        ),
        "filesystem.list" => object(
            serde_json::json!({"path":string("Workspace-relative directory")}),
            &[],
        ),
        "agent.run" => object(
            serde_json::json!({"prompt":string("Task prompt"),"max_steps":integer(),"timeout_ms":integer(),"model_route":string("Optional model route")}),
            &["prompt"],
        ),
        "memory.search" => object(
            serde_json::json!({"query":string("Search query"),"limit":integer()}),
            &["query"],
        ),
        "git.commit" => object(
            serde_json::json!({"message":string("Commit message")}),
            &["message"],
        ),
        "git.diff" | "git.log" | "git.show" | "git.blame" | "git.pull" | "git.push" => object(
            serde_json::json!({"path":string("Optional workspace path"),"reference":string("Optional revision"),"remote":string("Optional remote"),"branch":string("Optional branch"),"force":{"type":"boolean"},"max_count":integer()}),
            &[],
        ),
        "mcp.call" => object(
            serde_json::json!({"server_id":string("Core-owned MCP server identity"),"tool_name":string("Allowlisted MCP tool"),"params":{},"timeout_ms":integer()}),
            &["server_id", "tool_name"],
        ),
        "browser.open" | "browser.session.navigate" | "http.fetch" => object(
            serde_json::json!({"url":string("URL resolved by policy"),"max_chars":integer(),"timeout_ms":integer()}),
            &["url"],
        ),
        "browser.extract" => object(
            serde_json::json!({"url":string("URL"),"selector":string("CSS selector"),"attribute":string("Optional attribute"),"limit":integer(),"timeout_ms":integer()}),
            &["url", "selector"],
        ),
        "app.open" => object(
            serde_json::json!({"app":string("Application name or alias from the local catalog")}),
            &["app"],
        ),
        "app.list" => object(
            serde_json::json!({"query":string("Optional substring filter"),"limit":integer()}),
            &[],
        ),
        "shell.execute" | "process.spawn" => object(
            serde_json::json!({"program":string("Executable"),"args":{"type":"array","items":{"type":"string"}},"cwd":string("Working directory"),"timeout_ms":integer()}),
            &["program"],
        ),
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
        Ok(format!("sha256:{:x}", h.finalize()))
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
            capability_class: "filesystem.read".into(),
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
}
