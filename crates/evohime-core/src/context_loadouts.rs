use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current schema version for context loadout profiles.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of context sources accepted in one profile.
const MAX_ENTRIES: usize = 256;
/// Lifecycle state of a context loadout profile.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoadoutStatus {
    /// Profile is being prepared and is not yet selectable.
    Draft,
    /// Profile is active for binding and resolution.
    Active,
    /// A newer profile revision replaced this profile.
    Superseded,
    /// Profile needs an owner or reviewer decision.
    NeedsReview,
    /// Profile failed validation.
    Invalid,
    /// Profile was disabled by its owner.
    Disabled,
}
/// Kind of context source referenced by a loadout entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Scoped memory view.
    MemoryView,
    /// Knowledge collection.
    KnowledgeCollection,
    /// Individual knowledge source.
    KnowledgeSource,
    /// Skill package or skill content.
    Skill,
    /// Project instruction or guidance source.
    ProjectGuidance,
    /// Repository metadata projection.
    RepositoryProjection,
    /// Artifact collection.
    ArtifactCollection,
    /// Scoped episodic experience view.
    ExperienceView,
    /// Research evidence collection.
    ResearchCollection,
    /// Custom source registered through the loadout registry.
    CustomRegistered,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Strategy for selecting a source revision during context resolution.
