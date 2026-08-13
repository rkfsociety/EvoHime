//! Runtime/UI wiring contract for capability-registry selection.
//!
//! Pure, offline, deterministic logic sitting on top of
//! `capability_registry::match_capabilities`. It answers the UI-facing
//! question "which role/skill is selected for the current/next task, why,
//! and what does it grant" and lets a user pin the current selection (so a
//! later registry update or re-match cannot silently swap it) or explicitly
//! replace it with another manifest already present in the registry.
//!
//! Persistence of the pin/override choice is out of scope for this module
//! (see `evohime-local-storage::capability_selection_store`); this module
//! only defines the selection view and the pure state-transition rules a
//! store/IPC layer is expected to apply.

use crate::capability_registry::{
    match_capabilities, CapabilityManifest, EffectivePermissions, MatchQuery, RegistryError,
};
use serde::{Deserialize, Serialize};

/// What the UI shows for the manifest currently selected (or pinned) for a
/// task: identity, the matcher's reasons, and the permissions it grants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelectionView {
    pub manifest_name: String,
    pub version: String,
    pub reasons: Vec<String>,
    pub permissions: EffectivePermissions,
    pub acceptance_criteria: Vec<String>,
    /// True when the user pinned this selection, so future matcher runs
    /// must not silently swap it out for a higher-scoring manifest.
    pub pinned: bool,
}

/// How the current selection was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionOrigin {
    /// The deterministic matcher chose this manifest; a new match for the
    /// same query may replace it.
    Auto,
    /// The user pinned this manifest; the matcher must not replace it
    /// until the user unpins or explicitly replaces it.
    Pinned,
    /// The user explicitly picked a different manifest than the matcher
    /// would have chosen; treated the same as `Pinned` for auto-swap
    /// purposes.
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySelectionState {
    pub selection: CapabilitySelectionView,
    pub origin: SelectionOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    Registry(RegistryError),
    /// The matcher found no manifest satisfying the query.
    NoMatch,
    /// `replace` was asked for a manifest name not present in the registry
    /// (or not satisfying the query's own risk/tool/domain bounds).
    UnknownManifest,
}

impl std::fmt::Display for SelectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Registry(error) => write!(f, "{error}"),
            Self::NoMatch => write!(f, "no capability manifest satisfies the query"),
            Self::UnknownManifest => {
                write!(f, "requested manifest is not present in the registry")
            }
        }
    }
}

impl std::error::Error for SelectionError {}

impl From<RegistryError> for SelectionError {
    fn from(value: RegistryError) -> Self {
        Self::Registry(value)
    }
}

fn to_view(
    manifest: &CapabilityManifest,
    reasons: Vec<String>,
    permissions: EffectivePermissions,
    pinned: bool,
) -> CapabilitySelectionView {
    CapabilitySelectionView {
        manifest_name: manifest.name.clone(),
        version: manifest.version.clone(),
        reasons,
        permissions,
        acceptance_criteria: manifest
            .allowed_tools
            .iter()
            .map(|tool| format!("Tool `{tool}` remains within manifest bounds"))
            .collect(),
        pinned,
    }
}

/// Runs the deterministic matcher and returns a `CapabilitySelectionView`
/// for the top-ranked manifest, with human-readable reasons. Returns
/// `SelectionError::NoMatch` if nothing satisfies the query.
pub fn select_for_task(
    manifests: &[CapabilityManifest],
    query: &MatchQuery,
) -> Result<CapabilitySelectionView, SelectionError> {
    let matches = match_capabilities(manifests, query)?;
    let top = matches.first().ok_or(SelectionError::NoMatch)?;
    let manifest = manifests
        .iter()
        .find(|m| m.name == top.manifest_name)
        .ok_or(SelectionError::NoMatch)?;
    let mut reasons = Vec::new();
    if query
        .intent
        .to_ascii_lowercase()
        .contains(&manifest.name.to_ascii_lowercase())
    {
        reasons.push(format!("intent mentions manifest name `{}`", manifest.name));
    }
    if !query.required_tools.is_empty() {
        reasons.push(format!(
            "grants all {} required tool(s)",
            query.required_tools.len()
        ));
    }
    if !query.required_domains.is_empty() {
        reasons.push(format!(
            "grants all {} required domain(s)",
            query.required_domains.len()
        ));
    }
    reasons.push(format!("matcher score {}", top.score));
    Ok(to_view(manifest, reasons, top.permissions.clone(), false))
}

/// Given an existing (possibly pinned) selection state and a freshly
/// computed auto-match, decides what the UI should show next: a pinned or
/// replaced selection is never silently overwritten by a new auto-match.
pub fn reconcile_with_pin(
    current: Option<&CapabilitySelectionState>,
    fresh_auto_match: Result<CapabilitySelectionView, SelectionError>,
) -> Result<CapabilitySelectionState, SelectionError> {
    if let Some(state) = current {
        if state.origin != SelectionOrigin::Auto {
            // Pinned/replaced selections stick regardless of what the
            // matcher would now pick.
            return Ok(state.clone());
        }
    }
    fresh_auto_match.map(|selection| CapabilitySelectionState {
        selection,
        origin: SelectionOrigin::Auto,
    })
}

