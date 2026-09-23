//! Core-owned, revision-bound language intelligence metadata contract.
//! Provider processes and workspace mutation remain owned by existing
//! supervisor/policy/edit subsystems; this module never executes raw LSP JSON.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Serialized schema version supported by language-intelligence records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum entries accepted in a language-intelligence collection.
pub const MAX_ITEMS: usize = 128;
/// Maximum character count for contract metadata values.
pub const MAX_TEXT: usize = 512;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Publication lifecycle of a language-server descriptor.
pub enum Lifecycle {
    /// Descriptor has not been approved for registration.
    Draft,
    /// Descriptor is eligible for session creation.
    Active,
    /// A newer descriptor revision replaced this one.
    Superseded,
    /// A contract field or invariant is invalid.
    Invalid,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Supported transport for a language-server adapter.
pub enum Transport {
    /// Supervisor-managed standard input/output transport.
    Stdio,
    /// Supervisor-managed local socket transport.
    LocalSocket,
    /// Explicitly registered remote bridge transport.
    RegisteredRemoteBridge,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Workspace root layout supported by a language server.
pub enum WorkspaceMode {
    /// Provider accepts one workspace root.
    SingleRoot,
    /// Provider accepts several workspace roots.
    MultiRoot,
    /// Provider supports either workspace layout.
    Either,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Trust decision required before a provider may be used.
pub enum TrustClass {
    /// Provider is bundled and maintained by the application.
    BuiltIn,
    /// Provider was explicitly registered by an owner.
    ExplicitlyRegistered,
    /// Provider was approved by an applicable policy.
    PolicyApproved,
    /// Provider is not trusted and cannot be admitted.
    Untrusted,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Lifecycle state of one language-server session.
pub enum SessionState {
    /// Session record exists but startup has not begun.
    Created,
    /// Supervisor is launching the provider.
    Starting,
    /// Provider handshake and capability negotiation are in progress.
    Initializing,
    /// Provider is initialized and available for validated queries.
    Ready,
    /// Provider is available with reduced health or capabilities.
    Degraded,
    /// Supervisor is recovering the provider process.
    Restarting,
    /// Provider shutdown is in progress.
    Stopping,
    /// Provider session is stopped.
    Stopped,
    /// Proposal validation or application failed.
    Failed,
    /// Provider does not support the requested contract or workspace.
    Unsupported,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Whether a semantic result matches current source revisions.
pub enum Freshness {
    /// Captured document and provider revisions match current revisions.
    Fresh,
    /// The result was built from an outdated revision.
    Stale,
    /// Freshness could not be established.
    Unknown,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Validation and application state of a workspace-edit proposal.
pub enum ProposalStatus {
    /// Edit proposal has not passed validation.
    Proposed,
    /// Edit proposal passed structural and revision validation.
    Validated,
    /// Edit proposal awaits an independent approval decision.
    NeedsApproval,
    /// The result was built from an outdated revision.
    Stale,
    /// Edit proposal was rejected.
    Rejected,
    /// Edit proposal was applied by the authorized edit subsystem.
    Applied,
    /// Some proposed operations could not be represented.
    PartiallyUnsupported,
    /// Proposal validation or application failed.
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Revision-bound metadata and trust requirements for a language server.
pub struct LanguageServerDescriptor {
    /// Serialized schema version supported by this record.
    pub schema_version: u32,
    /// Stable descriptor, session, or proposal identifier.
    pub id: String,
    /// Monotonic descriptor or workspace edit revision.
    pub revision: u64,
    /// Publication lifecycle of the descriptor.
    pub lifecycle: Lifecycle,
    /// Human-readable language server name.
    pub display_name: String,
    /// Language identifiers supported by the provider.
    pub languages: Vec<String>,
    /// File patterns used to select documents for this server.
    pub file_patterns: Vec<String>,
    /// Owner-managed executable reference; raw command text is not accepted.
    pub executable_ref: String,
    /// Validated argument template with no executable path expansion.
    pub argv_template: Vec<String>,
    /// Transport used by the supervisor-managed provider.
    pub transport: Transport,
    /// Optional reference to validated initialization options.
    pub initialization_options_ref: Option<String>,
    /// Optional execution environment snapshot reference.
    pub environment_profile_ref: Option<String>,
    /// Single-root or multi-root workspace support declaration.
    pub workspace_mode: WorkspaceMode,
    /// Registration and trust state used for admission.
    pub trust_class: TrustClass,
    /// Capabilities expected from the provider, checked by policy.
    pub capability_expectations: Vec<String>,
    /// Integrity hash of canonical descriptor, session, or result content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Process and capability references for one server session.
pub struct LanguageServerSession {
    /// Stable descriptor, session, or proposal identifier.
    pub id: String,
    /// Descriptor revision used to create this session.
    pub descriptor_ref: String,
    /// Workspace binding used by the session.
    pub workspace_binding_ref: String,
    /// Optional worktree identity bound to the session.
    pub worktree_ref: Option<String>,
    /// Optional supervisor-owned process reference.
    pub process_ref: Option<String>,
    /// Current session or proposal lifecycle state.
    pub state: SessionState,
    /// Optional immutable capability snapshot reference.
    pub capability_snapshot_ref: Option<String>,
    /// Session start time as Unix epoch milliseconds.
    pub started_at_ms: i64,
    /// Optional ready time as Unix epoch milliseconds.
    pub ready_at_ms: Option<i64>,
    /// Optional stable reference to failure diagnostics.
    pub failure_ref: Option<String>,
    /// Restart generation used to reject stale session results.
    pub generation: u64,
    /// Integrity hash of canonical descriptor, session, or result content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Canonical workspace document identity and revision.
pub struct LanguageDocumentRef {
    /// Workspace identifier containing the document.
    pub workspace_ref: String,
    /// Canonical file reference resolved by the workspace owner.
    pub canonical_file_ref: String,
    /// Document URI passed through the provider adapter.
    pub uri: String,
    /// Language identifier associated with the document.
    pub language_id: String,
    /// Monotonic document revision at query time.
    pub file_revision: u64,
    /// Integrity hash of canonical descriptor, session, or result content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Immutable query inputs and source revisions captured at request time.
pub struct LanguageQuerySnapshot {
    /// Stable identifier for this semantic query.
    pub query_ref: String,
    /// Language-server session that handled the query.
    pub session_ref: String,
    /// Document identity and revision supplied to the query.
    pub document: LanguageDocumentRef,
    /// Document revision captured by this query.
    pub document_revision: u64,
    /// Provider revision captured when the query ran.
    pub provider_revision: u64,
    /// Optional immutable capability snapshot reference.
    pub capability_snapshot_ref: String,
    /// Semantic operation requested from the provider.
    pub query_kind: String,
    /// Optional source position or range encoded by the adapter.
    pub position_or_range: Option<String>,
    /// Query creation time as Unix epoch milliseconds.
    pub requested_at_ms: i64,
    /// Integrity hash of canonical descriptor, session, or result content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Bounded references to semantic results tied to a query and provider revision.
pub struct LanguageSemanticResult {
    /// Stable identifier for this semantic query.
    pub query_ref: String,
    /// Provider descriptor or session reference that produced the result.
    pub provider_ref: String,
    /// Provider revision captured when the query ran.
    pub provider_revision: u64,
    /// Source document references used to produce the result.
    pub source_documents: Vec<String>,
    /// Bounded references to structured semantic results.
    pub result_refs: Vec<String>,
    /// Whether the provider result was truncated to contract limits.
    pub truncated: bool,
    /// Number of result items omitted by truncation.
    pub omitted_count: u32,
    /// Comparison of captured revisions with current revisions.
    pub freshness: Freshness,
    /// Result observation time as Unix epoch milliseconds.
    pub observed_at_ms: i64,
    /// Integrity hash of canonical descriptor, session, or result content.
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
/// Provider-proposed workspace edit bound to a base workspace fingerprint.
pub struct LanguageWorkspaceEditProposal {
    /// Stable descriptor, session, or proposal identifier.
    pub id: String,
    /// Stable identifier for this semantic query.
    pub query_ref: String,
    /// Provider descriptor or session reference that produced the result.
    pub provider_ref: String,
    /// Workspace fingerprint against which edits were proposed.
    pub base_workspace_fingerprint: String,
    /// References to validated edit operations.
    pub edit_refs: Vec<String>,
    /// Operations the provider proposed but the adapter cannot represent.
    pub unsupported_operations: Vec<String>,
    /// Current validation or application status.
    pub status: ProposalStatus,
    /// Integrity hash of canonical descriptor, session, or result content.
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
/// Invalid contract data, exceeded bound, stale state, or untrusted provider.
pub enum LanguageIntelligenceError {
    /// A contract field or invariant is invalid.
    #[error("invalid language intelligence field: {0}")]
    Invalid(&'static str),
    /// A collection or metadata bound was exceeded.
    #[error("language intelligence limit exceeded: {0}")]
    Limit(&'static str),
    /// The result was built from an outdated revision.
    #[error("stale language intelligence revision")]
    Stale,
    /// Executable reference failed the trust boundary.
    #[error("unsafe language server executable")]
    UntrustedExecutable,
    /// The requested session lifecycle transition is not allowed.
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
/// Computes the descriptor hash with its stored hash field cleared.
pub fn descriptor_hash(v: &LanguageServerDescriptor) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
/// Computes the session hash with its stored hash field cleared.
pub fn session_hash(v: &LanguageServerSession) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
/// Computes the query hash with its stored hash field cleared.
pub fn query_hash(v: &LanguageQuerySnapshot) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
/// Computes the semantic result hash with its stored hash field cleared.
pub fn result_hash(v: &LanguageSemanticResult) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}
/// Computes the workspace edit proposal hash with its stored hash field cleared.
pub fn proposal_hash(
    v: &LanguageWorkspaceEditProposal,
) -> Result<String, LanguageIntelligenceError> {
    hash(v, |x| x.content_hash.clear())
}

/// Checks descriptor schema, trust, transport, argument bounds, and hash.
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
/// Checks session references, generation, timing, and integrity hash.
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
/// Checks query identifiers, captured revisions, timestamp, and integrity hash.
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
/// Validates the requested language-server lifecycle transition.
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
/// Compares captured query revisions to current document and provider revisions.
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
