//! Policy gate: require research evidence before a bounded run proceeds
//! when the run's task/scope touches security, dependency-management, or
//! external-API-related work.
//!
//! Classification is deliberately simple: substring/keyword heuristics over
//! the task text, documented as heuristics rather than claimed as semantic
//! understanding, mirroring the other bounded policy layers in this
//! codebase (`research_pipeline`, `network_capability`). The gate itself
//! does not touch storage; the caller supplies the evidence already
//! associated with the run (e.g. loaded via `evohime-local-storage`'s
//! `research_store`) so this module stays a pure decision function.

use std::collections::BTreeSet;
use std::fmt;

use crate::research::ResearchEvidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SensitiveCategory {
    Security,
    Dependency,
    ExternalApi,
}

impl fmt::Display for SensitiveCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Security => write!(f, "security"),
            Self::Dependency => write!(f, "dependency"),
            Self::ExternalApi => write!(f, "external_api"),
        }
    }
}

const SECURITY_KEYWORDS: &[&str] = &[
    "security",
    "vulnerability",
    "cve",
    "auth",
    "authentication",
    "authorization",
    "encrypt",
    "credential",
    "secret",
    "exploit",
    "ssrf",
    "xss",
    "csrf",
    "sql injection",
    "sanitiz",
    "permission",
    "sandbox",
];

const DEPENDENCY_KEYWORDS: &[&str] = &[
    "dependency",
    "dependencies",
    "cargo.toml",
    "cargo add",
    "cargo update",
    "package.json",
    "npm install",
    "npm update",
    "pip install",
    "requirements.txt",
    "upgrade version",
    "bump version",
    "yarn add",
    "crate version",
];

const API_KEYWORDS: &[&str] = &[
    "external api",
    "rest api",
    "graphql",
    "webhook",
    "api endpoint",
    "third-party api",
    "third party api",
    "integrate with",
    "api key",
    "oauth",
    "sdk integration",
];

/// Heuristic classifier: lowercases `task_text` and matches known keyword
/// lists. Returns the empty set when nothing matches, in which case the
/// gate does not apply.
pub fn classify(task_text: &str) -> BTreeSet<SensitiveCategory> {
    let lowered = task_text.to_ascii_lowercase();
    let mut categories = BTreeSet::new();
    if SECURITY_KEYWORDS.iter().any(|kw| lowered.contains(kw)) {
        categories.insert(SensitiveCategory::Security);
    }
    if DEPENDENCY_KEYWORDS.iter().any(|kw| lowered.contains(kw)) {
        categories.insert(SensitiveCategory::Dependency);
    }
    if API_KEYWORDS.iter().any(|kw| lowered.contains(kw)) {
        categories.insert(SensitiveCategory::ExternalApi);
    }
    categories
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// The run's scope did not match any sensitive category, or fresh
    /// research evidence already exists for it.
    Allow,
    /// The run touches at least one sensitive category and no fresh
    /// research evidence was supplied; the run must be blocked (or the
    /// caller should prompt the user) before proceeding.
    RequireResearch {
        categories: BTreeSet<SensitiveCategory>,
    },
}

impl GateDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow)
    }
}

/// Decides whether `task_text` may run given the `ResearchEvidence` records
/// already gathered for this run. At least one evidence record must be
/// fresh at `now_ms` (per `ResearchEvidence::is_fresh_at`, i.e. respecting
/// its TTL contract) for the gate to allow a sensitive-category run.
pub fn check_research_gate(
    task_text: &str,
    evidence_for_run: &[ResearchEvidence],
    now_ms: u64,
) -> GateDecision {
    let categories = classify(task_text);
    if categories.is_empty() {
        return GateDecision::Allow;
    }
    let has_fresh_evidence = evidence_for_run
        .iter()
        .any(|evidence| evidence.is_fresh_at(now_ms));
    if has_fresh_evidence {
        GateDecision::Allow
    } else {
        GateDecision::RequireResearch { categories }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::SourceMetadata;

    fn evidence(captured_at_ms: u64, ttl_ms: u64) -> ResearchEvidence {
        let source = SourceMetadata::new(
            "https://example.test/a",
            "Example",
            "Example Org",
            "text/html",
            captured_at_ms,
        )
        .unwrap();
        ResearchEvidence::capture(source, "finding", captured_at_ms, ttl_ms).unwrap()
    }

    #[test]
    fn classifies_security_dependency_and_api_scopes_by_keyword() {
        assert!(classify("Investigate a possible SSRF vulnerability")
            .contains(&SensitiveCategory::Security));
        assert!(
            classify("Bump the cargo.toml dependency to the latest patch")
                .contains(&SensitiveCategory::Dependency)
        );
        assert!(classify("Integrate with the third-party REST API")
            .contains(&SensitiveCategory::ExternalApi));
        assert!(classify("Rename a local variable for readability").is_empty());
    }

    #[test]
    fn non_sensitive_task_is_always_allowed_without_evidence() {
        let decision = check_research_gate("Fix a typo in the README", &[], 10_000);
        assert_eq!(decision, GateDecision::Allow);
    }

    #[test]
    fn sensitive_task_without_evidence_is_blocked() {
        let decision = check_research_gate(
            "Add authentication middleware for the login endpoint",
            &[],
            10_000,
        );
        assert!(!decision.is_allowed());
        match decision {
            GateDecision::RequireResearch { categories } => {
                assert!(categories.contains(&SensitiveCategory::Security));
            }
            other => panic!("expected RequireResearch, got {other:?}"),
        }
    }

    #[test]
    fn sensitive_task_with_fresh_evidence_proceeds() {
        let evidence = evidence(1_000, 5_000);
        let decision = check_research_gate(
            "Upgrade the npm install dependency for the http client",
            std::slice::from_ref(&evidence),
            4_000,
        );
        assert_eq!(decision, GateDecision::Allow);
    }

    #[test]
    fn sensitive_task_with_only_stale_evidence_is_still_blocked() {
        let evidence = evidence(1_000, 5_000);
        let decision = check_research_gate(
            "Rotate the API key used for the external API integration",
            std::slice::from_ref(&evidence),
            10_000_000,
        );
        assert!(!decision.is_allowed());
    }
}
