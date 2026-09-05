//! Core-owned, metadata-only architecture snapshots.
//!
//! Repository facts are untrusted input. This module only validates bounded
//! identities and provenance; extraction and filesystem authorization remain in
//! the Core runtime boundary.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_ID: usize = 128;
pub const MAX_TEXT: usize = 1024;
pub const MAX_ITEMS: usize = 512;
pub const MAX_EVIDENCE: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactState {
    Verified,
    Reviewed,
    Candidate,
    Unsupported,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    Fresh,
    PossiblyStale,
    Stale,
    Refreshing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRef {
    pub file_ref: String,
    pub file_revision: String,
    pub content_hash: String,
    pub evidence_kind: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Component {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub responsibility: String,
    pub state: FactState,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub state: FactState,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boundary {
    pub id: String,
    pub name: String,
    pub state: FactState,
    pub members: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub extractor_version: String,
    pub covered_kinds: Vec<String>,
    pub diagnostics: Vec<CoverageDiagnostic>,
    pub omissions: Vec<Omission>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageDiagnostic {
    pub code: String,
    pub detail: String,
    pub state: FactState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Omission {
    pub id: String,
    pub reason: String,
    pub bound_revision: String,
    pub state: FactState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureSnapshot {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub workspace_identity: String,
    pub source_revision: String,
    pub freshness: Freshness,
    pub components: Vec<Component>,
    pub relationships: Vec<Relationship>,
    pub boundaries: Vec<Boundary>,
    pub coverage: Coverage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Removed,
    Changed,
    PossibleRename,
    IdentityUncertain,
    BoundaryChanged,
    EvidenceChanged,
    CoverageChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeltaEntry {
    pub kind: ChangeKind,
    pub subject_id: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureDelta {
    pub schema_version: u32,
    pub before_hash: String,
    pub after_hash: String,
    pub entries: Vec<DeltaEntry>,
    pub delta_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpectedArchitectureDelta {
    pub schema_version: u32,
    pub expected: Vec<DeltaEntry>,
    pub baseline_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchitectureChangeReview {
    pub schema_version: u32,
    pub unexpected: Vec<DeltaEntry>,
    pub missing: Vec<DeltaEntry>,
    pub ambiguous: Vec<DeltaEntry>,
    pub verdict: String,
}

pub type ArchitectureComponent = Component;
pub type ArchitectureRelationship = Relationship;
pub type ArchitectureBoundary = Boundary;
pub type ArchitectureEvidenceRef = EvidenceRef;
pub type ArchitectureCoverageProfile = Coverage;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid architecture snapshot: {0}")]
    Invalid(&'static str),
    #[error("architecture snapshot is oversized")]
    Oversized,
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(|c| c == '\0')
}
fn validate_evidence(evidence: &[EvidenceRef]) -> Result<(), Error> {
    if evidence.len() > MAX_EVIDENCE {
        return Err(Error::Oversized);
    }
    for item in evidence {
        if !valid_text(&item.file_ref, MAX_ID)
            || !valid_text(&item.file_revision, MAX_ID)
            || !valid_text(&item.content_hash, MAX_ID)
            || !valid_text(&item.evidence_kind, MAX_ID)
        {
            return Err(Error::Invalid("evidence"));
        }
        if item
            .start_line
            .zip(item.end_line)
            .is_some_and(|(a, b)| a == 0 || b < a)
        {
            return Err(Error::Invalid("evidence_range"));
        }
    }
    Ok(())
}

pub fn validate(snapshot: &ArchitectureSnapshot) -> Result<(), Error> {
    if snapshot.schema_version != CONTRACT_VERSION
        || !valid_text(&snapshot.snapshot_id, MAX_ID)
        || !valid_text(&snapshot.workspace_identity, MAX_ID)
        || !valid_text(&snapshot.source_revision, MAX_ID)
    {
        return Err(Error::Invalid("header"));
    }
    if snapshot.components.len() > MAX_ITEMS
        || snapshot.relationships.len() > MAX_ITEMS
        || snapshot.boundaries.len() > MAX_ITEMS
    {
        return Err(Error::Oversized);
    }
    let ids: BTreeMap<_, _> = snapshot.components.iter().map(|c| (&c.id, c)).collect();
    if ids.len() != snapshot.components.len() {
        return Err(Error::Invalid("duplicate_component"));
    }
    for c in &snapshot.components {
        if !valid_text(&c.id, MAX_ID)
            || !valid_text(&c.kind, MAX_ID)
            || !valid_text(&c.name, MAX_TEXT)
            || c.responsibility.len() > MAX_TEXT
        {
            return Err(Error::Invalid("component"));
        }
        validate_evidence(&c.evidence)?;
    }
    let mut relationship_ids = BTreeMap::new();
    for r in &snapshot.relationships {
        if !valid_text(&r.id, MAX_ID) || !ids.contains_key(&r.from) || !ids.contains_key(&r.to) {
            return Err(Error::Invalid("relationship"));
        }
        if relationship_ids.insert(&r.id, ()).is_some() {
            return Err(Error::Invalid("duplicate_relationship"));
        }
        validate_evidence(&r.evidence)?;
    }
    for b in &snapshot.boundaries {
        if !valid_text(&b.id, MAX_ID)
            || !valid_text(&b.name, MAX_TEXT)
            || b.members.len() > MAX_ITEMS
            || b.members.iter().any(|id| !ids.contains_key(id))
        {
            return Err(Error::Invalid("boundary"));
        }
        validate_evidence(&b.evidence)?;
    }
    if !valid_text(&snapshot.coverage.extractor_version, MAX_ID)
        || snapshot.coverage.covered_kinds.len() > MAX_ITEMS
        || snapshot.coverage.diagnostics.len() > MAX_ITEMS
        || snapshot.coverage.omissions.len() > MAX_ITEMS
    {
        return Err(Error::Invalid("coverage"));
    }
    for diagnostic in &snapshot.coverage.diagnostics {
        if !valid_text(&diagnostic.code, MAX_ID)
            || diagnostic.detail.len() > MAX_TEXT
            || diagnostic.detail.contains('\0')
        {
            return Err(Error::Invalid("coverage_diagnostic"));
        }
    }
    for omission in &snapshot.coverage.omissions {
        if !valid_text(&omission.id, MAX_ID)
            || omission.reason.len() > MAX_TEXT
            || omission.reason.contains('\0')
            || !valid_text(&omission.bound_revision, MAX_ID)
        {
            return Err(Error::Invalid("omission"));
        }
    }
    Ok(())
}

pub fn canonical_hash<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("architecture contract is serializable");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn snapshot_hash(snapshot: &ArchitectureSnapshot) -> Result<String, Error> {
    validate(snapshot)?;
    Ok(canonical_hash(snapshot))
}

/// Cache identity is deliberately independent from the display snapshot id.
/// Any change to one of these inputs creates a new derived revision.
pub fn cache_key(
    workspace_set_identity: &str,
    root_identity: &str,
    source_fingerprint: &str,
    extractor_version: &str,
) -> String {
    canonical_hash(&(
        CONTRACT_VERSION,
        workspace_set_identity,
        root_identity,
        source_fingerprint,
        extractor_version,
    ))
}

pub fn identity_match_key(component: &Component) -> String {
    canonical_hash(&(component.kind.as_str(), component.name.as_str()))
}

pub fn delta(
    before: &ArchitectureSnapshot,
    after: &ArchitectureSnapshot,
) -> Result<ArchitectureDelta, Error> {
    validate(before)?;
    validate(after)?;
    if before.workspace_identity != after.workspace_identity {
        return Err(Error::Invalid("incompatible_workspace"));
    }
    let before_hash = canonical_hash(before);
    let after_hash = canonical_hash(after);
    let mut entries = Vec::new();
    compare_entities(
        &mut entries,
        &before.components,
        &after.components,
        "component",
        |x| &x.id,
    );
    compare_entities(
        &mut entries,
        &before.relationships,
        &after.relationships,
        "relationship",
        |x| &x.id,
    );
    compare_entities(
        &mut entries,
        &before.boundaries,
        &after.boundaries,
        "boundary",
        |x| &x.id,
    );
    if before.coverage != after.coverage {
        entries.push(DeltaEntry {
            kind: ChangeKind::CoverageChanged,
            subject_id: "coverage".into(),
            detail: "coverage".into(),
        });
    }
    entries.sort_by(|a, b| {
        (format!("{:?}", a.kind), &a.subject_id, &a.detail).cmp(&(
            format!("{:?}", b.kind),
            &b.subject_id,
            &b.detail,
        ))
    });
    let delta_hash = canonical_hash(&entries);
    Ok(ArchitectureDelta {
        schema_version: CONTRACT_VERSION,
        before_hash,
        after_hash,
        entries,
        delta_hash,
    })
}

fn compare_entities<T: PartialEq, F: Fn(&T) -> &String>(
    entries: &mut Vec<DeltaEntry>,
    before: &[T],
    after: &[T],
    kind: &str,
    id: F,
) {
    let bm: BTreeMap<_, _> = before.iter().map(|x| (id(x), x)).collect();
    let am: BTreeMap<_, _> = after.iter().map(|x| (id(x), x)).collect();
    for key in bm
        .keys()
        .chain(am.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        match (bm.get(key), am.get(key)) {
            (None, Some(_)) => entries.push(DeltaEntry {
                kind: ChangeKind::Added,
                subject_id: (*key).clone(),
                detail: kind.into(),
            }),
            (Some(_), None) => entries.push(DeltaEntry {
                kind: ChangeKind::Removed,
                subject_id: (*key).clone(),
                detail: kind.into(),
            }),
            (Some(a), Some(b)) if a != b => entries.push(DeltaEntry {
                kind: ChangeKind::Changed,
                subject_id: (*key).clone(),
                detail: kind.into(),
            }),
            _ => {}
        }
    }
}

pub fn review(
    expected: &ExpectedArchitectureDelta,
    actual: &ArchitectureDelta,
) -> Result<ArchitectureChangeReview, Error> {
    if expected.schema_version != CONTRACT_VERSION || expected.baseline_hash != actual.before_hash {
        return Err(Error::Invalid("expected_baseline"));
    }
    let expected_map: BTreeMap<_, _> = expected
        .expected
        .iter()
        .map(|e| (&e.subject_id, e))
        .collect();
    let actual_map: BTreeMap<_, _> = actual.entries.iter().map(|e| (&e.subject_id, e)).collect();
    let mut missing = Vec::new();
    let mut unexpected = Vec::new();
    let ambiguous = Vec::new();
    for (id, item) in &expected_map {
        if !actual_map.contains_key(id) {
            missing.push((*item).clone());
        }
    }
    for (id, item) in &actual_map {
        if !expected_map.contains_key(id) {
            unexpected.push((*item).clone());
        }
    }
    let verdict = if !ambiguous.is_empty() {
        "require_review"
    } else if !missing.is_empty() || !unexpected.is_empty() {
        "warn"
    } else {
        "match"
    };
    Ok(ArchitectureChangeReview {
        schema_version: CONTRACT_VERSION,
        unexpected,
        missing,
        ambiguous,
        verdict: verdict.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> ArchitectureSnapshot {
        ArchitectureSnapshot {
            schema_version: 1,
            snapshot_id: "s1".into(),
            workspace_identity: "root-a".into(),
            source_revision: "r1".into(),
            freshness: Freshness::Fresh,
            components: vec![Component {
                id: "core".into(),
                kind: "process".into(),
                name: "Core".into(),
                responsibility: "state".into(),
                state: FactState::Verified,
                evidence: vec![EvidenceRef {
                    file_ref: "crates/evohime-core/src/lib.rs".into(),
                    file_revision: "r1".into(),
                    content_hash: "abc".into(),
                    evidence_kind: "manifest".into(),
                    start_line: Some(1),
                    end_line: Some(2),
                    symbol: None,
                }],
            }],
            relationships: vec![],
            boundaries: vec![],
            coverage: Coverage {
                extractor_version: crate::routing_trace::ROUTING_CATALOG_VERSION.into(),
                covered_kinds: vec!["process".into()],
                diagnostics: vec![],
                omissions: vec![],
            },
        }
    }
    #[test]
    fn hash_is_stable() {
        let s = sample();
        assert_eq!(snapshot_hash(&s), snapshot_hash(&s));
    }
    #[test]
    fn relationship_endpoint_is_checked() {
        let mut s = sample();
        s.relationships.push(Relationship {
            id: "x".into(),
            from: "core".into(),
            to: "missing".into(),
            kind: "calls".into(),
            state: FactState::Candidate,
            evidence: vec![],
        });
        assert_eq!(validate(&s), Err(Error::Invalid("relationship")));
    }
    #[test]
    fn delta_is_deterministic() {
        let a = sample();
        let mut b = a.clone();
        b.snapshot_id = "s2".into();
        b.components[0].responsibility = "state and ipc".into();
        let d1 = delta(&a, &b).unwrap();
        let d2 = delta(&a, &b).unwrap();
        assert_eq!(d1, d2);
        assert_eq!(d1.entries[0].kind, ChangeKind::Changed);
    }
    #[test]
    fn candidate_never_becomes_verified() {
        let mut s = sample();
        s.components[0].state = FactState::Candidate;
        assert_eq!(s.components[0].state, FactState::Candidate);
        assert!(validate(&s).is_ok());
    }
    #[test]
    fn expected_delta_is_fail_closed() {
        let a = sample();
        let mut b = a.clone();
        b.snapshot_id = "s2".into();
        b.components[0].responsibility = "changed".into();
        let d = delta(&a, &b).unwrap();
        let result = review(
            &ExpectedArchitectureDelta {
                schema_version: 1,
                expected: d.entries.clone(),
                baseline_hash: d.before_hash.clone(),
            },
            &d,
        )
        .unwrap();
        assert_eq!(result.verdict, "match");
        assert_eq!(
            review(
                &ExpectedArchitectureDelta {
                    schema_version: 1,
                    expected: vec![],
                    baseline_hash: "wrong".into()
                },
                &d
            ),
            Err(Error::Invalid("expected_baseline"))
        );
    }

    #[test]
    fn cache_key_changes_when_any_authority_input_changes() {
        let a = cache_key("set", "root", "source", "extractor-v1");
        assert_ne!(a, cache_key("set", "root", "source-2", "extractor-v1"));
        assert_eq!(a, cache_key("set", "root", "source", "extractor-v1"));
    }

    #[test]
    fn identity_matching_does_not_use_display_name_alone() {
        let a = sample().components[0].clone();
        let mut b = a.clone();
        b.kind = "worker".into();
        assert_ne!(identity_match_key(&a), identity_match_key(&b));
    }
}
