//! Bounded policy layer for network-capable tools.
//!
//! This module makes a decision before an HTTP-capable tool performs I/O. It
//! intentionally contains no client and never resolves or fetches a URL. The
//! caller must still apply the same policy to redirects and perform runtime
//! SSRF checks immediately before connecting.

use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, net::IpAddr};

pub const MAX_ALLOWED_DOMAINS: usize = 64;
pub const MAX_DOMAIN_BYTES: usize = 253;
pub const MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_LATENCY_MS: u64 = 120_000;
pub const MAX_COST_MICROS: u64 = 1_000_000;
pub const MAX_REASON_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshPolicy {
    Never,
    IfStale { max_age_seconds: u64 },
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkCapabilityPolicy {
    pub allowed_domains: BTreeSet<String>,
    pub max_response_bytes: u64,
    pub max_latency_ms: u64,
    pub max_cost_micros: u64,
    pub allow_refresh: bool,
    pub refresh_policy: RefreshPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkRequest {
    pub url: String,
    pub expected_response_bytes: u64,
    pub expected_latency_ms: u64,
    pub estimated_cost_micros: u64,
    pub refresh: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Allow,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkDecision {
    pub kind: DecisionKind,
    pub reason: String,
}

impl NetworkDecision {
    fn allow() -> Self {
        Self {
            kind: DecisionKind::Allow,
            reason: "bounded_network_policy".to_owned(),
        }
    }

    fn deny(reason: &str) -> Self {
        Self {
            kind: DecisionKind::Deny,
            reason: reason.chars().take(MAX_REASON_BYTES).collect(),
        }
    }

    pub fn is_allowed(&self) -> bool {
        self.kind == DecisionKind::Allow
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    EmptyDomain,
    InvalidDomain,
    TooManyDomains,
    InvalidBudget,
    InvalidRefreshAge,
}

impl NetworkCapabilityPolicy {
    pub fn new(
        domains: impl IntoIterator<Item = String>,
        max_response_bytes: u64,
        max_latency_ms: u64,
        max_cost_micros: u64,
        allow_refresh: bool,
        refresh_policy: RefreshPolicy,
    ) -> Result<Self, PolicyError> {
        let mut allowed_domains = BTreeSet::new();
        for domain in domains {
            let normalized = normalize_domain(&domain)?;
            if !allowed_domains.insert(normalized) {
                continue;
            }
            if allowed_domains.len() > MAX_ALLOWED_DOMAINS {
                return Err(PolicyError::TooManyDomains);
            }
        }
        if max_response_bytes == 0
            || max_response_bytes > MAX_RESPONSE_BYTES
            || max_latency_ms == 0
            || max_latency_ms > MAX_LATENCY_MS
            || max_cost_micros > MAX_COST_MICROS
        {
            return Err(PolicyError::InvalidBudget);
        }
        if let RefreshPolicy::IfStale { max_age_seconds } = refresh_policy {
            if max_age_seconds == 0 {
                return Err(PolicyError::InvalidRefreshAge);
            }
        }
        if matches!(refresh_policy, RefreshPolicy::Always) && !allow_refresh {
            return Err(PolicyError::InvalidRefreshAge);
        }
        Ok(Self {
            allowed_domains,
            max_response_bytes,
            max_latency_ms,
            max_cost_micros,
            allow_refresh,
            refresh_policy,
        })
    }

    pub fn evaluate(&self, request: &NetworkRequest) -> NetworkDecision {
        if request.cancelled {
            return NetworkDecision::deny("cancelled");
        }
        if request.expected_response_bytes > self.max_response_bytes {
            return NetworkDecision::deny("response_budget_exceeded");
        }
        if request.expected_latency_ms > self.max_latency_ms {
            return NetworkDecision::deny("latency_budget_exceeded");
        }
        if request.estimated_cost_micros > self.max_cost_micros {
            return NetworkDecision::deny("cost_budget_exceeded");
        }
        if request.refresh && !self.allow_refresh {
            return NetworkDecision::deny("refresh_denied");
        }

        let Ok(url) = Url::parse(&request.url) else {
            return NetworkDecision::deny("invalid_url");
        };
        if url.scheme() != "https" {
            return NetworkDecision::deny("https_required");
        }
        if !url.username().is_empty() || url.password().is_some() {
            return NetworkDecision::deny("url_credentials_denied");
        }
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return NetworkDecision::deny("missing_host");
        };
        if is_ssrf_host(&host) {
            return NetworkDecision::deny("ssrf_target_denied");
        }
        if !self
            .allowed_domains
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        {
            return NetworkDecision::deny("domain_not_allowlisted");
        }
        NetworkDecision::allow()
    }
}

fn normalize_domain(domain: &str) -> Result<String, PolicyError> {
    let normalized = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if normalized.is_empty() {
        return Err(PolicyError::EmptyDomain);
    }
    if normalized.len() > MAX_DOMAIN_BYTES
        || normalized.starts_with('.')
        || normalized.contains('*')
        || normalized.chars().any(char::is_whitespace)
        || normalized.parse::<IpAddr>().is_ok()
        || normalized.split('.').any(|part| part.is_empty())
    {
        return Err(PolicyError::InvalidDomain);
    }
    Ok(normalized)
}

fn is_ssrf_host(host: &str) -> bool {
    if matches!(
        host,
        "localhost" | "localhost.localdomain" | "metadata.google.internal"
    ) {
        return true;
    }
    let Ok(ip) = host.parse::<IpAddr>() else {
        return false;
    };
    match ip {
        IpAddr::V4(ip) => {
            ip.is_loopback() || ip.is_private() || ip.is_link_local() || ip.is_unspecified()
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unique_local()
                || ip.is_unicast_link_local()
                || ip.is_unspecified()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> NetworkCapabilityPolicy {
        NetworkCapabilityPolicy::new(
            ["Example.COM".to_owned()],
            1024,
            5000,
            100,
            true,
            RefreshPolicy::IfStale {
                max_age_seconds: 60,
            },
        )
        .unwrap()
    }

    fn request(url: &str) -> NetworkRequest {
        NetworkRequest {
            url: url.to_owned(),
            expected_response_bytes: 100,
            expected_latency_ms: 100,
            estimated_cost_micros: 1,
            refresh: false,
            cancelled: false,
        }
    }

    #[test]
    fn allows_exact_and_subdomains_deterministically() {
        let p = policy();
        assert!(p.evaluate(&request("https://example.com/a")).is_allowed());
        assert!(p
            .evaluate(&request("https://api.example.com/a"))
            .is_allowed());
        assert_eq!(
            p.allowed_domains,
            BTreeSet::from(["example.com".to_owned()])
        );
    }

    #[test]
    fn denies_non_https_credentials_and_unlisted_domains() {
        let p = policy();
        assert_eq!(
            p.evaluate(&request("http://example.com")).reason,
            "https_required"
        );
        assert_eq!(
            p.evaluate(&request("https://user@example.com")).reason,
            "url_credentials_denied"
        );
        assert_eq!(
            p.evaluate(&request("https://other.example")).reason,
            "domain_not_allowlisted"
        );
    }

    #[test]
    fn denies_ssrf_targets_even_when_domain_is_allowlisted() {
        let p = NetworkCapabilityPolicy::new(
            ["localhost".to_owned()],
            1024,
            5000,
            100,
            false,
            RefreshPolicy::Never,
        )
        .unwrap();
        assert_eq!(
            p.evaluate(&request("https://localhost")).reason,
            "ssrf_target_denied"
        );
        let p = NetworkCapabilityPolicy::new(
            ["example.com".to_owned()],
            1024,
            5000,
            100,
            false,
            RefreshPolicy::Never,
        )
        .unwrap();
        assert_eq!(
            p.evaluate(&request("https://127.0.0.1")).reason,
            "ssrf_target_denied"
        );
    }

    #[test]
    fn enforces_response_latency_and_cost_budgets() {
        let p = policy();
        let mut r = request("https://example.com");
        r.expected_response_bytes = 1025;
        assert_eq!(p.evaluate(&r).reason, "response_budget_exceeded");
        r.expected_response_bytes = 100;
        r.expected_latency_ms = 5001;
        assert_eq!(p.evaluate(&r).reason, "latency_budget_exceeded");
        r.expected_latency_ms = 100;
        r.estimated_cost_micros = 101;
        assert_eq!(p.evaluate(&r).reason, "cost_budget_exceeded");
    }

    #[test]
    fn cancellation_and_refresh_are_explicit() {
        let p = policy();
        let mut r = request("https://example.com");
        r.cancelled = true;
        assert_eq!(p.evaluate(&r).reason, "cancelled");
        r.cancelled = false;
        r.refresh = true;
        assert!(p.evaluate(&r).is_allowed());
        let no_refresh = NetworkCapabilityPolicy::new(
            ["example.com".to_owned()],
            1024,
            5000,
            100,
            false,
            RefreshPolicy::Never,
        )
        .unwrap();
        assert_eq!(no_refresh.evaluate(&r).reason, "refresh_denied");
    }

    #[test]
    fn rejects_unbounded_policy_and_invalid_refresh() {
        assert!(matches!(
            NetworkCapabilityPolicy::new(
                ["example.com".to_owned()],
                MAX_RESPONSE_BYTES + 1,
                1,
                1,
                false,
                RefreshPolicy::Never,
            ),
            Err(PolicyError::InvalidBudget)
        ));
        assert!(matches!(
            NetworkCapabilityPolicy::new(
                ["example.com".to_owned()],
                1,
                1,
                1,
                true,
                RefreshPolicy::IfStale { max_age_seconds: 0 },
            ),
            Err(PolicyError::InvalidRefreshAge)
        ));
    }
}
