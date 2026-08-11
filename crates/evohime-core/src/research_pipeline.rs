//! Bounded policy and state machine for the offline research pipeline.
//!
//! This module does not perform HTTP, search, summarization, or persistence.
//! It authorizes a future capability call, validates its bounded result, and
//! keeps the pipeline transitions deterministic and cancellation-safe.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};

pub const MAX_DOMAIN_CHARS: usize = 253;
pub const MAX_ALLOWED_DOMAINS: usize = 64;
pub const MAX_REQUEST_ID_CHARS: usize = 128;
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;
pub const MAX_LATENCY_MS: u64 = 120_000;
pub const MAX_COST_MICROS: u64 = 10_000_000;
pub const MAX_CITATIONS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    EmptyField(&'static str),
    FieldTooLong {
        field: &'static str,
        max: usize,
    },
    InvalidDomain,
    TooManyDomains,
    NetworkDenied,
    DomainDenied,
    LimitExceeded(&'static str),
    InvalidTransition {
        from: PipelineState,
        to: PipelineState,
    },
    AlreadyTerminal,
    Cancelled,
    TooManyCitations,
    InvalidCitation,
    CitationSourceMismatch,
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::InvalidDomain => write!(f, "invalid domain"),
            Self::TooManyDomains => write!(f, "too many allowed domains"),
            Self::NetworkDenied => write!(f, "network capability is denied"),
            Self::DomainDenied => write!(f, "domain is not in the allowlist"),
            Self::LimitExceeded(name) => write!(f, "{name} exceeds policy limit"),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid transition {from:?} -> {to:?}")
            }
            Self::AlreadyTerminal => write!(f, "pipeline is already terminal"),
            Self::Cancelled => write!(f, "pipeline was cancelled"),
            Self::TooManyCitations => write!(f, "too many citations"),
            Self::InvalidCitation => write!(f, "invalid citation"),
            Self::CitationSourceMismatch => {
                write!(f, "citation source does not match request domain")
            }
        }
    }
}

impl std::error::Error for PipelineError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineState {
    Planned,
    Fetching,
    Extracting,
    Summarizing,
    Completed,
    Cancelled,
    Failed,
}

impl PipelineState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchPolicy {
    pub network_allowed: bool,
    pub allowed_domains: Vec<String>,
    pub max_bytes: u64,
    pub max_latency_ms: u64,
    pub max_cost_micros: u64,
}

impl ResearchPolicy {
    pub fn validate(&self) -> Result<(), PipelineError> {
        if self.allowed_domains.len() > MAX_ALLOWED_DOMAINS {
            return Err(PipelineError::TooManyDomains);
        }
        if self.max_bytes == 0 || self.max_bytes > MAX_BYTES {
            return Err(PipelineError::LimitExceeded("max_bytes"));
        }
        if self.max_latency_ms == 0 || self.max_latency_ms > MAX_LATENCY_MS {
            return Err(PipelineError::LimitExceeded("max_latency_ms"));
        }
        if self.max_cost_micros > MAX_COST_MICROS {
            return Err(PipelineError::LimitExceeded("max_cost_micros"));
        }
        let mut domains = BTreeSet::new();
        for domain in &self.allowed_domains {
            validate_domain(domain)?;
            if !domains.insert(domain.to_ascii_lowercase()) {
                return Err(PipelineError::InvalidDomain);
            }
        }
        Ok(())
    }

