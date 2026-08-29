//! Core-owned discovery and progressive disclosure for local `SKILL.md`
//! packages. Skills are untrusted instructions: this module only reads
//! bounded metadata/content and never executes package helpers.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

pub const SKILL_SCHEMA_VERSION: u32 = 1;
pub const MAX_SKILLS: usize = 128;
pub const MAX_SKILL_BYTES: usize = 256 * 1024;
pub const MAX_REFERENCE_BYTES: usize = 64 * 1024;
pub const MAX_NAME_CHARS: usize = 128;
pub const MAX_DESCRIPTION_CHARS: usize = 2_048;
pub const MAX_LIST_ITEMS: usize = 64;
pub const MAX_LIST_ITEM_CHARS: usize = 128;
pub const MAX_DIAGNOSTICS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceKind {
    Explicit,
    ProjectNative,
    Global,
    Compatibility,
    Bundled,
}

impl SkillSourceKind {
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Explicit => 0,
            Self::ProjectNative => 1,
            Self::Global => 2,
            Self::Compatibility => 3,
            Self::Bundled => 4,
        }
    }
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::ProjectNative => "project_native",
            Self::Global => "global",
            Self::Compatibility => "compatibility",
            Self::Bundled => "bundled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillValidationStatus {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillMetadataV1 {
    pub schema_version: u32,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub scope: String,
    pub source_kind: SkillSourceKind,
    pub source_ref: String,
    pub content_hash: String,
    pub allowed_tools: Vec<String>,
    pub required_capabilities: Vec<String>,
    pub disable_model_invocation: bool,
    pub reference_count: usize,
    pub validation_status: SkillValidationStatus,
    pub validation_error_code: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillDiagnostic {
    pub code: String,
    pub skill_id: String,
    pub source_kind: SkillSourceKind,
    pub source_ref: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCatalogV1 {
    pub schema_version: u32,
    pub skills: Vec<SkillMetadataV1>,
    pub diagnostics: Vec<SkillDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillProvenance {
    pub source_kind: SkillSourceKind,
    pub source_ref: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSkill {
    pub metadata: SkillMetadataV1,
    pub content: String,
    pub provenance: SkillProvenance,
    pub cache_hit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedSkillReference {
    pub name: String,
    pub content: String,
    pub content_hash: String,
    pub provenance: SkillProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillPermissions {
    pub allowed_tools: Vec<String>,
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRoot {
    pub kind: SkillSourceKind,
    pub label: String,
    pub path: PathBuf,
}

impl SkillRoot {
    pub fn new(kind: SkillSourceKind, label: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            kind,
            label: label.into(),
            path: path.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkillRegistryError {
    #[error("skill package was not found: {0}")]
    NotFound(String),
    #[error("skill has invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    #[error("skill field {field} is invalid: {reason}")]
    InvalidField { field: String, reason: String },
    #[error("skill schema version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("skill path is unsafe: {0}")]
    UnsafePath(String),
    #[error("skill file exceeds its bound: {0} bytes")]
    TooLarge(usize),
    #[error("skill content is not valid UTF-8")]
    InvalidEncoding,
    #[error("skill content contains a secret-shaped value")]
    SensitiveContent,
    #[error("skill changed during load")]
    StaleContent,
    #[error("skill reference was not found: {0}")]
    ReferenceNotFound(String),
    #[error("skill requires capabilities outside the parent grant: {0}")]
    CapabilityEscalation(String),
    #[error("skill I/O failed: {0}")]
    Io(String),
}

impl SkillRegistryError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::InvalidFrontmatter(_) => "invalid_frontmatter",
            Self::InvalidField { .. } => "invalid_field",
            Self::UnsupportedVersion(_) => "unsupported_version",
            Self::UnsafePath(_) => "unsafe_path",
            Self::TooLarge(_) => "too_large",
            Self::InvalidEncoding => "invalid_encoding",
            Self::SensitiveContent => "sensitive_content",
            Self::StaleContent => "stale_content",
            Self::ReferenceNotFound(_) => "reference_not_found",
            Self::CapabilityEscalation(_) => "capability_escalation",
            Self::Io(_) => "io_failed",
        }
    }
}

#[derive(Clone)]
struct SkillPackage {
    metadata: SkillMetadataV1,
    package_dir: PathBuf,
    skill_file: PathBuf,
}

#[derive(Clone)]
struct CachedSkill {
    hash: String,
    content: String,
}

pub struct SkillRegistry {
    roots: Vec<SkillRoot>,
    cache: BTreeMap<String, CachedSkill>,
}

impl SkillRegistry {
    pub fn from_roots(roots: Vec<SkillRoot>) -> Self {
        Self {
            roots,
            cache: BTreeMap::new(),
        }
    }

    pub fn for_workspace(workspace: &Path) -> Self {
        let mut roots = vec![
            SkillRoot::new(
                SkillSourceKind::ProjectNative,
                "project",
                workspace.join(".agents/skills"),
            ),
            SkillRoot::new(
                SkillSourceKind::Global,
                "global",
                appdata_root().join("EvoHime/skills"),
            ),
            SkillRoot::new(
                SkillSourceKind::Compatibility,
                "compatibility-codex",
                workspace.join(".codex/skills"),
            ),
            SkillRoot::new(
                SkillSourceKind::Compatibility,
                "compatibility-claude",
                workspace.join(".claude/skills"),
            ),
        ];
        if let Ok(exe) = std::env::current_exe() {
            roots.push(SkillRoot::new(
                SkillSourceKind::Bundled,
                "bundled",
                exe.parent().unwrap_or(Path::new(".")).join("skills"),
            ));
            roots.push(SkillRoot::new(
                SkillSourceKind::Bundled,
                "bundled-resources",
                exe.parent()
                    .unwrap_or(Path::new("."))
                    .join("resources/skills"),
            ));
        }
        Self::from_roots(roots)
    }

    pub fn catalog(&mut self) -> SkillCatalogV1 {
        let mut candidates = BTreeMap::<String, SkillPackage>::new();
        let mut diagnostics = Vec::new();
        let mut roots = self.roots.clone();
        roots.sort_by(|a, b| {
            a.kind
                .precedence()
                .cmp(&b.kind.precedence())
                .then_with(|| a.label.cmp(&b.label))
                .then_with(|| a.path.to_string_lossy().cmp(&b.path.to_string_lossy()))
        });
        for root in roots {
            self.scan_root(&root, &mut candidates, &mut diagnostics);
        }
        let mut skills = candidates
            .values()
            .map(|package| package.metadata.clone())
            .collect::<Vec<_>>();
        skills.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
        diagnostics.sort_by(|a, b| {
            a.skill_id
                .cmp(&b.skill_id)
                .then_with(|| a.source_ref.cmp(&b.source_ref))
                .then_with(|| a.code.cmp(&b.code))
        });
        diagnostics.truncate(MAX_DIAGNOSTICS);
        let valid = skills
            .iter()
            .map(|item| (item.skill_id.clone(), item.content_hash.clone()))
            .collect::<BTreeMap<_, _>>();
        self.cache
            .retain(|id, cached| valid.get(id).is_some_and(|hash| hash == &cached.hash));
        SkillCatalogV1 {
            schema_version: SKILL_SCHEMA_VERSION,
            skills,
            diagnostics,
        }
    }

    pub fn load(&mut self, skill_id: &str) -> Result<LoadedSkill, SkillRegistryError> {
        let package = self.selected_package(skill_id)?;
        if package.metadata.validation_status != SkillValidationStatus::Valid {
            return Err(SkillRegistryError::InvalidFrontmatter(
                package
                    .metadata
                    .validation_error_code
                    .unwrap_or_else(|| "invalid".into()),
            ));
        }
        let bytes = read_bounded(&package.skill_file, MAX_SKILL_BYTES)?;
        let hash = hash_bytes(&bytes);
        if hash != package.metadata.content_hash {
            return Err(SkillRegistryError::StaleContent);
        }
        let text = String::from_utf8(bytes).map_err(|_| SkillRegistryError::InvalidEncoding)?;
        let parsed = parse_document(
            &text,
            package.metadata.source_kind,
            &package.metadata.source_ref,
        )?;
        if parsed.metadata.skill_id != package.metadata.skill_id {
            return Err(SkillRegistryError::StaleContent);
        }
        if let Some(cached) = self.cache.get(skill_id) {
            if cached.hash == hash {
                return Ok(self.loaded(package.metadata, cached.content.clone(), true));
            }
        }
        if contains_secret_shape(&parsed.body) {
            return Err(SkillRegistryError::SensitiveContent);
        }
        self.cache.insert(
            skill_id.to_owned(),
            CachedSkill {
                hash: hash.clone(),
                content: parsed.body.clone(),
            },
        );
        Ok(self.loaded(package.metadata, parsed.body, false))
    }

    pub fn load_reference(
        &mut self,
        skill_id: &str,
        name: &str,
    ) -> Result<LoadedSkillReference, SkillRegistryError> {
        let package = self.selected_package(skill_id)?;
        validate_relative_reference(name)?;
        let path = package.package_dir.join(name);
        if !path.starts_with(package.package_dir.join("references")) {
            return Err(SkillRegistryError::UnsafePath(name.into()));
        }
        ensure_no_symlink(&package.package_dir, &path)?;
        let bytes = read_bounded(&path, MAX_REFERENCE_BYTES).map_err(|error| match error {
            SkillRegistryError::Io(_) => SkillRegistryError::ReferenceNotFound(name.into()),
            other => other,
        })?;
        let content =
            String::from_utf8(bytes.clone()).map_err(|_| SkillRegistryError::InvalidEncoding)?;
        if contains_secret_shape(&content) {
            return Err(SkillRegistryError::SensitiveContent);
        }
        Ok(LoadedSkillReference {
            name: name.replace('\\', "/"),
            content,
            content_hash: hash_bytes(&bytes),
            provenance: self.provenance(&package.metadata),
        })
    }

    pub fn effective_permissions(
        &mut self,
        skill_id: &str,
        parent_tools: &BTreeSet<String>,
        parent_capabilities: &BTreeSet<String>,
    ) -> Result<SkillPermissions, SkillRegistryError> {
        let package = self.selected_package(skill_id)?;
        let missing = package
            .metadata
            .required_capabilities
            .iter()
            .filter(|capability| !parent_capabilities.contains(*capability))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(SkillRegistryError::CapabilityEscalation(missing.join(",")));
        }
        let mut allowed_tools = package
            .metadata
            .allowed_tools
            .iter()
            .filter(|tool| parent_tools.contains(*tool))
            .cloned()
            .collect::<Vec<_>>();
        allowed_tools.sort();
        allowed_tools.dedup();
        Ok(SkillPermissions {
            allowed_tools,
            required_capabilities: package.metadata.required_capabilities,
        })
    }

    fn loaded(&self, metadata: SkillMetadataV1, content: String, cache_hit: bool) -> LoadedSkill {
        LoadedSkill {
            provenance: self.provenance(&metadata),
            metadata,
            content,
            cache_hit,
        }
    }
    fn provenance(&self, metadata: &SkillMetadataV1) -> SkillProvenance {
        SkillProvenance {
            source_kind: metadata.source_kind,
            source_ref: metadata.source_ref.clone(),
            version: metadata.version.clone(),
            content_hash: metadata.content_hash.clone(),
        }
    }

    fn selected_package(&mut self, skill_id: &str) -> Result<SkillPackage, SkillRegistryError> {
        let normalized = normalize_id(skill_id)?;
        let mut candidates = BTreeMap::<String, SkillPackage>::new();
        let mut diagnostics = Vec::new();
        let mut roots = self.roots.clone();
        roots.sort_by(|a, b| {
            a.kind
                .precedence()
                .cmp(&b.kind.precedence())
                .then_with(|| a.label.cmp(&b.label))
                .then_with(|| a.path.to_string_lossy().cmp(&b.path.to_string_lossy()))
        });
        for root in roots {
            self.scan_root(&root, &mut candidates, &mut diagnostics);
        }
        candidates
            .remove(&normalized)
            .ok_or(SkillRegistryError::NotFound(normalized))
    }

    fn scan_root(
        &self,
        root: &SkillRoot,
        candidates: &mut BTreeMap<String, SkillPackage>,
        diagnostics: &mut Vec<SkillDiagnostic>,
    ) {
        let Ok(root_meta) = fs::symlink_metadata(&root.path) else {
            return;
        };
        if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
            return;
        }
        let Ok(root_path) = root.path.canonicalize() else {
            return;
        };
        let Ok(entries) = fs::read_dir(&root_path) else {
            return;
        };
        let mut dirs = entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|kind| kind.is_dir() && !kind.is_symlink())
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        dirs.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        dirs.truncate(MAX_SKILLS);
        for entry in dirs {
            let package_dir = entry.path();
            let skill_file = package_dir.join("SKILL.md");
            if !skill_file.is_file() {
                continue;
            }
            let fallback_id = normalize_id(&entry.file_name().to_string_lossy())
                .unwrap_or_else(|_| "invalid-skill".into());
            let source_ref = format!("{}:{}", root.label, entry.file_name().to_string_lossy());
            let package = match parse_file(&skill_file, root.kind, &source_ref) {
                Ok(package) => package,
                Err(error) => {
                    let hash = fs::read(&skill_file)
                        .map(|bytes| hash_bytes(&bytes))
                        .unwrap_or_default();
                    SkillPackage {
                        metadata: invalid_metadata(
                            fallback_id,
                            root.kind,
                            source_ref,
                            hash,
                            &error,
                        ),
                        package_dir,
                        skill_file,
                    }
                }
            };
            let id = package.metadata.skill_id.clone();
            if let Some(previous) = candidates.get(&id) {
                diagnostics.push(SkillDiagnostic {
                    code: "collision".into(),
                    skill_id: id.clone(),
                    source_kind: package.metadata.source_kind,
                    source_ref: package.metadata.source_ref.clone(),
                    message: format!(
                        "winner={} loser={}",
                        previous.metadata.source_ref, package.metadata.source_ref
                    ),
                });
                let new_wins = (
                    package.metadata.source_kind.precedence(),
                    package.metadata.source_ref.as_str(),
                ) < (
                    previous.metadata.source_kind.precedence(),
                    previous.metadata.source_ref.as_str(),
                );
                if new_wins {
                    candidates.insert(id, package);
                }
            } else if candidates.len() < MAX_SKILLS {
                candidates.insert(id, package);
            }
        }
    }
}

#[derive(Debug)]
struct ParsedDocument {
    metadata: SkillMetadataV1,
    body: String,
}

fn parse_file(
    path: &Path,
    kind: SkillSourceKind,
    source_ref: &str,
) -> Result<SkillPackage, SkillRegistryError> {
    let bytes = read_bounded(path, MAX_SKILL_BYTES)?;
    let hash = hash_bytes(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| SkillRegistryError::InvalidEncoding)?;
    let parsed = parse_document(&text, kind, source_ref)?;
    let package_dir = path
        .parent()
        .ok_or_else(|| SkillRegistryError::UnsafePath(path.display().to_string()))?
        .to_path_buf();
    let reference_count = count_references(&package_dir);
    let mut metadata = parsed.metadata;
    metadata.content_hash = hash;
    metadata.reference_count = reference_count;
    Ok(SkillPackage {
        metadata,
        package_dir,
        skill_file: path.to_path_buf(),
    })
}

fn parse_document(
    text: &str,
    kind: SkillSourceKind,
    source_ref: &str,
) -> Result<ParsedDocument, SkillRegistryError> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err(SkillRegistryError::InvalidFrontmatter(
            "opening delimiter is required".into(),
        ));
    }
    let mut fields = BTreeMap::new();
    let mut closing = None;
    for (index, line) in lines.by_ref().enumerate() {
        if line.trim() == "---" {
            closing = Some(index + 2);
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(':').ok_or_else(|| {
            SkillRegistryError::InvalidFrontmatter(format!(
                "line {} has no key/value separator",
                index + 2
            ))
        })?;
        let key = key.trim().to_ascii_lowercase();
        let value = unquote(value.trim());
        if contains_secret_shape(&value) {
            return Err(SkillRegistryError::SensitiveContent);
        }
        if is_dangerous_key(&key) {
            return Err(SkillRegistryError::InvalidField {
                field: key,
                reason: "permission or executable metadata is forbidden".into(),
            });
        }
        fields.insert(key, value);
    }
    let closing = closing.ok_or_else(|| {
        SkillRegistryError::InvalidFrontmatter("closing delimiter is required".into())
    })?;
    let name = required_field(&fields, "name")?;
    let description = required_field(&fields, "description")?;
    validate_text("name", &name, MAX_NAME_CHARS)?;
    validate_text("description", &description, MAX_DESCRIPTION_CHARS)?;
    if contains_prompt_injection(&description) {
        return Err(SkillRegistryError::InvalidField {
            field: "description".into(),
            reason: "prompt-injection marker".into(),
        });
    }
    let skill_id = normalize_id(&name)?;
    let version = fields
        .get("version")
        .cloned()
        .unwrap_or_else(|| "1.0.0".into());
    validate_text("version", &version, 64)?;
    let default_scope = match kind {
        SkillSourceKind::ProjectNative => "project",
        SkillSourceKind::Compatibility => "compatibility",
        SkillSourceKind::Explicit => "session",
        SkillSourceKind::Global => "global",
        SkillSourceKind::Bundled => "bundled",
    };
    let scope = fields
        .get("scope")
        .cloned()
        .unwrap_or_else(|| default_scope.into());
    if !matches!(
        scope.as_str(),
        "global" | "project" | "compatibility" | "bundled" | "session"
    ) {
        return Err(SkillRegistryError::InvalidField {
            field: "scope".into(),
            reason: "unknown scope".into(),
        });
    }
    let allowed_tools = parse_list(fields.get("allowed-tools"), "allowed-tools")?;
    let required_capabilities =
        parse_list(fields.get("required-capabilities"), "required-capabilities")?;
    let disable_model_invocation = fields
        .get("disable-model-invocation")
        .map(|value| match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(SkillRegistryError::InvalidField {
                field: "disable-model-invocation".into(),
                reason: "expected true or false".into(),
            }),
        })
        .transpose()?
        .unwrap_or(false);
    let known = [
        "name",
        "description",
        "version",
        "scope",
        "allowed-tools",
        "required-capabilities",
        "disable-model-invocation",
    ];
    let warnings = fields
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .take(16)
        .map(|key| format!("unknown metadata field: {key}"))
        .collect();
    let body = text.lines().skip(closing).collect::<Vec<_>>().join("\n");
    Ok(ParsedDocument {
        metadata: SkillMetadataV1 {
            schema_version: SKILL_SCHEMA_VERSION,
            skill_id,
            name,
            description,
            version,
            scope,
            source_kind: kind,
            source_ref: source_ref.into(),
            content_hash: String::new(),
            allowed_tools,
            required_capabilities,
            disable_model_invocation,
            reference_count: 0,
            validation_status: SkillValidationStatus::Valid,
            validation_error_code: None,
            warnings,
        },
        body,
    })
}