/// Pins the current selection so future matcher runs cannot swap it.
pub fn pin(mut state: CapabilitySelectionState) -> CapabilitySelectionState {
    state.selection.pinned = true;
    state.origin = SelectionOrigin::Pinned;
    state
}

/// Explicitly replaces the selection with `manifest_name` from the
/// registry, re-deriving permissions/reasons against the same query so the
/// UI never shows stale grants for the newly chosen manifest. Rejects
/// manifests that are not present, invalid, or that do not satisfy the
/// query's own risk/tool/domain bounds -- a user cannot "replace" their way
/// into a manifest the matcher would never have offered.
pub fn replace(
    manifests: &[CapabilityManifest],
    query: &MatchQuery,
    manifest_name: &str,
) -> Result<CapabilitySelectionState, SelectionError> {
    let matches = match_capabilities(manifests, query)?;
    let matched = matches
        .iter()
        .find(|m| m.manifest_name == manifest_name)
        .ok_or(SelectionError::UnknownManifest)?;
    let manifest = manifests
        .iter()
        .find(|m| m.name == manifest_name)
        .ok_or(SelectionError::UnknownManifest)?;
    let view = to_view(
        manifest,
        vec!["explicitly selected by user".to_string()],
        matched.permissions.clone(),
        true,
    );
    Ok(CapabilitySelectionState {
        selection: view,
        origin: SelectionOrigin::Replaced,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_registry::{InstallPolicy, InstallSource, RiskClass, RoleRef, SkillRef};

    fn manifest(name: &str) -> CapabilityManifest {
        let content_hash = "0123456789abcdef0123456789abcdef".to_string();
        let signature = crate::capability_registry::test_sign_with_trusted_key(
            name,
            "1.0.0",
            &content_hash,
        );
        CapabilityManifest {
            name: name.into(),
            version: "1.0.0".into(),
            content_hash,
            signature,
            signing_key_id: "evohime-dev-1".into(),
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

    fn query() -> MatchQuery {
        MatchQuery {
            intent: "review reviewer".into(),
            required_tools: vec!["git.diff".into()],
            required_domains: vec!["docs.example.com".into()],
            requested_risk: RiskClass::Low,
        }
    }

    #[test]
    fn selects_top_match_with_reasons_risk_tools_and_acceptance_criteria() {
        let manifests = vec![manifest("reviewer"), manifest("planner")];
        let view = select_for_task(&manifests, &query()).unwrap();
        assert_eq!(view.manifest_name, "reviewer");
        assert_eq!(view.version, "1.0.0");
        assert!(!view.reasons.is_empty());
        assert_eq!(view.permissions.allowed_tools, vec!["git.diff".to_string()]);
        assert!(!view.acceptance_criteria.is_empty());
        assert!(!view.pinned);
    }

    #[test]
    fn pin_prevents_auto_swap_on_next_reconcile() {
        let manifests = vec![manifest("reviewer")];
        let auto = select_for_task(&manifests, &query()).unwrap();
        let state = pin(CapabilitySelectionState {
            selection: auto,
            origin: SelectionOrigin::Auto,
        });
        assert_eq!(state.origin, SelectionOrigin::Pinned);
        assert!(state.selection.pinned);

        // A fresh auto-match (e.g. a different top-scoring manifest after a
        // registry update) must not override the pin.
        let other_manifests = vec![manifest("planner"), manifest("reviewer")];
        let fresh = select_for_task(&other_manifests, &query());
        let reconciled = reconcile_with_pin(Some(&state), fresh).unwrap();
        assert_eq!(reconciled, state);
    }

    #[test]
    fn reconcile_follows_matcher_when_not_pinned() {
        let manifests = vec![manifest("reviewer")];
        let auto = select_for_task(&manifests, &query()).unwrap();
        let state = CapabilitySelectionState {
            selection: auto.clone(),
            origin: SelectionOrigin::Auto,
        };
        let fresh = select_for_task(&manifests, &query());
        let reconciled = reconcile_with_pin(Some(&state), fresh).unwrap();
        assert_eq!(reconciled.selection.manifest_name, "reviewer");
        assert_eq!(reconciled.origin, SelectionOrigin::Auto);
    }

    #[test]
    fn replace_switches_to_explicit_manifest_and_marks_replaced() {
        let manifests = vec![manifest("reviewer"), manifest("planner")];
        let replaced = replace(&manifests, &query(), "planner").unwrap();
        assert_eq!(replaced.selection.manifest_name, "planner");
        assert_eq!(replaced.origin, SelectionOrigin::Replaced);
        assert!(replaced.selection.pinned);
        // Replaced selections also stick across reconcile.
        let fresh = select_for_task(&manifests, &query());
        let reconciled = reconcile_with_pin(Some(&replaced), fresh).unwrap();
        assert_eq!(reconciled, replaced);
    }

    #[test]
    fn replace_rejects_manifest_outside_registry_or_query_bounds() {
        let manifests = vec![manifest("reviewer")];
        assert_eq!(
            replace(&manifests, &query(), "ghost"),
            Err(SelectionError::UnknownManifest)
        );

        let mut high_risk_query = query();
        high_risk_query.required_tools = vec!["shell.execute".into()];
        assert_eq!(
            replace(&manifests, &high_risk_query, "reviewer"),
            Err(SelectionError::UnknownManifest)
        );
    }
}
