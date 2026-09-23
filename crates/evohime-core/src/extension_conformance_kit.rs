//! Deterministic, in-memory conformance harness for extension surfaces.
//!
//! The kit verifies descriptors and probes; it never grants trust/capabilities
//! and never runs production credentials or extension code. Registration is a
//! staged transaction and is discarded on any failed assertion.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Current schema version for extension conformance records.
pub const SCHEMA_VERSION: u32 = 1;
/// Maximum number of descriptors staged in one registration transaction.
pub const MAX_SUBJECTS: usize = 64;
/// Maximum length of a stable extension identifier.
pub const MAX_ID: usize = 128;
/// Maximum number of capability references in a descriptor.
pub const MAX_CAPABILITIES: usize = 64;
/// Maximum serialized conformance report size.
pub const MAX_REPORT_BYTES: usize = 64 * 1024;

/// Extension surface whose descriptor and behavior are checked.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionKind {
    /// Provider that supplies an external service integration.
    IntegrationProvider,
    /// Adapter that invokes an external coding or task agent.
    ExternalAgentAdapter,
    /// Workbench surface registered by an extension.
    Workbench,
    /// User-interface extension surface.
    UiExtension,
    /// Provider of declarative runtime components.
    DeclarativeComponentProvider,
}
/// Deterministic fault injected into registration or delivery processing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaultMode {
    /// Run without an injected fault.
    None,
    /// Fail before registration commit begins.
    BeforeCommit,
    /// Fail after preparation but before commit.
    AfterPrepare,
    /// Simulate duplicate delivery during registration.
    DuplicateDelivery,
    /// Simulate an unsupported API version.
    UnsupportedVersion,
}
/// Versioned, hash-bound declaration of an extension subject.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionDescriptor {
    /// Descriptor schema version.
    pub schema_version: u32,
    /// Stable identity of the registered extension subject.
    pub subject_id: String,
    /// Kind of extension represented by this descriptor.
    pub kind: ExtensionKind,
    /// Provider responsible for implementing the subject.
    pub provider_id: String,
    /// Instance identity used to detect cross-instance leakage.
    pub instance_id: String,
    /// Extension API version.
    pub api_version: u32,
    /// Capabilities the descriptor requests.
    pub capability_refs: Vec<String>,
    /// Whether the extension is disabled for this conformance run.
    pub disabled: bool,
    /// Digest of the descriptor with this field cleared.
    pub descriptor_hash: String,
}
/// Bounded assertions supplied by an isolated extension test fixture.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConformanceProbe {
    /// API version the probe claims to implement.
    pub api_version: u32,
    /// Instance identity returned by the probe.
    pub instance_id: String,
    /// Required checks specific to the extension kind.
    pub specialized_checks: BTreeMap<String, bool>,
    /// Number of side effects observed while enabled.
    pub side_effect_count: u32,
    /// Number of side effects observed while disabled.
    pub disabled_side_effect_count: u32,
    /// Security assertions that must all pass.
    pub security_assertions: BTreeMap<String, bool>,
}
/// Deterministic result of descriptor and probe conformance checks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConformanceReport {
    /// Report schema version.
    pub schema_version: u32,
    /// Subject whose descriptor was evaluated.
    pub subject_id: String,
    /// Extension kind whose specialized suite ran.
    pub kind: ExtensionKind,
    /// Whether all required checks passed.
    pub passed: bool,
    /// Named conformance assertions and their results.
    pub checks: BTreeMap<String, bool>,
    /// Injected fault and diagnostic metadata.
    pub faults: BTreeMap<String, String>,
    /// Digest of the report with this field cleared.
    pub report_hash: String,
}
/// Invalid descriptors, failed assertions, or rolled-back registration.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConformanceError {
    /// Input uses an unsupported conformance schema version.
    #[error("unsupported conformance schema version")]
    UnsupportedVersion,
    /// Descriptor identity, bounds, or digest is invalid.
    #[error("invalid extension descriptor: {0}")]
    Invalid(&'static str),
    /// A required isolation, security, or kind-specific assertion failed.
    #[error("conformance assertion failed: {0}")]
    Failed(&'static str),
    /// A staged registration was discarded after validation or injected failure.
    #[error("registration transaction rolled back")]
    RolledBack,
    /// A subject identifier was staged more than once.
    #[error("duplicate registration")]
    Duplicate,
}
fn bounded(v: &str) -> bool {
    !v.is_empty() && v.len() <= MAX_ID && !v.chars().any(char::is_control)
}
/// Computes the stable descriptor digest with its hash field cleared.
pub fn descriptor_hash(d: &ExtensionDescriptor) -> String {
    let mut c = d.clone();
    c.descriptor_hash.clear();
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(&c).unwrap_or_default()))
    )
}
/// Validates descriptor identity, bounds, schema, and integrity digest.
pub fn validate_descriptor(d: &ExtensionDescriptor) -> Result<(), ConformanceError> {
    if d.schema_version != SCHEMA_VERSION {
        return Err(ConformanceError::UnsupportedVersion);
    };
    if !bounded(&d.subject_id)
        || !bounded(&d.provider_id)
        || !bounded(&d.instance_id)
        || d.api_version == 0
        || d.capability_refs.len() > MAX_CAPABILITIES
    {
        return Err(ConformanceError::Invalid("identity/version/bounds"));
    };
    if d.capability_refs.iter().any(|c| !bounded(c)) {
        return Err(ConformanceError::Invalid("capability ref"));
    };
    if d.descriptor_hash != descriptor_hash(d) {
        return Err(ConformanceError::Invalid("descriptor hash"));
    };
    Ok(())
}
/// Checks kind-specific probe coverage and requires every security assertion.
pub fn specialized_suite(
    kind: &ExtensionKind,
    probe: &ConformanceProbe,
) -> Result<(), ConformanceError> {
    if probe.api_version == 0 || !bounded(&probe.instance_id) {
        return Err(ConformanceError::Failed("probe identity"));
    };
    let required = match kind {
        ExtensionKind::IntegrationProvider => "integration_provider_contract",
        ExtensionKind::ExternalAgentAdapter => "external_agent_adapter_contract",
        ExtensionKind::Workbench => "workbench_contract",
        ExtensionKind::UiExtension => "ui_extension_contract",
        ExtensionKind::DeclarativeComponentProvider => "declarative_component_provider_contract",
    };
    if probe.specialized_checks.get(required) != Some(&true) {
        return Err(ConformanceError::Failed("specialized suite"));
    };
    if probe.security_assertions.values().any(|v| !*v) {
        return Err(ConformanceError::Failed("security assertion"));
    };
    Ok(())
}
/// Runs a deterministic conformance check without executing extension code.
pub fn run(
    d: &ExtensionDescriptor,
    probe: &ConformanceProbe,
    fault: FaultMode,
) -> Result<ConformanceReport, ConformanceError> {
    validate_descriptor(d)?;
    if fault == FaultMode::UnsupportedVersion {
        return Err(ConformanceError::UnsupportedVersion);
    };
    if d.instance_id != probe.instance_id {
        return Err(ConformanceError::Failed("instance isolation"));
    };
    specialized_suite(&d.kind, probe)?;
    if d.disabled && probe.disabled_side_effect_count != 0 {
        return Err(ConformanceError::Failed("disabled side effect"));
    };
    if !d.disabled && probe.side_effect_count > 0 {
        return Err(ConformanceError::Failed("probe side effect"));
    };
    let mut checks = BTreeMap::new();
    checks.insert("descriptor_valid".into(), true);
    checks.insert("instance_isolated".into(), true);
    checks.insert("security_assertions".into(), true);
    checks.insert(
        "disabled_no_side_effect".into(),
        probe.disabled_side_effect_count == 0,
    );
    checks.insert("specialized_suite".into(), true);
    let mut faults = BTreeMap::new();
    faults.insert(
        "mode".into(),
        serde_json::to_string(&fault).unwrap_or_default(),
    );
    let mut report = ConformanceReport {
        schema_version: SCHEMA_VERSION,
        subject_id: d.subject_id.clone(),
        kind: d.kind.clone(),
        passed: true,
        checks,
        faults,
        report_hash: String::new(),
    };
    report.report_hash = report_hash(&report);
    Ok(report)
}
/// Computes the stable digest of a conformance report with its hash field cleared.
pub fn report_hash(r: &ConformanceReport) -> String {
    let mut c = r.clone();
    c.report_hash.clear();
    format!(
        "sha256:{}",
        hex::encode(Sha256::digest(serde_json::to_vec(&c).unwrap_or_default()))
    )
}