    pub fn permits(
        &self,
        domain: &str,
        bytes: u64,
        latency_ms: u64,
        cost_micros: u64,
    ) -> Result<(), PipelineError> {
        self.validate()?;
        if !self.network_allowed {
            return Err(PipelineError::NetworkDenied);
        }
        validate_domain(domain)?;
        let domain = domain.to_ascii_lowercase();
        if !self.allowed_domains.iter().any(|allowed| {
            let allowed = allowed.to_ascii_lowercase();
            domain == allowed || domain.ends_with(&format!(".{allowed}"))
        }) {
            return Err(PipelineError::DomainDenied);
        }
        if bytes == 0 || bytes > self.max_bytes {
            return Err(PipelineError::LimitExceeded("bytes"));
        }
        if latency_ms > self.max_latency_ms {
            return Err(PipelineError::LimitExceeded("latency_ms"));
        }
        if cost_micros > self.max_cost_micros {
            return Err(PipelineError::LimitExceeded("cost_micros"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResearchRequest {
    pub request_id: String,
    pub domain: String,
    pub max_bytes: u64,
    pub max_latency_ms: u64,
    pub max_cost_micros: u64,
    pub cancelled: bool,
}

impl ResearchRequest {
    pub fn validate(&self, policy: &ResearchPolicy) -> Result<(), PipelineError> {
        validate_text("request_id", &self.request_id, MAX_REQUEST_ID_CHARS)?;
        policy.permits(
            &self.domain,
            self.max_bytes,
            self.max_latency_ms,
            self.max_cost_micros,
        )?;
        if self.cancelled {
            return Err(PipelineError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub url: String,
    pub source_hash: String,
    pub excerpt_hash: String,
}

pub fn validate_citations(domain: &str, citations: &[Citation]) -> Result<(), PipelineError> {
    validate_domain(domain)?;
    if citations.len() > MAX_CITATIONS {
        return Err(PipelineError::TooManyCitations);
    }
    for citation in citations {
        let citation_domain = url_domain(&citation.url).ok_or(PipelineError::InvalidCitation)?;
        if citation_domain != domain.to_ascii_lowercase()
            && !citation_domain.ends_with(&format!(".{domain}"))
        {
            return Err(PipelineError::CitationSourceMismatch);
        }
        if !is_hash(&citation.source_hash) || !is_hash(&citation.excerpt_hash) {
            return Err(PipelineError::InvalidCitation);
        }
    }
    Ok(())
}

pub fn transition(from: PipelineState, to: PipelineState) -> Result<PipelineState, PipelineError> {
    if from.is_terminal() {
        return Err(PipelineError::AlreadyTerminal);
    }
    let valid = matches!(
        (from, to),
        (PipelineState::Planned, PipelineState::Fetching)
            | (PipelineState::Fetching, PipelineState::Extracting)
            | (PipelineState::Extracting, PipelineState::Summarizing)
            | (PipelineState::Summarizing, PipelineState::Completed)
            | (_, PipelineState::Cancelled)
            | (_, PipelineState::Failed)
    );
    if valid {
        Ok(to)
    } else {
        Err(PipelineError::InvalidTransition { from, to })
    }
}

fn validate_text(field: &'static str, value: &str, max: usize) -> Result<(), PipelineError> {
    if value.trim().is_empty() {
        return Err(PipelineError::EmptyField(field));
    }
    if value.chars().count() > max {
        return Err(PipelineError::FieldTooLong { field, max });
    }
    Ok(())
}

fn validate_domain(domain: &str) -> Result<(), PipelineError> {
    if domain.len() > MAX_DOMAIN_CHARS || domain.is_empty() || domain != domain.trim() {
        return Err(PipelineError::InvalidDomain);
    }
    if domain.contains('/')
        || domain.contains(':')
        || domain.contains('@')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(PipelineError::InvalidDomain);
    }
    if domain
        .split('.')
        .any(|part| part.is_empty() || part.starts_with('-') || part.ends_with('-'))
    {
        return Err(PipelineError::InvalidDomain);
    }
    Ok(())
}

fn url_domain(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = rest.split(['/', '?', '#']).next()?;
    if host.contains('@') || host.contains(':') || host.is_empty() {
        return None;
    }
    validate_domain(host).ok()?;
    Some(host.to_ascii_lowercase())
}

fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ResearchPolicy {
        ResearchPolicy {
            network_allowed: true,
            allowed_domains: vec!["example.com".into()],
            max_bytes: 1024,
            max_latency_ms: 500,
            max_cost_micros: 100,
        }
    }

    #[test]
    fn permits_only_bounded_allowlisted_requests() {
        let policy = policy();
        assert!(policy.permits("docs.example.com", 512, 400, 50).is_ok());
        assert_eq!(
            policy.permits("evil.example.net", 1, 1, 1),
            Err(PipelineError::DomainDenied)
        );
        assert_eq!(
            policy.permits("example.com", 2048, 1, 1),
            Err(PipelineError::LimitExceeded("bytes"))
        );
    }

    #[test]
    fn network_and_cancellation_are_explicit() {
        let mut denied = policy();
        denied.network_allowed = false;
        assert_eq!(
            denied.permits("example.com", 1, 1, 1),
            Err(PipelineError::NetworkDenied)
        );
        let request = ResearchRequest {
            request_id: "r1".into(),
            domain: "example.com".into(),
            max_bytes: 1,
            max_latency_ms: 1,
            max_cost_micros: 1,
            cancelled: true,
        };
        assert_eq!(request.validate(&policy()), Err(PipelineError::Cancelled));
    }

    #[test]
    fn transitions_are_deterministic_and_terminal_safe() {
        assert_eq!(
            transition(PipelineState::Planned, PipelineState::Fetching),
            Ok(PipelineState::Fetching)
        );
        assert_eq!(
            transition(PipelineState::Fetching, PipelineState::Planned),
            Err(PipelineError::InvalidTransition {
                from: PipelineState::Fetching,
                to: PipelineState::Planned
            })
        );
        assert_eq!(
            transition(PipelineState::Completed, PipelineState::Failed),
            Err(PipelineError::AlreadyTerminal)
        );
    }

    #[test]
    fn citations_require_matching_https_source_and_hashes() {
        let hash = "a".repeat(64);
        let citation = Citation {
            url: "https://docs.example.com/page".into(),
            source_hash: hash.clone(),
            excerpt_hash: hash,
        };
        assert!(validate_citations("example.com", &[citation]).is_ok());
        let bad = Citation {
            url: "https://evil.example.net/page".into(),
            source_hash: "x".repeat(64),
            excerpt_hash: "x".repeat(64),
        };
        assert_eq!(
            validate_citations("example.com", &[bad]),
            Err(PipelineError::CitationSourceMismatch)
        );
    }

    #[test]
    fn serializes_deterministically_without_runtime_side_effects() {
        let request = ResearchRequest {
            request_id: "r1".into(),
            domain: "example.com".into(),
            max_bytes: 1,
            max_latency_ms: 2,
            max_cost_micros: 3,
            cancelled: false,
        };
        let first = serde_json::to_string(&request).unwrap();
        assert_eq!(first, serde_json::to_string(&request).unwrap());
        assert!(!first.contains("http"));
    }
}
