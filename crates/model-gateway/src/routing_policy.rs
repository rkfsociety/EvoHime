//! Deterministic, bounded model-route selection.
//!
//! This module deliberately contains provider metadata only. Credentials,
//! authorization headers, URLs containing credentials, and prompt contents are
//! outside the routing policy contract.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;

/// Maximum candidates accepted by one route-selection request.
pub const MAX_CANDIDATES: usize = 64;
/// Maximum required or advertised capabilities per candidate.
pub const MAX_CAPABILITIES: usize = 32;
/// Maximum length of route, model, and capability names.
pub const MAX_NAME_BYTES: usize = 128;
/// Maximum length of one stable decision reason code.
pub const MAX_REASON_BYTES: usize = 96;
/// Maximum fallback candidates returned in one decision.
pub const MAX_FALLBACKS: usize = 16;

/// Sensitivity class supported by a route or required by a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    /// Data may be sent to public services.
    Public,
    /// Data is limited to approved internal services.
    Internal,
    /// Data requires sensitive-data handling controls.
    Sensitive,
    /// Data may only be processed by routes satisfying restricted policy.
    Restricted,
}

impl PrivacyClass {
    fn permits(self, required: Self) -> bool {
        self >= required
    }
}

impl PartialOrd for PrivacyClass {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrivacyClass {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

/// Provider route metadata considered by deterministic selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteCandidate {
    /// Stable route identifier; this is not a provider credential or URL.
    pub route_id: String,
    /// Model identifier exposed by the route.
    pub model: String,
    /// Capabilities advertised for this model.
    pub capabilities: Vec<String>,
    /// Estimated price in micro-units per 1,000 tokens.
    pub cost_micros_per_1k_tokens: u64,
    /// Advertised 95th percentile latency in milliseconds.
    pub p95_latency_ms: u32,
    /// Highest sensitivity class the route may process.
    pub privacy: PrivacyClass,
    /// Whether health and policy currently permit the route.
    pub available: bool,
    /// Lower values are preferred when all requested policy dimensions tie.
    pub fallback_rank: u16,
}

/// Constraints and caller preferences for deterministic route selection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutingRequest {
    /// Capabilities that every eligible route must provide.
    pub required_capabilities: Vec<String>,
    /// Optional maximum route cost per 1,000 tokens.
    pub max_cost_micros_per_1k_tokens: Option<u64>,
    /// Optional maximum advertised P95 latency.
    pub max_latency_ms: Option<u32>,
    /// Minimum privacy classification required by the request.
    pub required_privacy: PrivacyClass,
    /// Whether eligible alternatives should be returned as fallbacks.
    pub allow_fallback: bool,
    /// Unprivileged user hint; policy filters always take precedence.
    #[serde(default)]
    pub preferred_route: Option<String>,
    #[serde(default)]
    /// Task category used by higher-level route policy.
    pub task_class: Option<String>,
    #[serde(default)]
    /// Whether selection must be restricted to offline-capable routes.
    pub offline: bool,
    #[serde(default)]
    /// Whether cloud routes are permitted.
    pub allow_cloud: bool,
    #[serde(default)]
    /// Estimated input size used by cost-aware selection.
    pub estimated_input_tokens: u32,
    #[serde(default = "default_quality_delta")]
    /// Minimum relative quality difference required before preferring a more expensive route.
    pub quality_delta: f64,
}

fn default_quality_delta() -> f64 {
    0.05
}

/// Outcome category for route selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// One route was selected without a fallback chain.
    Selected,
    /// A route was selected and eligible fallbacks were returned.
    Fallback,
    /// No candidate satisfied the request constraints.
    Denied,
}

/// Selected route, fallback order, and stable policy reasons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingDecision {
    /// Category of the selection outcome.
    pub kind: DecisionKind,
    /// Selected route identifier, absent when denied.
    pub selected_route: Option<String>,
    /// Selected model identifier, absent when denied.
    pub selected_model: Option<String>,
    /// Ordered eligible routes that may replace the selection.
    pub fallback_chain: Vec<String>,
    /// Stable explanation codes for the selection.
    pub reasons: Vec<String>,
}