/// Stages validated descriptors and commits them atomically after checks pass.
#[derive(Default)]
pub struct RegistrationTransaction {
    staged: BTreeMap<String, ExtensionDescriptor>,
}
impl RegistrationTransaction {
    /// Validates and stages one descriptor, rejecting duplicate subjects or overflow.
    pub fn stage(&mut self, d: ExtensionDescriptor) -> Result<(), ConformanceError> {
        validate_descriptor(&d)?;
        if self.staged.contains_key(&d.subject_id) {
            return Err(ConformanceError::Duplicate);
        };
        if self.staged.len() >= MAX_SUBJECTS {
            return Err(ConformanceError::Invalid("subject bound"));
        };
        self.staged.insert(d.subject_id.clone(), d);
        Ok(())
    }
    /// Validates all staged entries and either commits the batch or rolls it back.
    pub fn commit(self, fault: FaultMode) -> Result<Vec<ExtensionDescriptor>, ConformanceError> {
        if !self.staged.values().all(|d| validate_descriptor(d).is_ok()) {
            return Err(ConformanceError::RolledBack);
        };
        if matches!(
            fault,
            FaultMode::BeforeCommit | FaultMode::AfterPrepare | FaultMode::DuplicateDelivery
        ) {
            return Err(ConformanceError::RolledBack);
        };
        Ok(self.staged.into_values().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d() -> ExtensionDescriptor {
        let mut d = ExtensionDescriptor {
            schema_version: 1,
            subject_id: "s".into(),
            kind: ExtensionKind::Workbench,
            provider_id: "p".into(),
            instance_id: "i".into(),
            api_version: 1,
            capability_refs: vec![],
            disabled: false,
            descriptor_hash: String::new(),
        };
        d.descriptor_hash = descriptor_hash(&d);
        d
    }
    fn p() -> ConformanceProbe {
        ConformanceProbe {
            api_version: 1,
            instance_id: "i".into(),
            specialized_checks: [("workbench_contract".into(), true)].into_iter().collect(),
            side_effect_count: 0,
            disabled_side_effect_count: 0,
            security_assertions: [("no_escape".into(), true)].into_iter().collect(),
        }
    }
    #[test]
    fn report_is_hash_bound_and_specialized() {
        let r = run(&d(), &p(), FaultMode::None).unwrap();
        assert!(r.passed);
        assert_eq!(r.report_hash, report_hash(&r));
    }
    #[test]
    fn transaction_rolls_back_deterministically() {
        let mut t = RegistrationTransaction::default();
        t.stage(d()).unwrap();
        assert_eq!(
            t.commit(FaultMode::AfterPrepare),
            Err(ConformanceError::RolledBack)
        );
    }
}
