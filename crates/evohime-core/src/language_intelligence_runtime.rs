//! Core-owned, revision-bound language intelligence metadata contract.
//! Provider processes and workspace mutation remain owned by existing
//! supervisor/policy/edit subsystems; this module never executes raw LSP JSON.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_ITEMS: usize = 128;
pub const MAX_TEXT: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Draft,
    Active,
    Superseded,
    Invalid,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Stdio,
    LocalSocket,
    RegisteredRemoteBridge,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    SingleRoot,
    MultiRoot,
    Either,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustClass {
    BuiltIn,
    ExplicitlyRegistered,
    PolicyApproved,
    Untrusted,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Starting,
    Initializing,
    Ready,
    Degraded,
    Restarting,
    Stopping,
    Stopped,
    Failed,
    Unsupported,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    Stale,
    Unknown,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Proposed,
    Validated,
    NeedsApproval,
    Stale,
    Rejected,
    Applied,
    PartiallyUnsupported,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageServerDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub lifecycle: Lifecycle,
    pub display_name: String,
    pub languages: Vec<String>,
    pub file_patterns: Vec<String>,
    pub executable_ref: String,
    pub argv_template: Vec<String>,
    pub transport: Transport,
    pub initialization_options_ref: Option<String>,
    pub environment_profile_ref: Option<String>,
    pub workspace_mode: WorkspaceMode,
    pub trust_class: TrustClass,
    pub capability_expectations: Vec<String>,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageServerSession {
    pub id: String,
    pub descriptor_ref: String,
    pub workspace_binding_ref: String,
    pub worktree_ref: Option<String>,
    pub process_ref: Option<String>,
    pub state: SessionState,
    pub capability_snapshot_ref: Option<String>,
    pub started_at_ms: i64,
    pub ready_at_ms: Option<i64>,
    pub failure_ref: Option<String>,
    pub generation: u64,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageDocumentRef {
    pub workspace_ref: String,
    pub canonical_file_ref: String,
    pub uri: String,
    pub language_id: String,
    pub file_revision: u64,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageQuerySnapshot {
    pub query_ref: String,
    pub session_ref: String,
    pub document: LanguageDocumentRef,
    pub document_revision: u64,
    pub provider_revision: u64,
    pub capability_snapshot_ref: String,
    pub query_kind: String,
    pub position_or_range: Option<String>,
    pub requested_at_ms: i64,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageSemanticResult {
    pub query_ref: String,
    pub provider_ref: String,
    pub provider_revision: u64,
    pub source_documents: Vec<String>,
    pub result_refs: Vec<String>,
    pub truncated: bool,
    pub omitted_count: u32,
    pub freshness: Freshness,
    pub observed_at_ms: i64,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageWorkspaceEditProposal {
    pub id: String,
    pub query_ref: String,
    pub provider_ref: String,
    pub base_workspace_fingerprint: String,
    pub edit_refs: Vec<String>,
    pub unsupported_operations: Vec<String>,
    pub status: ProposalStatus,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LanguageIntelligenceError {
    #[error("invalid language intelligence field: {0}")]
    Invalid(&'static str),
    #[error("language intelligence limit exceeded: {0}")]
    Limit(&'static str),
    #[error("stale language intelligence revision")]
    Stale,
    #[error("unsafe language server executable")]
    UntrustedExecutable,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
}

fn bounded(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= MAX_TEXT
}
fn hash<T: Serialize + Clone>(
    value: &T,
    clear: impl FnOnce(&mut T),
) -> Result<String, LanguageIntelligenceError> {
    let mut copy = value.clone();
    clear(&mut copy);
    let bytes =
        serde_json::to_vec(&copy).map_err(|_| LanguageIntelligenceError::Invalid("json"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}
pub fn descriptor_hash(v: &LanguageServerDescriptor) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
pub fn session_hash(v: &LanguageServerSession) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
pub fn query_hash(v: &LanguageQuerySnapshot) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
pub fn result_hash(v: &LanguageSemanticResult) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
pub fn proposal_hash(
    v: &LanguageWorkspaceEditProposal,
) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}

pub fn validate_descriptor(v: &LanguageServerDescriptor) -> Result<(), LanguageIntelligenceError> {
    if v.schema_version != SCHEMA_VERSION
        || v.revision == 0
        || !bounded(&v.id)
        || !bounded(&v.display_name)
        || !bounded(&v.executable_ref)
        || v.languages.is_empty()
        || v.languages.len() > MAX_ITEMS
        || v.file_patterns.len() > MAX_ITEMS
        || v.argv_template.len() > MAX_ITEMS
        || v.capability_expectations.len() > MAX_ITEMS
        || matches!(v.trust_class, TrustClass::Untrusted)
        || !matches!(v.transport, Transport::Stdio)
        || descriptor_hash(v)? != v.content_hash
    {
        return Err(LanguageIntelligenceError::Invalid("descriptor"));
    }
    if v.argv_template
        .iter()
        .any(|arg| !bounded(arg) || arg.contains("..") || arg.contains('/') || arg.contains('\\'))
    {
        return Err(LanguageIntelligenceError::UntrustedExecutable);
    }
    Ok(())
}
pub fn validate_session(v: &LanguageServerSession) -> Result<(), LanguageIntelligenceError> {
    if !bounded(&v.id)
        || !bounded(&v.descriptor_ref)
        || !bounded(&v.workspace_binding_ref)
        || v.started_at_ms <= 0
        || v.generation == 0
        || session_hash(v)? != v.content_hash
    {
        return Err(LanguageIntelligenceError::Invalid("session"));
    }
    Ok(())
}
pub fn validate_query(v: &LanguageQuerySnapshot) -> Result<(), LanguageIntelligenceError> {
    if !bounded(&v.query_ref)
        || !bounded(&v.session_ref)
        || !bounded(&v.capability_snapshot_ref)
        || !bounded(&v.query_kind)
        || v.document_revision == 0
        || v.provider_revision == 0
        || v.requested_at_ms <= 0
        || query_hash(v)? != v.content_hash
    {
        return Err(LanguageIntelligenceError::Invalid("query"));
    }
    Ok(())
}
pub fn transition_session(
    from: SessionState,
    to: SessionState,
) -> Result<(), LanguageIntelligenceError> {
    let valid = matches!(
        (from, to),
        (SessionState::Created, SessionState::Starting)
            | (SessionState::Starting, SessionState::Initializing)
            | (SessionState::Initializing, SessionState::Ready)
            | (
                SessionState::Ready,
                SessionState::Degraded | SessionState::Stopping | SessionState::Restarting
            )
            | (
                SessionState::Degraded,
                SessionState::Restarting | SessionState::Stopping
            )
            | (SessionState::Restarting, SessionState::Starting)
            | (SessionState::Stopping, SessionState::Stopped)
            | (
                SessionState::Starting | SessionState::Initializing,
                SessionState::Failed
            )
    );
    if valid {
        Ok(())
    } else {
        Err(LanguageIntelligenceError::InvalidTransition)
    }
}
pub fn result_freshness(
    query: &LanguageQuerySnapshot,
    document_revision: u64,
    provider_revision: u64,
) -> Freshness {
    if query.document_revision != document_revision || query.provider_revision != provider_revision
    {
        Freshness::Stale
    } else {
        Freshness::Fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_untrusted_descriptor() {
        let mut d = LanguageServerDescriptor {
            schema_version: SCHEMA_VERSION,
            id: "rust".into(),
            revision: 1,
            lifecycle: Lifecycle::Active,
            display_name: "Rust Analyzer".into(),
            languages: vec!["rust".into()],
            file_patterns: vec!["*.rs".into()],
            executable_ref: "managed:rust-analyzer".into(),
            argv_template: vec!["rust-analyzer".into()],
            transport: Transport::Stdio,
            initialization_options_ref: None,
            environment_profile_ref: None,
            workspace_mode: WorkspaceMode::SingleRoot,
            trust_class: TrustClass::Untrusted,
            capability_expectations: vec![],
            content_hash: String::new(),
        };
        d.content_hash = descriptor_hash(&d).unwrap();
        assert_eq!(
            validate_descriptor(&d),
            Err(LanguageIntelligenceError::Invalid("descriptor"))
        );
    }
    #[test]
    fn stale_result_is_never_fresh() {
        let q = LanguageQuerySnapshot {
            query_ref: "q".into(),
            session_ref: "s".into(),
            document: LanguageDocumentRef {
                workspace_ref: "w".into(),
                canonical_file_ref: "f".into(),
                uri: "file:///f".into(),
                language_id: "rust".into(),
                file_revision: 1,
                content_hash: "h".into(),
            },
            document_revision: 1,
            provider_revision: 1,
            capability_snapshot_ref: "c".into(),
            query_kind: "definition".into(),
            position_or_range: None,
            requested_at_ms: 1,
            content_hash: String::new(),
        };
        assert_eq!(result_freshness(&q, 2, 1), Freshness::Stale);
    }
}
