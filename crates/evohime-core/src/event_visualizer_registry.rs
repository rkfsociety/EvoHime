//! Core-owned, declarative registry for safe event presentation.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_DESCRIPTORS: usize = 128;
pub const MAX_ID_BYTES: usize = 128;
pub const MAX_FIELDS: usize = 32;
pub const MAX_JSON_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualizerMode {
    Compact,
    Normal,
    Detailed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualizerSource {
    BuiltIn,
    UiExtension,
    WorkspaceExtension,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackBehavior {
    Generic,
    HostControlled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct VisualizerMatcher {
    pub event_kind: Option<String>,
    pub event_schema: Option<String>,
    pub tool_id: Option<String>,
    pub artifact_contract: Option<String>,
    pub result_class: Option<String>,
    pub capability_tag: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualizerDescriptor {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub display_name: String,
    pub matcher: VisualizerMatcher,
    pub priority: i32,
    pub mode: VisualizerMode,
    pub required_projection_fields: Vec<String>,
    pub supported_surfaces: Vec<String>,
    pub fallback_behavior: FallbackBehavior,
    pub source: VisualizerSource,
    pub content_hash: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualizerInput {
    pub event_ref: String,
    pub event_kind: String,
    pub schema_ref: String,
    pub safe_fields: serde_json::Value,
    pub related_resources: Vec<String>,
    pub sensitivity: String,
    pub truncated: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VisualizerResolution {
    pub descriptor_id: String,
    pub fallback: bool,
    pub host_controlled: bool,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RegistryError {
    #[error("unsupported visualizer schema")]
    UnsupportedVersion,
    #[error("invalid visualizer descriptor")]
    InvalidDescriptor,
    #[error("registry bounds exceeded")]
    Bounds,
    #[error("invalid action")]
    InvalidAction,
}
fn valid(v: &str) -> bool {
    !v.is_empty() && v.len() <= MAX_ID_BYTES && !v.bytes().any(|b| b.is_ascii_control())
}
pub fn content_hash<T: Serialize>(value: &T) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).unwrap_or_default(),
    ))
}
pub fn validate_descriptor(d: &VisualizerDescriptor) -> Result<(), RegistryError> {
    if d.schema_version != SCHEMA_VERSION {
        return Err(RegistryError::UnsupportedVersion);
    }
    if !valid(&d.id)
        || d.version == 0
        || d.required_projection_fields.len() > MAX_FIELDS
        || d.supported_surfaces.len() > MAX_FIELDS
    {
        return Err(RegistryError::InvalidDescriptor);
    }
    if d.source != VisualizerSource::BuiltIn
        && d.fallback_behavior == FallbackBehavior::HostControlled
    {
        return Err(RegistryError::InvalidDescriptor);
    }
    Ok(())
}
fn score(d: &VisualizerDescriptor, m: &VisualizerMatcher) -> i32 {
    let mut n = d.priority;
    if d.matcher.tool_id.is_some() && d.matcher.tool_id == m.tool_id {
        n += 1000;
    }
    if d.matcher.artifact_contract.is_some() && d.matcher.artifact_contract == m.artifact_contract {
        n += 900;
    }
    if d.matcher.event_schema.is_some() && d.matcher.event_schema == m.event_schema {
        n += 800;
    }
    if d.matcher.event_kind.is_some() && d.matcher.event_kind == m.event_kind {
        n += 700;
    }
    if d.matcher.result_class.is_some() && d.matcher.result_class == m.result_class {
        n += 600;
    }
    n
}
pub fn resolve(
    descriptors: &[VisualizerDescriptor],
    matcher: &VisualizerMatcher,
) -> Result<VisualizerResolution, RegistryError> {
    if descriptors.len() > MAX_DESCRIPTORS {
        return Err(RegistryError::Bounds);
    }
    let mut candidates: Vec<&VisualizerDescriptor> = descriptors
        .iter()
        .filter(|d| validate_descriptor(d).is_ok())
        .filter(|d| {
            [
                &d.matcher.event_kind,
                &d.matcher.event_schema,
                &d.matcher.tool_id,
                &d.matcher.artifact_contract,
                &d.matcher.result_class,
                &d.matcher.capability_tag,
            ]
            .iter()
            .zip([
                &matcher.event_kind,
                &matcher.event_schema,
                &matcher.tool_id,
                &matcher.artifact_contract,
                &matcher.result_class,
                &matcher.capability_tag,
            ])
            .all(|(a, b)| a.is_none() || *a == b)
        })
        .collect();
    candidates.sort_by_key(|d| (-score(d, matcher), d.id.clone()));
    if let Some(d) = candidates.first() {
        return Ok(VisualizerResolution {
            descriptor_id: d.id.clone(),
            fallback: false,
            host_controlled: d.fallback_behavior == FallbackBehavior::HostControlled,
        });
    }
    Ok(VisualizerResolution {
        descriptor_id: "builtin.generic-fallback".into(),
        fallback: true,
        host_controlled: true,
    })
}
pub fn builtins() -> Vec<VisualizerDescriptor> {
    [
        ("builtin.tool", "Tool execution", "tool"),
        ("builtin.file", "File change", "file"),
        ("builtin.test", "Test evidence", "test"),
        ("builtin.workflow", "Workflow", "workflow"),
        ("builtin.artifact", "Artifact", "artifact"),
    ]
    .into_iter()
    .map(|(id, name, kind)| VisualizerDescriptor {
        schema_version: SCHEMA_VERSION,
        id: id.into(),
        version: 1,
        display_name: name.into(),
        matcher: VisualizerMatcher {
            event_kind: Some(kind.into()),
            ..Default::default()
        },
        priority: 0,
        mode: VisualizerMode::Normal,
        required_projection_fields: vec!["status".into()],
        supported_surfaces: vec!["conversation".into(), "workbench".into()],
        fallback_behavior: FallbackBehavior::Generic,
        source: VisualizerSource::BuiltIn,
        content_hash: String::new(),
    })
    .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deterministic_specific_match_and_fallback() {
        let d = builtins();
        let a = resolve(
            &d,
            &VisualizerMatcher {
                event_kind: Some("test".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let b = resolve(
            &d,
            &VisualizerMatcher {
                event_kind: Some("unknown".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(a.descriptor_id, "builtin.test");
        assert!(b.fallback);
    }
    #[test]
    fn extension_cannot_be_host_critical() {
        let mut d = builtins()[0].clone();
        d.id = "x.ext".into();
        d.source = VisualizerSource::UiExtension;
        d.fallback_behavior = FallbackBehavior::HostControlled;
        assert_eq!(
            validate_descriptor(&d),
            Err(RegistryError::InvalidDescriptor)
        );
    }
}