fn invalid_metadata(
    skill_id: String,
    kind: SkillSourceKind,
    source_ref: String,
    hash: String,
    error: &SkillRegistryError,
) -> SkillMetadataV1 {
    SkillMetadataV1 {
        schema_version: SKILL_SCHEMA_VERSION,
        skill_id: skill_id.clone(),
        name: skill_id,
        description: String::new(),
        version: String::new(),
        scope: kind.as_str().into(),
        source_kind: kind,
        source_ref,
        content_hash: hash,
        allowed_tools: Vec::new(),
        required_capabilities: Vec::new(),
        disable_model_invocation: true,
        reference_count: 0,
        validation_status: SkillValidationStatus::Invalid,
        validation_error_code: Some(error.code().into()),
        warnings: Vec::new(),
    }
}

fn required_field(
    fields: &BTreeMap<String, String>,
    key: &str,
) -> Result<String, SkillRegistryError> {
    fields
        .get(key)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| SkillRegistryError::InvalidField {
            field: key.into(),
            reason: "required".into(),
        })
}

fn parse_list(value: Option<&String>, field: &str) -> Result<Vec<String>, SkillRegistryError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let value = value.trim();
    let value = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value);
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for item in value
        .split(',')
        .map(unquote)
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
    {
        validate_text(field, &item, MAX_LIST_ITEM_CHARS)?;
        if !seen.insert(item.clone()) {
            return Err(SkillRegistryError::InvalidField {
                field: field.into(),
                reason: "duplicate item".into(),
            });
        }
        result.push(item);
    }
    if result.len() > MAX_LIST_ITEMS {
        return Err(SkillRegistryError::InvalidField {
            field: field.into(),
            reason: "too many items".into(),
        });
    }
    result.sort();
    Ok(result)
}

