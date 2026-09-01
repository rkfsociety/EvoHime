use evohime_permissions::Permission;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MANIFEST_KIND: &str = "tool/manifest/v1";

/// Canonical input-schema catalog for builtin tools.  The registry and Core
/// both consume this function; tool implementations remain responsible for
/// semantic validation after the structural contract has been checked.
pub fn builtin_input_schema(tool_id: &str) -> Value {
    let string = |description: &str| serde_json::json!({"type":"string","description":description});
    let integer = || serde_json::json!({"type":"integer","minimum":1});
    let object = |properties: Value, required: &[&str]| serde_json::json!({"type":"object","properties":properties,"required":required,"additionalProperties":false});
    match tool_id {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolOrigin {
    Builtin,
    Mcp,
    Catalog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectClass {
    ReadOnly,
    Mutating,
    Destructive,
    Network,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Never,
    OnPermission,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolManifest {
    pub kind: String,
    pub tool_id: String,
    pub version: String,
    pub display_name: String,
    pub description: String,
    pub input_schema: Value,
    pub output_schema: Value,
    pub capability_class: String,
    pub side_effect: SideEffectClass,
    pub provider_identity: String,
    pub required_permissions: Vec<Permission>,
    pub approval: ApprovalMode,
    pub workspace_scope: String,
    pub network_domains: Vec<String>,
    pub secret_references: Vec<String>,
    pub timeout_ms: u64,
    pub output_size_limit: u64,
    pub retry_class: String,
    pub supports_cancellation: bool,
    pub origin: ToolOrigin,
    pub source_reference: String,
    pub package_hash: Option<String>,
    pub license: Option<String>,
    pub compatible_core: String,
    pub protocol_version: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("manifest kind must be {MANIFEST_KIND}")]
    WrongKind,
    #[error("manifest field is empty: {0}")]
    EmptyField(&'static str),
    #[error("schema must be a JSON object: {0}")]
    InvalidSchema(&'static str),
    #[error("manifest contains permissive additionalProperties schema")]
    PermissiveSchema,
    #[error("invalid timeout or output limit")]
    InvalidLimits,
}

impl ToolManifest {
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

    pub fn canonical_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

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
