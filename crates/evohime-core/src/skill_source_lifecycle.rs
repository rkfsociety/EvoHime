use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
/// Current serialized schema version for skill source lifecycle records.
pub const SCHEMA_VERSION: u32 = 1;
const MAX_TEXT: usize = 8192;
/// Origin category of a skill package.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Shipped with the application.
    Bundled,
    /// Resolved from a versioned Git repository.
    GitRepository,
    /// Stored in a local directory outside the workspace.
    LocalDirectory,
    /// Stored within the active workspace.
    WorkspaceDirectory,
    /// Imported as a packaged artifact.
    ImportedPackage,
    /// Imported through a legacy compatibility route.
    CompatibilitySource,
}
/// Ownership and edit policy for an installed skill.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallationMode {
    /// Managed bundle that cannot be locally overwritten.
    BundledManaged,
    /// Managed installation that follows a subscription source.
    ManagedSubscription,
    /// Vendored copy that users may edit locally.
    VendoredEditable,
    /// Installation scoped to one workspace.
    WorkspaceLocal,
    /// Installation scoped to the current user.
    UserLocal,
    /// Imported installation retained under compatibility rules.
    CompatibilityImported,
}
/// Update workflow status for an installed skill.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    /// Installed revision matches the selected source.
    Current,
    /// Source revision lookup is in progress.
    Checking,
    /// A newer source revision is available.
    UpdateAvailable,
    /// Candidate revision awaits human inspection.
    StagedForReview,
    /// Candidate requires a new trust decision.
    TrustReviewRequired,
    /// Candidate passed checks and is ready for activation.
    ReadyToActivate,
    /// Local content differs from its recorded base.
    LocallyModified,
    /// Local and upstream revisions changed independently.
    Diverged,
    /// Changes require a merge decision.
    MergeRequired,
    /// Merge conflict prevents update activation.
    Conflict,
    /// Update is currently being applied.
    Updating,
    /// An update attempt failed.
    UpdateFailed,
    /// Installation is pinned to its current revision.
    Pinned,
    /// Source could not be resolved or read.
    SourceUnavailable,
}
/// Relationship between local content and its upstream base revision.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DivergenceState {
    /// Local installation matches the recorded base revision.
    CleanAtBase,
    /// Local content changed while upstream remained at its base.
    LocallyModified,
    /// Upstream changed while local content remained at its base.
    UpstreamChanged,
    /// Both local and upstream content changed.
    Diverged,
    /// Changes require explicit merge handling.
    MergeRequired,
    /// Conflicting edits were found.
    Conflict,
    /// Source revision has no attached base.
    Detached,
    /// Divergence cannot be determined.
    Unknown,
}
/// Integrity-bound source locator for a skill package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Source {
    /// Source schema version.
    pub schema_version: u32,
    /// Stable source identifier.
    pub id: String,
    /// Origin category.
    pub kind: SourceKind,
    /// Repository, path, or package locator.
    pub origin: String,
    /// Optional requested branch, tag, or version.
    pub requested_ref: Option<String>,
    /// Resolved immutable revision or artifact reference.
    pub resolver_ref: String,
    /// Digest of the source record with this field cleared.
    pub content_hash: String,
}
/// Installed skill revision, local divergence, update state, and trust reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Installed {
    /// Stable installation identifier.
    pub installation_id: String,
    /// Logical skill identifier shared across installations.
    pub logical_skill_id: String,
    /// Ownership and update mode.
    pub mode: InstallationMode,
    /// Source record identifier.
    pub source_ref: String,
    /// Immutable revision currently installed.
    pub installed_revision_ref: String,
    /// Digest of the package as installed.
    pub package_hash: String,
    /// Digest of current local content.
    pub local_content_hash: String,
    /// Optional revision from which an editable vendor copy was created.
    pub vendor_base_revision_ref: Option<String>,
    /// Most recent upstream revision observed.
    pub upstream_last_seen_revision_ref: Option<String>,
    /// Current update workflow state.
    pub update_state: UpdateState,
    /// Local-versus-upstream divergence classification.
    pub divergence_state: DivergenceState,
    /// Reference to the trust decision for the installed package hash.
    pub trust_record_ref: String,
    /// Digest of the installation record with this field cleared.
    pub content_hash: String,
}
/// Invalid lifecycle metadata or an update that would overwrite managed edits.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SkillLifecycleError {
    /// Source or installation data fails validation.
    #[error("invalid skill lifecycle record: {0}")]
    Invalid(String),
    /// Update would overwrite local edits in a user-owned installation mode.
    #[error("managed installation cannot be overwritten")]
    ManagedOverwriteDenied,
}
fn hash<T: Serialize>(v: &T) -> Result<String, SkillLifecycleError> {
    serde_json::to_vec(v)
        .map(|b| hex::encode(Sha256::digest(b)))
        .map_err(|e| SkillLifecycleError::Invalid(e.to_string()))
}
/// Validates source identity and verifies its canonical content hash.
pub fn validate_source(s: &Source) -> Result<(), SkillLifecycleError> {
    if s.schema_version != SCHEMA_VERSION
        || s.id.trim().is_empty()
        || s.origin.len() > MAX_TEXT
        || s.resolver_ref.trim().is_empty()
    {
        return Err(SkillLifecycleError::Invalid("source identity".into()));
    }
    let mut c = s.clone();
    c.content_hash.clear();
    if s.content_hash != hash(&c)? {
        return Err(SkillLifecycleError::Invalid("source hash".into()));
    }
    Ok(())
}
/// Validates installation identity, ownership mode, divergence, and content hash.
pub fn validate_installation(i: &Installed) -> Result<(), SkillLifecycleError> {
    if i.installation_id.trim().is_empty()
        || i.logical_skill_id.trim().is_empty()
        || i.source_ref.trim().is_empty()
        || i.installed_revision_ref.trim().is_empty()
        || i.package_hash.trim().is_empty()
        || i.local_content_hash.trim().is_empty()
    {
        return Err(SkillLifecycleError::Invalid("installation identity".into()));
    }
    if matches!(
        i.mode,
        InstallationMode::ManagedSubscription | InstallationMode::BundledManaged
    ) && matches!(
        i.divergence_state,
        DivergenceState::LocallyModified | DivergenceState::Diverged
    ) {
        return Err(SkillLifecycleError::Invalid("managed divergence".into()));
    }
    let mut c = i.clone();
    c.content_hash.clear();
    if i.content_hash != hash(&c)? {
        return Err(SkillLifecycleError::Invalid("installation hash".into()));
    }
    Ok(())
}
/// Denies updates that would overwrite edits in locally owned installation modes.
pub fn update_allowed(i: &Installed) -> Result<(), SkillLifecycleError> {
    if matches!(
        i.mode,
        InstallationMode::VendoredEditable
            | InstallationMode::WorkspaceLocal
            | InstallationMode::UserLocal
    ) && matches!(
        i.divergence_state,
        DivergenceState::LocallyModified | DivergenceState::Diverged | DivergenceState::Conflict
    ) {
        return Err(SkillLifecycleError::ManagedOverwriteDenied);
    }
    Ok(())
}
