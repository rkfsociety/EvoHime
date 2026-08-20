//! Deterministic, bounded model-route selection.
//!
//! This module deliberately contains provider metadata only. Credentials,
//! authorization headers, URLs containing credentials, and prompt contents are
//! outside the routing policy contract.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeSet;

pub const MAX_CANDIDATES: usize = 64;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_NAME_BYTES: usize = 128;
pub const MAX_REASON_BYTES: usize = 96;
pub const MAX_FALLBACKS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Public,
    Internal,
    Sensitive,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteCandidate {
    /// Stable route identifier; this is not a provider credential or URL.
    pub route_id: String,
    pub model: String,
    pub capabilities: Vec<String>,
    pub cost_micros_per_1k_tokens: u64,
    pub p95_latency_ms: u32,
    pub privacy: PrivacyClass,
    pub available: bool,
    /// Lower values are preferred when all requested policy dimensions tie.
    pub fallback_rank: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingRequest {
    pub required_capabilities: Vec<String>,
    pub max_cost_micros_per_1k_tokens: Option<u64>,
    pub max_latency_ms: Option<u32>,
    pub required_privacy: PrivacyClass,
    pub allow_fallback: bool,
    /// Unprivileged user hint; policy filters always take precedence.
    #[serde(default)]
    pub preferred_route: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    Selected,
    Fallback,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingDecision {
    pub kind: DecisionKind,
    pub selected_route: Option<String>,
    pub selected_model: Option<String>,
    pub fallback_chain: Vec<String>,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingPolicyError {
    TooManyCandidates,
    TooManyCapabilities,
    InvalidName,
    DuplicateCapability,
    SecretLikeField,
    EmptyRouteSet,
}

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
