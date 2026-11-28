#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![warn(missing_docs)]

//! Isolated `ambient` service module implementing `evohime-service-contract`.

pub mod ambient_proactivity;
pub mod autonomous_metric_experiment_runtime;
pub mod temporal_signal_intelligence;

use std::sync::atomic::{AtomicU64, Ordering};

pub use evohime_listener_contract::{
    ProactivityDecideRequest, ProactivityDecideResponse, ProactivityRejectionPayload,
};
use evohime_service_contract::{
    ServiceCapabilityDescriptor, ServiceContractError, ServiceDescriptor, ServiceHandler,
    ServiceHealthReport, ServiceHealthStatus, SERVICE_CONTRACT_VERSION,
};

use crate::ambient_proactivity::ProactiveEffect;
#[cfg(test)]
use evohime_listener_contract::{AmbientPolicy, ProactivityBudget};

fn parse_proactive_effect(value: &str) -> Result<ProactiveEffect, ServiceContractError> {
    ProactiveEffect::parse(value).ok_or_else(|| {
        ServiceContractError::invalid_input(format!("unknown proactive effect: {value}"))
    })
}

/// Service handler for background proactivity, temporal signals, and autonomous experiments.
#[derive(Debug, Default)]
pub struct AmbientService {
    experiments_started: AtomicU64,
}

impl AmbientService {
    /// Creates a new [`AmbientService`] instance.
    pub fn new() -> Self {
        Self {
            experiments_started: AtomicU64::new(0),
        }
    }
}