pub enum RevisionPolicy {
    /// Resolve only the exact source revision selected by the profile author.
    ExactPinned,
    /// Resolve the newest revision compatible with the run's constraints.
    LatestCompatibleAtRunStart,
    /// Resolve the newest active revision when a run begins.
    LatestActiveAtRunStart,
    /// Resolve the project binding captured when the conversation begins.
    FollowProjectBindingAtConversationStart,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Importance of a source to the successful use of a profile.
pub enum Requiredness {
    /// Missing this source prevents the loadout from being usable.
    Required,
    /// Prefer this source, but proceed if it cannot be resolved.
    Preferred,
    /// Include the source when available without affecting health when absent.
    Optional,
    /// Treat the source as a hint for resolution and ranking.
    Advisory,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Context activation behavior associated with a source entry.
pub enum UsageMode {
    /// Include a compact summary during initial context construction.
    BootstrapOverview,
    /// Keep a source catalog visible without loading its full content.
    AlwaysAvailableCatalog,
    /// Retrieve source content when the task context calls for it.
    AutoRetrieve,
    /// Load the source only after an explicit request.
    ExplicitOnly,
    /// Treat the source as active instructions for the run.
    InstructionActive,
    /// Use the source only as supporting evidence.
    EvidenceOnly,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
/// Aggregate availability state of a context resolution snapshot.
pub enum Health {
    /// Every required source resolved and no entries were rejected.
    Ready,
    /// Required sources resolved, with non-blocking issues present.
    ReadyWithWarnings,
    /// Resolution completed with rejected or unavailable optional content.
    Degraded,
    /// At least one required source was omitted.
    Blocked,
    /// The profile or snapshot failed validation.
    Invalid,
    /// Resolution health has not been determined.
    Unknown,
}
/// One source and its resolution policy within a context loadout.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    /// Stable identifier unique within the profile.
    pub id: String,
    /// Registry category used to resolve the source reference.
    pub kind: SourceKind,
    /// Registry-specific source identifier.
    pub source_ref: String,
    /// Policy used to select the source revision.
    pub revision_policy: RevisionPolicy,
    /// Effect of source availability on loadout health.
    pub requiredness: Requiredness,
    /// Relative ordering hint; higher values can be prioritized by resolvers.
    pub priority: i32,
    /// How and when the source content may be made available.
    pub usage_mode: UsageMode,
    /// Optional token estimate used when planning context capacity.
    pub token_budget_hint: Option<u32>,
}
/// Versioned collection of sources used to construct an agent context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    /// Serialized schema version; must equal [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable profile identifier.
    pub id: String,
    /// Positive monotonically increasing profile revision.
    pub revision: u64,
    /// Human-readable profile name.
    pub display_name: String,
    /// Sources and policies included in the profile.
    pub entries: Vec<Entry>,
    /// Lifecycle state controlling whether the profile can be selected.
    pub status: LoadoutStatus,
    /// Identity of the profile creator.
    pub created_by: String,
    /// Creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// SHA-256 hash of the serialized profile with this field cleared.
    pub content_hash: String,
}
/// Association between a profile and a project, conversation, or other target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Binding {
    /// Stable binding identifier.
    pub id: String,
    /// Identifier of the profile selected by this binding.
    pub profile_id: String,
    /// Target category interpreted by the binding resolver.
    pub target_kind: String,
    /// Target identifier within the target category.
    pub target_ref: String,
    /// Relative precedence when multiple bindings match a target.
    pub precedence: i32,
    /// Whether the resolver may apply this binding.
    pub enabled: bool,
    /// Optional expiration time as Unix epoch milliseconds.
    pub expires_at_ms: Option<i64>,
    /// Integrity hash of the serialized binding with this field cleared.
    pub content_hash: String,
}
/// Immutable record of the sources selected for one context construction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    /// Identifier of the profile used for resolution.
    pub profile_id: String,
    /// Profile revision used for resolution.
    pub profile_revision: u64,
    /// Integrity hash of the selected profile revision.
    pub profile_hash: String,
    /// Source references successfully resolved into the context.
    pub resolved_refs: Vec<String>,
    /// Required references that could not be resolved.
    pub omitted_required: Vec<String>,
    /// Entries rejected during resolution or validation.
    pub rejected_entries: Vec<String>,
    /// Resolver-reported health before aggregate evaluation.
    pub health: Health,
    /// Snapshot creation time as Unix epoch milliseconds.
    pub created_at_ms: i64,
    /// Integrity hash of the serialized snapshot with this field cleared.
    pub content_hash: String,
}
/// Validation failure encountered while processing a context loadout.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoadoutError {
    /// A profile field, reference, or integrity hash is invalid.
    #[error("invalid context loadout: {0}")]
    Invalid(String),
    /// The profile exceeds the supported entry limit.
    #[error("context loadout limit exceeded")]
    Limit,
}
fn hash<T: Serialize>(value: &T) -> Result<String, LoadoutError> {
    serde_json::to_vec(value)
        .map(|v| hex::encode(Sha256::digest(v)))
        .map_err(|e| LoadoutError::Invalid(e.to_string()))
}
/// Validates profile identity, entry references, bounds, and content hash.
pub fn validate_profile(profile: &Profile) -> Result<(), LoadoutError> {
    if profile.schema_version != SCHEMA_VERSION
        || profile.id.trim().is_empty()
        || profile.revision == 0
        || profile.display_name.len() > 8192
        || profile.created_by.trim().is_empty()
        || profile.created_at_ms <= 0
        || profile.entries.is_empty()
        || profile.entries.len() > MAX_ENTRIES
    {
        return Err(LoadoutError::Invalid("profile identity".into()));
    }
    if profile.entries.iter().any(|e| {
        e.id.trim().is_empty() || e.source_ref.trim().is_empty() || e.source_ref.len() > 8192
    }) {
        return Err(LoadoutError::Invalid("entry reference".into()));
    }
    let mut c = profile.clone();
    c.content_hash.clear();
    if profile.content_hash != hash(&c)? {
        return Err(LoadoutError::Invalid("profile content_hash".into()));
    }
    Ok(())
}
/// Computes effective snapshot health from required omissions and rejections.
pub fn evaluate(snapshot: &Snapshot) -> Health {
    if !snapshot.omitted_required.is_empty() {
        Health::Blocked
    } else if !snapshot.rejected_entries.is_empty() {
        Health::Degraded
    } else {
        snapshot.health
    }
}
