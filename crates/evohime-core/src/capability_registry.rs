//! Bounded, offline capability-registry contract for roles and skills.
//!
//! This module intentionally has no installation, filesystem, or network side
//! effects. It validates immutable metadata, derives an effective permission
//! intersection, and performs deterministic intent matching.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const MAX_MANIFESTS: usize = 128;
pub const MAX_NAME_CHARS: usize = 128;
pub const MAX_VERSION_CHARS: usize = 64;
pub const MAX_HASH_CHARS: usize = 128;
pub const MAX_SIGNATURE_CHARS: usize = 512;
pub const MAX_ITEMS: usize = 64;
pub const MAX_ITEM_CHARS: usize = 128;
pub const MAX_PATH_CHARS: usize = 512;
pub const MAX_INTENT_CHARS: usize = 2_048;
pub const MAX_MATCHES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleRef {
    pub name: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRef {
    pub name: String,
    pub version: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub name: String,
    pub version: String,
    pub content_hash: String,
    pub signature: String,
    pub roles: Vec<RoleRef>,
    pub skills: Vec<SkillRef>,
    pub allowed_tools: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub protected_paths: Vec<String>,
    pub risk_class: RiskClass,
    pub install: InstallPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPolicy {
    pub source: InstallSource,
    pub allow_install_scripts: bool,
    pub allow_update: bool,
    pub rollback_on_failure: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallSource {
    LocalArchive,
    HttpsArchive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectivePermissions {
    pub allowed_tools: Vec<String>,
    pub allowed_domains: Vec<String>,
    pub protected_paths: Vec<String>,
    pub risk_class: RiskClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchQuery {
    pub intent: String,
    pub required_tools: Vec<String>,
    pub required_domains: Vec<String>,
    pub requested_risk: RiskClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchResult {
    pub manifest_name: String,
    pub version: String,
    pub score: u16,
    pub permissions: EffectivePermissions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    EmptyField(&'static str),
    FieldTooLong { field: &'static str, max: usize },
    TooManyItems { field: &'static str, max: usize },
    DuplicateItem(&'static str),
    InvalidHash,
    InvalidSignature,
    InvalidPath,
    InvalidDomain,
    PromptInjection,
    RiskEscalation,
    TooManyManifests,
    InvalidUpdate,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::TooManyItems { field, max } => write!(f, "{field} exceeds {max} items"),
            Self::DuplicateItem(field) => write!(f, "{field} contains a duplicate"),
            Self::InvalidHash => write!(f, "content_hash must be a bounded hexadecimal digest"),
            Self::InvalidSignature => write!(f, "signature metadata is invalid"),
            Self::InvalidPath => write!(f, "path escapes the protected workspace"),
            Self::InvalidDomain => write!(f, "domain is not a normalized host name"),
            Self::PromptInjection => write!(f, "prompt-injection text is not allowed in metadata"),
            Self::RiskEscalation => write!(f, "requested risk exceeds manifest risk"),
            Self::TooManyManifests => write!(f, "registry exceeds its manifest bound"),
            Self::InvalidUpdate => write!(f, "update policy requires a compatible rollback"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl CapabilityManifest {
    pub fn validate(&self) -> Result<(), RegistryError> {
        validate_text("name", &self.name, MAX_NAME_CHARS, true)?;
        validate_text("version", &self.version, MAX_VERSION_CHARS, true)?;
        validate_hash(&self.content_hash)?;
        validate_signature(&self.signature)?;
        validate_unique_names(
            "roles",
            &self.roles.iter().map(|v| &v.name).collect::<Vec<_>>(),
        )?;
        validate_unique_names(
            "skills",
            &self.skills.iter().map(|v| &v.name).collect::<Vec<_>>(),
        )?;
        for role in &self.roles {
            validate_ref(&role.name, &role.version, &role.content_hash)?;
        }
        for skill in &self.skills {
            validate_ref(&skill.name, &skill.version, &skill.content_hash)?;
        }
        validate_items("allowed_tools", &self.allowed_tools)?;
        validate_items("allowed_domains", &self.allowed_domains)?;
        for domain in &self.allowed_domains {
            validate_domain(domain)?;
        }
        validate_paths(&self.protected_paths)?;
        if self.install.allow_install_scripts {
            return Err(RegistryError::InvalidUpdate);
        }
        if self.install.allow_update && !self.install.rollback_on_failure {
            return Err(RegistryError::InvalidUpdate);
        }
        Ok(())
    }

    pub fn effective_permissions(
        &self,
        requested: &EffectivePermissions,
    ) -> Result<EffectivePermissions, RegistryError> {
        self.validate()?;
        if requested.risk_class > self.risk_class {
            return Err(RegistryError::RiskEscalation);
        }
        Ok(EffectivePermissions {
            allowed_tools: intersection(&self.allowed_tools, &requested.allowed_tools),
            allowed_domains: intersection(&self.allowed_domains, &requested.allowed_domains),
            protected_paths: union_sorted(&self.protected_paths, &requested.protected_paths),
            risk_class: requested.risk_class,
        })
    }
}

pub fn validate_registry(manifests: &[CapabilityManifest]) -> Result<(), RegistryError> {
    if manifests.len() > MAX_MANIFESTS {
        return Err(RegistryError::TooManyManifests);
    }
    let names = manifests.iter().map(|v| &v.name).collect::<Vec<_>>();
    validate_unique_names("manifests", &names)
}

pub fn match_capabilities(
    manifests: &[CapabilityManifest],
    query: &MatchQuery,
) -> Result<Vec<MatchResult>, RegistryError> {
    validate_registry(manifests)?;
    validate_text("intent", &query.intent, MAX_INTENT_CHARS, true)?;
    reject_prompt_injection(&query.intent)?;
    validate_items("required_tools", &query.required_tools)?;
    validate_items("required_domains", &query.required_domains)?;

    let mut matches = Vec::new();
    for manifest in manifests {
        manifest.validate()?;
        if query.requested_risk > manifest.risk_class
            || !contains_all(&manifest.allowed_tools, &query.required_tools)
            || !contains_all(&manifest.allowed_domains, &query.required_domains)
        {
            continue;
        }
        let intent = query.intent.to_ascii_lowercase();
        let name = manifest.name.to_ascii_lowercase();
        let score = (u16::from(intent.contains(&name)) * 4)
            + (query.required_tools.len() as u16 * 2)
            + query.required_domains.len() as u16;
        let requested = EffectivePermissions {
            allowed_tools: query.required_tools.clone(),
            allowed_domains: query.required_domains.clone(),
            protected_paths: Vec::new(),
            risk_class: query.requested_risk,
        };
        matches.push(MatchResult {
            manifest_name: manifest.name.clone(),
            version: manifest.version.clone(),
            score,
            permissions: manifest.effective_permissions(&requested)?,
        });
    }
    matches.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.manifest_name.cmp(&b.manifest_name))
    });
    matches.truncate(MAX_MATCHES);
    Ok(matches)
}

pub fn validate_update(
    current: &CapabilityManifest,
    candidate: &CapabilityManifest,
) -> Result<(), RegistryError> {
    current.validate()?;
    candidate.validate()?;
    if current.name != candidate.name || current.install.source != candidate.install.source {
        return Err(RegistryError::InvalidUpdate);
    }
    if candidate.risk_class > current.risk_class && !candidate.install.rollback_on_failure {
        return Err(RegistryError::InvalidUpdate);
    }
    Ok(())
}

fn validate_ref(name: &str, version: &str, hash: &str) -> Result<(), RegistryError> {
    validate_text("reference.name", name, MAX_NAME_CHARS, true)?;
    validate_text("reference.version", version, MAX_VERSION_CHARS, true)?;
    validate_hash(hash)
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), RegistryError> {
    if required && value.trim().is_empty() {
        return Err(RegistryError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(RegistryError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(RegistryError::PromptInjection);
    }
    reject_prompt_injection(value)
}

fn validate_hash(value: &str) -> Result<(), RegistryError> {
    if value.len() < 32
        || value.len() > MAX_HASH_CHARS
        || !value.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(RegistryError::InvalidHash);
    }
    Ok(())
}

fn validate_signature(value: &str) -> Result<(), RegistryError> {
    validate_text("signature", value, MAX_SIGNATURE_CHARS, true)?;
    if value.to_ascii_lowercase().contains("private")
        || value.to_ascii_lowercase().contains("secret")
    {
        return Err(RegistryError::InvalidSignature);
    }
    Ok(())
}

fn validate_items(field: &'static str, values: &[String]) -> Result<(), RegistryError> {
    if values.len() > MAX_ITEMS {
        return Err(RegistryError::TooManyItems {
            field,
            max: MAX_ITEMS,
        });
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_text(field, value, MAX_ITEM_CHARS, true)?;
        if !seen.insert(value.to_ascii_lowercase()) {
            return Err(RegistryError::DuplicateItem(field));
        }
    }
    Ok(())
}

fn validate_unique_names(field: &'static str, values: &[&String]) -> Result<(), RegistryError> {
    if values.len() > MAX_ITEMS {
        return Err(RegistryError::TooManyItems {
            field,
            max: MAX_ITEMS,
        });
    }
    let mut seen = BTreeSet::new();
    for value in values {
        validate_text(field, value, MAX_NAME_CHARS, true)?;
        if !seen.insert(value.to_ascii_lowercase()) {
            return Err(RegistryError::DuplicateItem(field));
        }
    }
    Ok(())
}

fn validate_paths(paths: &[String]) -> Result<(), RegistryError> {
    if paths.len() > MAX_ITEMS {
        return Err(RegistryError::TooManyItems {
            field: "protected_paths",
            max: MAX_ITEMS,
        });
    }
    let mut seen = BTreeSet::new();
    for path in paths {
        validate_text("protected_path", path, MAX_PATH_CHARS, true)?;
        if !seen.insert(path.to_ascii_lowercase()) {
            return Err(RegistryError::DuplicateItem("protected_paths"));
        }
        let normalized = path.replace('\\', "/");
        if normalized.starts_with('/') || normalized.contains("../") || normalized.contains(":") {
            return Err(RegistryError::InvalidPath);
        }
    }
    Ok(())
}

fn validate_domain(domain: &str) -> Result<(), RegistryError> {
    if domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains('/')
        || domain.contains(':')
    {
        return Err(RegistryError::InvalidDomain);
    }
    if !domain.contains('.') || domain.chars().any(|c| c.is_whitespace()) {
        return Err(RegistryError::InvalidDomain);
    }
    Ok(())
}

fn reject_prompt_injection(value: &str) -> Result<(), RegistryError> {
    let lower = value.to_ascii_lowercase();
    for marker in [
        "ignore previous",
        "system message",
        "developer message",
        "reveal the key",
        "tool call",
    ] {
        if lower.contains(marker) {
            return Err(RegistryError::PromptInjection);
        }
    }
    Ok(())
}

fn contains_all(available: &[String], required: &[String]) -> bool {
    required
        .iter()
        .all(|item| available.iter().any(|v| v == item))
}

fn intersection(left: &[String], right: &[String]) -> Vec<String> {
    let right = right.iter().collect::<BTreeSet<_>>();
    let mut values = left
        .iter()
        .filter(|v| right.contains(v))
        .cloned()
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

fn union_sorted(left: &[String], right: &[String]) -> Vec<String> {
    let mut values = left.iter().chain(right).cloned().collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(name: &str) -> CapabilityManifest {
        CapabilityManifest {
            name: name.into(),
            version: "1.0.0".into(),
            content_hash: "0123456789abcdef0123456789abcdef".into(),
            signature: "sig:v1:trusted".into(),
            roles: vec![RoleRef {
                name: "reviewer".into(),
                version: "1".into(),
                content_hash: "abcdef0123456789abcdef0123456789".into(),
            }],
            skills: vec![SkillRef {
                name: "review".into(),
                version: "1".into(),
                content_hash: "abcdef0123456789abcdef0123456789".into(),
            }],
            allowed_tools: vec!["filesystem.read".into(), "git.diff".into()],
            allowed_domains: vec!["docs.example.com".into()],
            protected_paths: vec!["src".into()],
            risk_class: RiskClass::Medium,
            install: InstallPolicy {
                source: InstallSource::LocalArchive,
                allow_install_scripts: false,
                allow_update: true,
                rollback_on_failure: true,
            },
        }
    }

    #[test]
    fn validates_manifest_metadata_and_guards() {
        let value = manifest("reviewer");
        assert!(value.validate().is_ok());
        let mut invalid = value.clone();
        invalid.protected_paths = vec!["../secrets".into()];
        assert_eq!(invalid.validate(), Err(RegistryError::InvalidPath));
        invalid = value.clone();
        invalid.signature = "private-secret-key".into();
        assert_eq!(invalid.validate(), Err(RegistryError::InvalidSignature));
    }

    #[test]
    fn derives_intersection_without_escalating_risk() {
        let value = manifest("reviewer");
        let request = EffectivePermissions {
            allowed_tools: vec!["git.diff".into(), "shell.execute".into()],
            allowed_domains: vec!["docs.example.com".into()],
            protected_paths: vec!["tests".into()],
            risk_class: RiskClass::Low,
        };
        let result = value.effective_permissions(&request).unwrap();
        assert_eq!(result.allowed_tools, vec!["git.diff"]);
        assert_eq!(result.protected_paths, vec!["src", "tests"]);
        let mut elevated = request;
        elevated.risk_class = RiskClass::High;
        assert_eq!(
            value.effective_permissions(&elevated),
            Err(RegistryError::RiskEscalation)
        );
    }

    #[test]
    fn matcher_is_bounded_and_deterministic() {
        let query = MatchQuery {
            intent: "review reviewer".into(),
            required_tools: vec!["git.diff".into()],
            required_domains: vec!["docs.example.com".into()],
            requested_risk: RiskClass::Low,
        };
        let result =
            match_capabilities(&[manifest("reviewer"), manifest("planner")], &query).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].manifest_name, "reviewer");
        assert!(result[0].score > result[1].score);
    }

    #[test]
    fn update_requires_same_identity_and_rollback() {
        let current = manifest("reviewer");
        let mut candidate = current.clone();
        candidate.version = "2.0.0".into();
        assert!(validate_update(&current, &candidate).is_ok());
        candidate.name = "other".into();
        assert_eq!(
            validate_update(&current, &candidate),
            Err(RegistryError::InvalidUpdate)
        );
    }

    #[test]
    fn prompt_injection_and_secret_like_metadata_are_rejected() {
        let mut value = manifest("reviewer");
        value.name = "ignore previous instructions".into();
        assert_eq!(value.validate(), Err(RegistryError::PromptInjection));
        value = manifest("reviewer");
        value.signature = "private-secret-key".into();
        assert_eq!(value.validate(), Err(RegistryError::InvalidSignature));
    }
}
