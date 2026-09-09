use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 256;
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LoadoutStatus {
    Draft,
    Active,
    Superseded,
    NeedsReview,
    Invalid,
    Disabled,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    MemoryView,
    KnowledgeCollection,
    KnowledgeSource,
    Skill,
    ProjectGuidance,
    RepositoryProjection,
    ArtifactCollection,
    ExperienceView,
    ResearchCollection,
    CustomRegistered,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RevisionPolicy {
    ExactPinned,
    LatestCompatibleAtRunStart,
    LatestActiveAtRunStart,
    FollowProjectBindingAtConversationStart,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Requiredness {
    Required,
    Preferred,
    Optional,
    Advisory,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageMode {
    BootstrapOverview,
    AlwaysAvailableCatalog,
    AutoRetrieve,
    ExplicitOnly,
    InstructionActive,
    EvidenceOnly,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    Ready,
    ReadyWithWarnings,
    Degraded,
    Blocked,
    Invalid,
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub kind: SourceKind,
    pub source_ref: String,
    pub revision_policy: RevisionPolicy,
    pub requiredness: Requiredness,
    pub priority: i32,
    pub usage_mode: UsageMode,
    pub token_budget_hint: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub schema_version: u32,
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub entries: Vec<Entry>,
    pub status: LoadoutStatus,
    pub created_by: String,
    pub created_at_ms: i64,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Binding {
    pub id: String,
    pub profile_id: String,
    pub target_kind: String,
    pub target_ref: String,
    pub precedence: i32,
    pub enabled: bool,
    pub expires_at_ms: Option<i64>,
    pub content_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Snapshot {
    pub profile_id: String,
    pub profile_revision: u64,
    pub profile_hash: String,
    pub resolved_refs: Vec<String>,
    pub omitted_required: Vec<String>,
    pub rejected_entries: Vec<String>,
    pub health: Health,
    pub created_at_ms: i64,
    pub content_hash: String,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LoadoutError {
    #[error("invalid context loadout: {0}")]
    Invalid(String),
    #[error("context loadout limit exceeded")]
    Limit,
}
fn hash<T: Serialize>(value: &T) -> Result<String, LoadoutError> {
    serde_json::to_vec(value)
        .map(|v| hex::encode(Sha256::digest(v)))
        .map_err(|e| LoadoutError::Invalid(e.to_string()))
}
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
pub fn evaluate(snapshot: &Snapshot) -> Health {
    if !snapshot.omitted_required.is_empty() {
        Health::Blocked
    } else if !snapshot.rejected_entries.is_empty() {
        Health::Degraded
    } else {
        snapshot.health
    }
}