impl ServiceHandler for AmbientService {
    fn descriptor(&self) -> ServiceDescriptor {
        ServiceDescriptor {
            service_id: "ambient".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            contract_version: SERVICE_CONTRACT_VERSION,
            capabilities: vec![
                ServiceCapabilityDescriptor {
                    name: "ambient.status".to_owned(),
                    description: "Returns current background proactivity state.".to_owned(),
                    input_schema: serde_json::json!({ "type": "object", "properties": {} }),
                    required_permissions: vec!["ambient.read".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "ambient.run_experiment".to_owned(),
                    description: "Starts a bounded autonomous metric experiment.".to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["experiment_id"],
                        "properties": { "experiment_id": { "type": "string" } }
                    }),
                    required_permissions: vec!["ambient.execute".to_owned()],
                    supports_streaming: false,
                },
                ServiceCapabilityDescriptor {
                    name: "ambient.proactivity_decide".to_owned(),
                    description: "Decides whether one bounded proactive proposal is allowed now."
                        .to_owned(),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["effect", "muted", "policy", "budget", "counters", "now_ms", "minute_of_day"]
                    }),
                    required_permissions: vec!["ambient.execute".to_owned()],
                    supports_streaming: false,
                },
            ],
        }
    }

    fn health(&self) -> ServiceHealthReport {
        ServiceHealthReport {
            service_id: "ambient".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            status: ServiceHealthStatus::Ready,
            active_requests: 0,
            detail: None,
        }
    }

    fn invoke(
        &self,
        capability: &str,
        payload: serde_json::Value,
        _caller_session_id: Option<&str>,
    ) -> Result<serde_json::Value, ServiceContractError> {
        match capability {
            "ambient.status" => Ok(serde_json::json!({
                "proactivity": "idle",
                "experiments_started": self.experiments_started.load(Ordering::Relaxed)
            })),
            "ambient.run_experiment" => {
                let experiment_id = payload
                    .get("experiment_id")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        ServiceContractError::invalid_input(
                            "experiment_id must be a non-empty string",
                        )
                    })?;
                let count = self.experiments_started.fetch_add(1, Ordering::Relaxed) + 1;
                Ok(serde_json::json!({
                    "experiment_id": experiment_id,
                    "run_number": count,
                    "status": "accepted"
                }))
            }
            "ambient.proactivity_decide" => {
                let request: ProactivityDecideRequest =
                    serde_json::from_value(payload).map_err(|err| {
                        ServiceContractError::invalid_input(format!(
                            "invalid proactivity decide request: {err}"
                        ))
                    })?;
                let effect = parse_proactive_effect(&request.effect)?;
                match ambient_proactivity::decide_proposal(
                    effect,
                    request.muted,
                    &request.policy,
                    &request.budget,
                    request.counters,
                    request.now_ms,
                    request.minute_of_day,
                ) {
                    Ok(authorized) => Ok(serde_json::json!({
                        "authorized": true,
                        "effect": authorized.effect().as_str(),
                        "rejection": serde_json::Value::Null,
                    })),
                    Err(rejection) => {
                        let rejection = ProactivityRejectionPayload::from_rejection(rejection)
                            .ok_or_else(|| {
                                ServiceContractError::unavailable(
                                    "proactivity decision is unavailable",
                                )
                            })?;
                        Ok(serde_json::json!({
                            "authorized": false,
                            "effect": serde_json::Value::Null,
                            "rejection": rejection,
                        }))
                    }
                }
            }
            other => Err(ServiceContractError::not_found(format!(
                "unknown ambient capability: {other}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_listener_contract::ProactivityDenial;
    use evohime_service_contract::{
        dispatch_request, validate_descriptor, ServiceRequest, ServiceResponse,
    };

    use crate::ambient_proactivity::RollingCounters;

    fn decide_payload(effect: &str, counters: RollingCounters) -> serde_json::Value {
        serde_json::to_value(ProactivityDecideRequest {
            effect: effect.to_owned(),
            muted: false,
            policy: AmbientPolicy {
                paused: false,
                ..AmbientPolicy::default()
            },
            budget: ProactivityBudget::DEFAULT,
            counters,
            now_ms: 1_770_000_000_000,
            minute_of_day: 12 * 60,
        })
        .expect("decide payload serializes")
    }

    #[test]
    fn proactivity_decide_authorizes_the_first_card() {
        let svc = AmbientService::new();
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "d1".to_owned(),
                capability: "ambient.proactivity_decide".to_owned(),
                payload: decide_payload("proposal_card", RollingCounters::default()),
                caller_session_id: None,
            },
        );
        let ServiceResponse::InvokeResult { payload, .. } = resp else {
            panic!("invoke result expected");
        };
        let parsed: ProactivityDecideResponse =
            serde_json::from_value(payload).expect("typed response");
        assert!(parsed.authorized);
        assert_eq!(parsed.effect.as_deref(), Some("proposal_card"));
        assert!(parsed.rejection.is_none());
    }

    #[test]
    fn proactivity_decide_reports_a_typed_hourly_denial() {
        let svc = AmbientService::new();
        let counters = RollingCounters {
            hour_started_at_ms: 1_770_000_000_000,
            hour_count: 3,
            day_started_at_ms: 1_770_000_000_000,
            day_count: 3,
            last_proposed_at_ms: None,
        };
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "d2".to_owned(),
                capability: "ambient.proactivity_decide".to_owned(),
                payload: decide_payload("pending_reminder", counters),
                caller_session_id: None,
            },
        );
        let ServiceResponse::InvokeResult { payload, .. } = resp else {
            panic!("invoke result expected");
        };
        let parsed: ProactivityDecideResponse =
            serde_json::from_value(payload).expect("typed response");
        assert!(!parsed.authorized);
        assert_eq!(
            parsed.rejection,
            Some(ProactivityRejectionPayload::Denied {
                denial: ProactivityDenial::HourlyCapReached
            })
        );
    }

    #[test]
    fn proactivity_decide_rejects_an_unknown_effect() {
        let svc = AmbientService::new();
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "d3".to_owned(),
                capability: "ambient.proactivity_decide".to_owned(),
                payload: decide_payload("shell_command", RollingCounters::default()),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::Error { .. }));
    }

    #[test]
    fn descriptor_and_dispatch_work() {
        let svc = AmbientService::new();
        validate_descriptor(&svc.descriptor()).expect("valid descriptor");
        let resp = dispatch_request(
            &svc,
            ServiceRequest::Invoke {
                request_id: "a1".to_owned(),
                capability: "ambient.status".to_owned(),
                payload: serde_json::json!({}),
                caller_session_id: None,
            },
        );
        assert!(matches!(resp, ServiceResponse::InvokeResult { .. }));
    }
}