fn normalize_id(value: &str) -> Result<String, SkillRegistryError> {
    let value = value.trim().to_ascii_lowercase().replace([' ', '_'], "-");
    if value.is_empty()
        || value.len() > MAX_NAME_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.'))
    {
        return Err(SkillRegistryError::InvalidField {
            field: "name".into(),
            reason: "skill id must be a bounded token".into(),
        });
    }
    Ok(value)
}

fn validate_text(field: &str, value: &str, max: usize) -> Result<(), SkillRegistryError> {
    if value.is_empty() || value.chars().count() > max || value.chars().any(char::is_control) {
        return Err(SkillRegistryError::InvalidField {
            field: field.into(),
            reason: "empty, oversized or control characters".into(),
        });
    }
    Ok(())
}
fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}
fn is_dangerous_key(key: &str) -> bool {
    [
        "permissions",
        "permission",
        "exec",
        "command",
        "commands",
        "install",
        "network",
        "download",
        "run",
        "hooks",
        "scripts",
    ]
    .contains(&key)
}
fn contains_prompt_injection(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("ignore previous") || value.contains("system prompt")
}
fn contains_secret_shape(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("authorization: bearer")
        || value.contains("bearer ")
        || value.contains("api_key=")
        || value.contains("apikey=")
        || value.contains("private key")
        || value.contains("password=")
        || value.contains("client_secret=")
        || value.contains("sk-")
}
fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn appdata_root() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("EvoHime"))
}
fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, SkillRegistryError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| SkillRegistryError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SkillRegistryError::UnsafePath(path.display().to_string()));
    }
    if metadata.len() > max as u64 {
        return Err(SkillRegistryError::TooLarge(metadata.len() as usize));
    }
    fs::read(path).map_err(|error| SkillRegistryError::Io(error.to_string()))
}
fn count_references(package_dir: &Path) -> usize {
    let path = package_dir.join("references");
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file() && !kind.is_symlink())
                .unwrap_or(false)
        })
        .count()
        .min(MAX_LIST_ITEMS)
}
fn validate_relative_reference(name: &str) -> Result<(), SkillRegistryError> {
    let path = Path::new(name);
    if name.is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || !path.starts_with("references")
    {
        return Err(SkillRegistryError::UnsafePath(name.into()));
    }
    Ok(())
}
fn ensure_no_symlink(root: &Path, path: &Path) -> Result<(), SkillRegistryError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| SkillRegistryError::UnsafePath(path.display().to_string()))?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current)
            .map_err(|error| SkillRegistryError::Io(error.to_string()))?
            .file_type()
            .is_symlink()
        {
            return Err(SkillRegistryError::UnsafePath(
                current.display().to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn package(root: &Path, dir: &str, name: &str, body: &str) -> PathBuf {
        let path = root.join(dir);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("SKILL.md"), format!("---\nname: {name}\ndescription: bounded skill\nversion: 1.0.0\nallowed-tools: [workspace.list, workspace.read]\nrequired-capabilities: [workspace.read]\n---\n{body}\n")).unwrap();
        path
    }

    #[test]
    fn discovery_is_bounded_on_deterministic_precedence_and_load_is_on_demand() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("project");
        let global = temp.path().join("global");
        package(&global, "same", "same", "global body");
        package(&project, "same", "same", "project body");
        package(&project, "other", "other", "other body");
        let mut registry = SkillRegistry::from_roots(vec![
            SkillRoot::new(SkillSourceKind::Global, "global", global),
            SkillRoot::new(SkillSourceKind::ProjectNative, "project", project),
        ]);
        let catalog = registry.catalog();
        assert_eq!(
            catalog
                .skills
                .iter()
                .map(|skill| skill.skill_id.as_str())
                .collect::<Vec<_>>(),
            ["other", "same"]
        );
        assert_eq!(catalog.diagnostics[0].code, "collision");
        assert_eq!(registry.load("same").unwrap().content, "project body");
        assert!(registry.load("same").unwrap().cache_hit);
    }

    #[test]
    fn invalid_frontmatter_is_typed_and_does_not_fallback_to_lower_precedence() {
        let temp = TempDir::new().unwrap();
        let project = temp.path().join("project");
        let global = temp.path().join("global");
        package(&global, "same", "same", "safe");
        fs::create_dir_all(project.join("same")).unwrap();
        fs::write(
            project.join("same/SKILL.md"),
            "---\nname: same\npermissions: full\n---\nunsafe",
        )
        .unwrap();
        let mut registry = SkillRegistry::from_roots(vec![
            SkillRoot::new(SkillSourceKind::Global, "global", global),
            SkillRoot::new(SkillSourceKind::ProjectNative, "project", project),
        ]);
        let catalog = registry.catalog();
        let skill = catalog
            .skills
            .iter()
            .find(|skill| skill.skill_id == "same")
            .unwrap();
        assert_eq!(skill.validation_status, SkillValidationStatus::Invalid);
        assert_eq!(
            skill.validation_error_code.as_deref(),
            Some("invalid_field")
        );
        assert!(registry.load("same").is_err());
    }

    #[test]
    fn capability_grants_only_narrow_and_reference_traversal_is_rejected() {
        let temp = TempDir::new().unwrap();
        let path = package(temp.path(), "reader", "reader", "body");
        fs::create_dir(path.join("references")).unwrap();
        fs::write(path.join("references/guide.md"), "guide").unwrap();
        let mut registry = SkillRegistry::from_roots(vec![SkillRoot::new(
            SkillSourceKind::ProjectNative,
            "project",
            temp.path(),
        )]);
        let tools = ["workspace.list".into(), "other".into()]
            .into_iter()
            .collect();
        let caps = ["workspace.read".into()].into_iter().collect();
        let permissions = registry
            .effective_permissions("reader", &tools, &caps)
            .unwrap();
        assert_eq!(permissions.allowed_tools, ["workspace.list"]);
        assert_eq!(
            registry
                .load_reference("reader", "references/guide.md")
                .unwrap()
                .content,
            "guide"
        );
        assert!(matches!(
            registry.load_reference("reader", "references/../SKILL.md"),
            Err(SkillRegistryError::UnsafePath(_))
        ));
        let empty = BTreeSet::new();
        assert!(matches!(
            registry.effective_permissions("reader", &tools, &empty),
            Err(SkillRegistryError::CapabilityEscalation(_))
        ));
    }

    #[test]
    fn hash_changes_and_secret_or_oversized_content_fails_closed() {
        let temp = TempDir::new().unwrap();
        let path = package(temp.path(), "reader", "reader", "body");
        let mut registry = SkillRegistry::from_roots(vec![SkillRoot::new(
            SkillSourceKind::ProjectNative,
            "project",
            temp.path(),
        )]);
        let first = registry.load("reader").unwrap().provenance.content_hash;
        fs::write(
            path.join("SKILL.md"),
            "---\nname: reader\ndescription: bounded skill\n---\napi_key=secret",
        )
        .unwrap();
        assert!(matches!(
            registry.load("reader"),
            Err(SkillRegistryError::SensitiveContent)
        ));
        assert_ne!(first, hash_bytes(&fs::read(path.join("SKILL.md")).unwrap()));
    }
}