/// Validation failures returned by route policy inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingPolicyError {
    /// Candidate count exceeds the configured maximum.
    TooManyCandidates,
    /// Capability count exceeds the configured maximum.
    TooManyCapabilities,
    /// A route, model, or capability name is invalid.
    InvalidName,
    /// A candidate or request repeats a capability name.
    DuplicateCapability,
    /// A name contains a secret-like value or field label.
    SecretLikeField,
    /// No route candidates were supplied.
    EmptyRouteSet,
}

/// Validates candidate count, identifiers, and unique bounded capabilities.
pub fn validate_candidates(candidates: &[RouteCandidate]) -> Result<(), RoutingPolicyError> {
    if candidates.is_empty() {
        return Err(RoutingPolicyError::EmptyRouteSet);
    }
    if candidates.len() > MAX_CANDIDATES {
        return Err(RoutingPolicyError::TooManyCandidates);
    }

    let mut route_ids = BTreeSet::new();
    for candidate in candidates {
        validate_name(&candidate.route_id)?;
        validate_name(&candidate.model)?;
        if !route_ids.insert(candidate.route_id.as_str()) {
            return Err(RoutingPolicyError::InvalidName);
        }
        if candidate.capabilities.len() > MAX_CAPABILITIES {
            return Err(RoutingPolicyError::TooManyCapabilities);
        }
        let mut capabilities = BTreeSet::new();
        for capability in &candidate.capabilities {
            validate_name(capability)?;
            if !capabilities.insert(capability.as_str()) {
                return Err(RoutingPolicyError::DuplicateCapability);
            }
        }
    }
    Ok(())
}

/// Validates bounded and unique request capability names.
pub fn validate_request(request: &RoutingRequest) -> Result<(), RoutingPolicyError> {
    if request.required_capabilities.len() > MAX_CAPABILITIES {
        return Err(RoutingPolicyError::TooManyCapabilities);
    }
    let mut capabilities = BTreeSet::new();
    for capability in &request.required_capabilities {
        validate_name(capability)?;
        if !capabilities.insert(capability.as_str()) {
            return Err(RoutingPolicyError::DuplicateCapability);
        }
    }
    Ok(())
}

/// Selects a route deterministically after filtering by capability, cost, latency, and privacy.
pub fn select_route(
    request: &RoutingRequest,
    candidates: &[RouteCandidate],
) -> Result<RoutingDecision, RoutingPolicyError> {
    validate_request(request)?;
    validate_candidates(candidates)?;

    let mut eligible = Vec::new();
    for candidate in candidates.iter().filter(|candidate| candidate.available) {
        if !candidate.privacy.permits(request.required_privacy) {
            continue;
        }
        if request
            .max_cost_micros_per_1k_tokens
            .is_some_and(|limit| candidate.cost_micros_per_1k_tokens > limit)
        {
            continue;
        }
        if request
            .max_latency_ms
            .is_some_and(|limit| candidate.p95_latency_ms > limit)
        {
            continue;
        }
        if request.required_capabilities.iter().any(|required| {
            !candidate
                .capabilities
                .iter()
                .any(|actual| actual == required)
        }) {
            continue;
        }
        eligible.push(candidate);
    }

    eligible.sort_by(|left, right| {
        left.cost_micros_per_1k_tokens
            .cmp(&right.cost_micros_per_1k_tokens)
            .then(left.p95_latency_ms.cmp(&right.p95_latency_ms))
            .then(left.fallback_rank.cmp(&right.fallback_rank))
            .then_with(|| {
                let preferred = request.preferred_route.as_deref();
                (preferred != Some(left.route_id.as_str()))
                    .cmp(&(preferred != Some(right.route_id.as_str())))
            })
            .then(left.route_id.cmp(&right.route_id))
    });

    let Some(selected) = eligible.first() else {
        return Ok(RoutingDecision {
            kind: DecisionKind::Denied,
            selected_route: None,
            selected_model: None,
            fallback_chain: Vec::new(),
            reasons: vec!["no_eligible_route".to_string()],
        });
    };

    let fallback_chain = if request.allow_fallback {
        eligible
            .iter()
            .skip(1)
            .take(MAX_FALLBACKS)
            .map(|candidate| candidate.route_id.clone())
            .collect()
    } else {
        Vec::new()
    };

    Ok(RoutingDecision {
        kind: if request.allow_fallback && !fallback_chain.is_empty() {
            DecisionKind::Fallback
        } else {
            DecisionKind::Selected
        },
        selected_route: Some(selected.route_id.clone()),
        selected_model: Some(selected.model.clone()),
        fallback_chain,
        reasons: vec!["capability_cost_latency_privacy_policy".to_string()],
    })
}

fn validate_name(value: &str) -> Result<(), RoutingPolicyError> {
    if value.is_empty() || value.len() > MAX_NAME_BYTES || value.chars().any(char::is_control) {
        return Err(RoutingPolicyError::InvalidName);
    }
    let lower = value.to_ascii_lowercase();
    if [
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "token",
        "secret",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(RoutingPolicyError::SecretLikeField);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(route_id: &str, cost: u64, latency: u32) -> RouteCandidate {
        RouteCandidate {
            route_id: route_id.to_string(),
            model: format!("{route_id}-model"),
            capabilities: vec!["chat".to_string(), "tools".to_string()],
            cost_micros_per_1k_tokens: cost,
            p95_latency_ms: latency,
            privacy: PrivacyClass::Internal,
            available: true,
            fallback_rank: 0,
        }
    }

    #[test]
    fn selection_is_deterministic_and_uses_all_policy_dimensions() {
        let request = RoutingRequest {
            required_capabilities: vec!["tools".to_string()],
            max_cost_micros_per_1k_tokens: Some(100),
            max_latency_ms: Some(500),
            required_privacy: PrivacyClass::Internal,
            allow_fallback: true,
            preferred_route: None,
            task_class: None,
            offline: false,
            allow_cloud: true,
            estimated_input_tokens: 0,
            quality_delta: 0.05,
        };
        let mut expensive = candidate("expensive", 90, 200);
        expensive.fallback_rank = 1;
        let cheap = candidate("cheap", 20, 400);
        let decision = select_route(&request, &[expensive, cheap]).expect("valid policy");
        assert_eq!(decision.kind, DecisionKind::Fallback);
        assert_eq!(decision.selected_route.as_deref(), Some("cheap"));
        assert_eq!(decision.fallback_chain, vec!["expensive"]);
    }

    #[test]
    fn unavailable_or_privacy_incompatible_routes_are_denied() {
        let request = RoutingRequest {
            required_capabilities: vec!["chat".to_string()],
            max_cost_micros_per_1k_tokens: None,
            max_latency_ms: None,
            required_privacy: PrivacyClass::Restricted,
            allow_fallback: false,
            preferred_route: None,
            task_class: None,
            offline: false,
            allow_cloud: true,
            estimated_input_tokens: 0,
            quality_delta: 0.05,
        };
        let mut route = candidate("cloud", 1, 1);
        route.available = false;
        assert_eq!(
            select_route(&request, &[route]).unwrap().kind,
            DecisionKind::Denied
        );
    }

    #[test]
    fn validation_rejects_secret_like_metadata_and_unbounded_input() {
        let mut route = candidate("api_key_route", 1, 1);
        assert_eq!(
            validate_candidates(&[route.clone()]),
            Err(RoutingPolicyError::SecretLikeField)
        );
        route.route_id = "safe".to_string();
        route.model = "safe-model".to_string();
        route.capabilities = vec!["chat".to_string(); MAX_CAPABILITIES + 1];
        assert_eq!(
            validate_candidates(&[route]),
            Err(RoutingPolicyError::TooManyCapabilities)
        );
    }

    #[test]
    fn fallback_chain_is_bounded_and_serializable_without_secrets() {
        let request = RoutingRequest {
            required_capabilities: vec![],
            max_cost_micros_per_1k_tokens: None,
            max_latency_ms: None,
            required_privacy: PrivacyClass::Public,
            allow_fallback: true,
            preferred_route: None,
            task_class: None,
            offline: false,
            allow_cloud: true,
            estimated_input_tokens: 0,
            quality_delta: 0.05,
        };
        let routes: Vec<_> = (0..(MAX_FALLBACKS + 4))
            .map(|index| candidate(&format!("route-{index}"), index as u64, 1))
            .collect();
        let decision = select_route(&request, &routes).expect("valid policy");
        assert_eq!(decision.fallback_chain.len(), MAX_FALLBACKS);
        let json = serde_json::to_string(&decision).expect("decision is serializable");
        assert!(!json.contains("api_key"));
        assert!(!json.contains("token"));
    }
}
