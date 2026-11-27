//! Runtime boundary for the Core-owned outbound A2A backend.
//!
//! The runtime deliberately owns only protocol lifecycle state. Scheduling,
//! credentials, artifacts and workflow continuation remain owned by their
//! existing Core subsystems. Network input is treated as untrusted and is
//! reduced to the bounded card snapshot defined in [`super::external_agent_a2a`].

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::net::ToSocketAddrs;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::external_agent_a2a::{
    apply_transition, build_binding, build_capsule, build_delegation, card_snapshot,
    reconcile_known_task, validate_endpoint, A2aError, ContextCapsule, DelegationRequest,
    RemoteAgentCardSnapshot, RemoteAgentProfile, RemoteAgentSkill, RemoteTaskBinding,
    RemoteTaskState, SCHEMA_VERSION,
};
use crate::external_coding_agent_adapter::A2A_PROTOCOL_VERSION;
use evohime_model_gateway::PrivacyClass;

/// Maximum bytes accepted from a discovery/card response.
pub const MAX_CARD_RESPONSE_BYTES: usize = 128 * 1024;
/// Maximum bytes accepted for one inline artifact.
pub const MAX_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;
/// Maximum number of artifacts in one completion.
pub const MAX_ARTIFACTS: usize = 64;

/// A validated, metadata-only artifact returned by a remote task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRemoteArtifact {
    /// Stable artifact identifier supplied by the remote task.
    pub id: String,
    /// Safe MIME label.
    pub mime: String,
    /// Byte length of the inline content.
    pub bytes: usize,
    /// SHA-256 digest of the inline content.
    pub content_hash: String,
}

/// Result of one bounded transport operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportOutcome {
    /// The remote endpoint accepted the request and returned a stable id.
    Accepted {
        /// Stable task identifier assigned by the remote endpoint.
        remote_task_id: String,
    },
    /// The request may have been accepted but no safe response was received.
    UnknownOutcome,
    /// The endpoint was unavailable without creating an ambiguous submit.
    Unavailable,
}

/// In-memory runtime registry used by Core IPC and deterministic integration
/// tests. Durable metadata is written by the caller through the A2A store.
#[derive(Debug, Default)]
pub struct A2aRuntimeRegistry {
    /// Explicitly registered profile revisions.
    pub profiles: BTreeMap<String, RemoteAgentProfile>,
    /// Latest card snapshot per profile.
    pub cards: BTreeMap<String, RemoteAgentCardSnapshot>,
    /// Active and historical task bindings.
    pub bindings: BTreeMap<String, RemoteTaskBinding>,
}

#[derive(Debug, Deserialize)]
struct CardWire {
    #[serde(default)]
    protocol_version: u32,
    #[serde(default)]
    skills: Vec<RemoteAgentSkill>,
    #[serde(default)]
    schema_hashes: Vec<String>,
    #[serde(default)]
    supports_streaming: bool,
    #[serde(default)]
    auth_schemes: Vec<String>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
/// Runtime-specific failures that are not part of the persisted contract.
pub enum A2aRuntimeError {
    /// Contract validation failed.
    #[error("a2a contract error: {0}")]
    Contract(#[from] A2aError),
    /// Card response exceeded its bounded body budget.
    #[error("a2a card response exceeds the bounded body budget")]
    CardTooLarge,
    /// Card response is not a valid bounded JSON card.
    #[error("a2a card response is invalid")]
    InvalidCard,
    /// A public profile resolved to a private or special-use address.
    #[error("a2a endpoint resolved to a forbidden address")]
    ResolvedAddressDenied,
    /// Redirects are not enabled by the profile-owned transport policy.
    #[error("a2a redirects are disabled")]
    RedirectDenied,
    /// Artifact payload violates the remote task budget or content policy.
    #[error("a2a artifact is invalid")]
    InvalidArtifact,
    /// The bounded outbound transport failed closed.
    #[error("a2a transport failed: {0}")]
    Transport(String),
}

/// Minimal outbound HTTP transport. Redirects are disabled and response
/// bodies are bounded before they are parsed. Authentication is intentionally
/// not implemented here: the supervisor/secret owner supplies it at the
/// existing external-agent boundary, never through this runtime API.
#[derive(Debug, Clone)]
pub struct A2aHttpTransport {
    client: Option<reqwest::Client>,
}

impl Default for A2aHttpTransport {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .ok();
        Self { client }
    }
}

impl A2aHttpTransport {
    async fn request_bytes(
        &self,
        profile: &RemoteAgentProfile,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<Vec<u8>, A2aRuntimeError> {
        validate_endpoint(&profile.endpoint)?;
        let base = reqwest::Url::parse(&profile.endpoint.origin)
            .map_err(|_| A2aRuntimeError::Transport("invalid_origin".into()))?;
        let host = base
            .host_str()
            .ok_or_else(|| A2aRuntimeError::Transport("missing_host".into()))?;
        let port = base
            .port_or_known_default()
            .ok_or_else(|| A2aRuntimeError::Transport("missing_port".into()))?;
        let addresses = (host, port)
            .to_socket_addrs()
            .map_err(|error| A2aRuntimeError::Transport(error.to_string()))?;
        let mut resolved = false;
        for address in addresses {
            resolved = true;
            validate_resolved_address(profile, address.ip())?;
        }
        if !resolved {
            return Err(A2aRuntimeError::ResolvedAddressDenied);
        }
        let url = base
            .join(path)
            .map_err(|_| A2aRuntimeError::Transport("invalid_path".into()))?;
        let client = self
            .client
            .as_ref()
            .ok_or_else(|| A2aRuntimeError::Transport("client_unavailable".into()))?;
        let request = client.request(method, url);
        let request = if let Some(body) = body {
            request.json(&body)
        } else {
            request
        };
        let response = request
            .send()
            .await
            .map_err(|error| A2aRuntimeError::Transport(error.to_string()))?;
        if response.status().is_redirection() {
            return Err(A2aRuntimeError::RedirectDenied);
        }
        if !response.status().is_success() {
            return Err(A2aRuntimeError::Transport(format!(
                "http_{}",
                response.status().as_u16()
            )));
        }
        if response
            .content_length()
            .is_some_and(|len| len as usize > MAX_CARD_RESPONSE_BYTES)
        {
            return Err(A2aRuntimeError::CardTooLarge);
        }
        let body = response
            .bytes()
            .await
            .map_err(|error| A2aRuntimeError::Transport(error.to_string()))?;
        if body.len() > MAX_CARD_RESPONSE_BYTES {
            return Err(A2aRuntimeError::CardTooLarge);
        }
        Ok(body.to_vec())
    }

    async fn request_json(
        &self,
        profile: &RemoteAgentProfile,
        method: reqwest::Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, A2aRuntimeError> {
        let bytes = self.request_bytes(profile, method, path, body).await?;
        serde_json::from_slice(&bytes).map_err(|_| A2aRuntimeError::InvalidCard)
    }

    /// Fetches and validates a bounded agent card from the profile-owned origin.
    pub async fn fetch_card(
        &self,
        profile: &RemoteAgentProfile,
        fetched_at_ms: i64,
        ttl_ms: i64,
    ) -> Result<RemoteAgentCardSnapshot, A2aRuntimeError> {
        let bytes = self
            .request_bytes(
                profile,
                reqwest::Method::GET,
                ".well-known/agent.json",
                None,
            )
            .await?;
        parse_card_response(profile, &bytes, fetched_at_ms, ttl_ms)
    }

    /// Submits a hash-bound delegation and returns a typed transport outcome.
    pub async fn submit(
        &self,
        profile: &RemoteAgentProfile,
        request: &DelegationRequest,
        capsule: &ContextCapsule,
    ) -> Result<TransportOutcome, A2aRuntimeError> {
        let response = match self
            .request_json(
                profile,
                reqwest::Method::POST,
                "a2a/tasks",
                Some(serde_json::json!({
                    "protocol_version": A2A_PROTOCOL_VERSION,
                    "idempotency_key": request.idempotency_key,
                    "request_hash": request.request_hash,
                    "objective_hash": request.objective_hash,
                    "capability": request.capability,
                    "capsule_hash": capsule.content_hash,
                    "context_refs": capsule.context_refs,
                    "privacy_class": capsule.privacy_class,
                })),
            )
            .await
        {
            Ok(response) => response,
            // A connection/write/read failure after submit may have reached
            // the remote endpoint. Never turn that ambiguity into a retry.
            Err(A2aRuntimeError::Transport(_)) => {
                return Ok(TransportOutcome::UnknownOutcome);
            }
            Err(error) => return Err(error),
        };
        let remote_task_id = response
            .get("task_id")
            .and_then(serde_json::Value::as_str)
            .filter(|value| crate::external_agent_a2a::is_valid_identifier(value))
            .ok_or(A2aRuntimeError::InvalidCard)?;
        Ok(TransportOutcome::Accepted {
            remote_task_id: remote_task_id.to_owned(),
        })
    }

    /// Polls a known task id. No submit is ever attempted by this method.
    pub async fn status(
        &self,
        profile: &RemoteAgentProfile,
        remote_task_id: &str,
    ) -> Result<serde_json::Value, A2aRuntimeError> {
        if !crate::external_agent_a2a::is_valid_identifier(remote_task_id)
            || remote_task_id.contains('/')
        {
            return Err(A2aRuntimeError::InvalidCard);
        }
        self.request_json(
            profile,
            reqwest::Method::GET,
            &format!("a2a/tasks/{remote_task_id}"),
            None,
        )
        .await
    }

    /// Requests cancellation for a known task id.
    pub async fn cancel(
        &self,
        profile: &RemoteAgentProfile,
        remote_task_id: &str,
    ) -> Result<serde_json::Value, A2aRuntimeError> {
        if !crate::external_agent_a2a::is_valid_identifier(remote_task_id)
            || remote_task_id.contains('/')
        {
            return Err(A2aRuntimeError::InvalidCard);
        }
        self.request_json(
            profile,
            reqwest::Method::POST,
            &format!("a2a/tasks/{remote_task_id}/cancel"),
            None,
        )
        .await
    }
}

/// Validates a resolved address after DNS resolution and before connecting.
/// The caller must invoke this for every address and every redirect hop.
pub fn validate_resolved_address(
    profile: &RemoteAgentProfile,
    address: IpAddr,
) -> Result<(), A2aRuntimeError> {
    if !profile.endpoint.allow_loopback
        && (address.is_loopback()
            || address.is_unspecified()
            || address.is_multicast()
            || matches!(address, IpAddr::V4(value) if value.is_link_local())
            || matches!(address, IpAddr::V6(value) if value.is_unicast_link_local())
            || matches!(address, IpAddr::V4(value) if value.is_private() || value.is_broadcast())
            || matches!(address, IpAddr::V6(value) if value.is_unique_local()))
    {
        return Err(A2aRuntimeError::ResolvedAddressDenied);
    }
    Ok(())
}

/// Rejects a redirect unless it stays on the exact profile origin.
pub fn validate_redirect(
    profile: &RemoteAgentProfile,
    next_origin: &str,
) -> Result<(), A2aRuntimeError> {
    if next_origin != profile.endpoint.origin {
        return Err(A2aRuntimeError::RedirectDenied);
    }
    Ok(())
}

/// Parses and normalizes a bounded card response. Raw bytes are hashed and are
/// never returned or retained by this function.
pub fn parse_card_response(
    profile: &RemoteAgentProfile,
    body: &[u8],
    fetched_at_ms: i64,
    ttl_ms: i64,
) -> Result<RemoteAgentCardSnapshot, A2aRuntimeError> {
    validate_endpoint(&profile.endpoint)?;
    if body.is_empty() || body.len() > MAX_CARD_RESPONSE_BYTES {
        return Err(A2aRuntimeError::CardTooLarge);
    }
    let wire: CardWire = serde_json::from_slice(body).map_err(|_| A2aRuntimeError::InvalidCard)?;
    let raw_hash = hex::encode(Sha256::digest(body));
    card_snapshot(
        profile,
        if wire.protocol_version == 0 {
            A2A_PROTOCOL_VERSION
        } else {
            wire.protocol_version
        },
        raw_hash,
        wire.skills,
        wire.schema_hashes,
        wire.supports_streaming,
        wire.auth_schemes,
        fetched_at_ms,
        ttl_ms,
    )
    .map_err(Into::into)
}

/// Validates an inline result without fetching URLs or accepting arbitrary
/// remote content. URL-shaped artifact values are intentionally rejected.
pub fn validate_inline_artifacts(
    artifacts: &[(String, String, Vec<u8>)],
    budget: u32,
) -> Result<Vec<ValidatedRemoteArtifact>, A2aRuntimeError> {
    if artifacts.len() > MAX_ARTIFACTS || artifacts.len() > budget as usize {
        return Err(A2aRuntimeError::InvalidArtifact);
    }
    artifacts
        .iter()
        .map(|(id, mime, bytes)| {
            if id.is_empty()
                || id.len() > 96
                || mime.is_empty()
                || mime.len() > 128
                || bytes.len() > MAX_ARTIFACT_BYTES
                || mime.eq_ignore_ascii_case("text/html")
                || id.starts_with("http://")
                || id.starts_with("https://")
            {
                return Err(A2aRuntimeError::InvalidArtifact);
            }
            Ok(ValidatedRemoteArtifact {
                id: id.clone(),
                mime: mime.clone(),
                bytes: bytes.len(),
                content_hash: hex::encode(Sha256::digest(bytes)),
            })
        })
        .collect()
}

impl A2aRuntimeRegistry {
    /// Inserts a profile after validating its endpoint and hash.
    pub fn register_profile(&mut self, profile: RemoteAgentProfile) -> Result<(), A2aRuntimeError> {
        let profile = crate::external_agent_a2a::validate_profile(profile)?;
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }

    /// Refreshes the profile-owned card snapshot from bounded bytes.
    pub fn refresh_card(
        &mut self,
        profile_id: &str,
        body: &[u8],
        now_ms: i64,
        ttl_ms: i64,
    ) -> Result<RemoteAgentCardSnapshot, A2aRuntimeError> {
        let profile = self
            .profiles
            .get(profile_id)
            .ok_or(A2aRuntimeError::InvalidCard)?;
        let card = parse_card_response(profile, body, now_ms, ttl_ms)?;
        self.cards.insert(profile_id.to_owned(), card.clone());
        Ok(card)
    }

    /// Builds and records an immutable task binding. The objective is hashed
    /// immediately and is not retained in the runtime registry.
    #[allow(clippy::too_many_arguments)]
    pub fn register_task(
        &mut self,
        profile_id: &str,
        delegation_id: String,
        parent_grant_ref: String,
        capability: String,
        objective: &str,
        required_privacy: PrivacyClass,
        idempotency_key: String,
        artifact_budget: u32,
        capsule_id: String,
        context_refs: Vec<String>,
        parent_grant_hash: String,
        policy_hash: String,
        now_ms: i64,
    ) -> Result<(DelegationRequest, ContextCapsule, RemoteTaskBinding), A2aRuntimeError> {
        let profile = self
            .profiles
            .get(profile_id)
            .filter(|profile| profile.enabled)
            .ok_or(A2aRuntimeError::InvalidCard)?;
        let card = self
            .cards
            .get(&profile.id)
            .ok_or(A2aRuntimeError::InvalidCard)?;
        let request = build_delegation(
            profile,
            card,
            delegation_id,
            parent_grant_ref,
            capability,
            objective,
            required_privacy,
            idempotency_key,
            artifact_budget,
            now_ms,
        )?;
        let capsule = build_capsule(
            &request,
            capsule_id,
            context_refs,
            parent_grant_hash,
            policy_hash.clone(),
            required_privacy,
        )?;
        let binding = build_binding(
            &request,
            &capsule,
            format!("binding:{}", request.delegation_id),
            policy_hash,
            now_ms,
        )?;
        self.bindings
            .insert(binding.binding_id.clone(), binding.clone());
        Ok((request, capsule, binding))
    }

    /// Applies a lifecycle state transition with CAS semantics.
    pub fn transition(
        &mut self,
        binding_id: &str,
        next: RemoteTaskState,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<RemoteTaskBinding, A2aRuntimeError> {
        let binding = self
            .bindings
            .get_mut(binding_id)
            .ok_or(A2aRuntimeError::InvalidCard)?;
        apply_transition(binding, next, expected_revision, now_ms)?;
        Ok(binding.clone())
    }

    /// Reconciles a known task id and never submits a new task.
    pub fn reconcile(
        &mut self,
        binding_id: &str,
        remote_task_id: String,
        expected_revision: u64,
        now_ms: i64,
    ) -> Result<RemoteTaskBinding, A2aRuntimeError> {
        let binding = self
            .bindings
            .get_mut(binding_id)
            .ok_or(A2aRuntimeError::InvalidCard)?;
        reconcile_known_task(binding, remote_task_id, expected_revision, now_ms)?;
        Ok(binding.clone())
    }

    /// Marks every nonterminal binding as unavailable after a transport/core
    /// restart; callers may reconcile known ids, but may not resubmit.
    pub fn recover_after_restart(&mut self, now_ms: i64) -> Vec<RemoteTaskBinding> {
        let mut recovered = Vec::new();
        for binding in self.bindings.values_mut() {
            if !binding.state.is_terminal() {
                let next = if binding.remote_task_id.is_some() {
                    RemoteTaskState::UnknownOutcome
                } else {
                    RemoteTaskState::Unavailable
                };
                if apply_transition(binding, next, binding.revision, now_ms).is_ok() {
                    recovered.push(binding.clone());
                }
            }
        }
        recovered
    }
}

/// Maps a transport result to the only safe lifecycle state.
pub fn transport_state(outcome: TransportOutcome) -> RemoteTaskState {
    match outcome {
        TransportOutcome::Accepted { .. } => RemoteTaskState::Submitted,
        TransportOutcome::UnknownOutcome => RemoteTaskState::UnknownOutcome,
        TransportOutcome::Unavailable => RemoteTaskState::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> RemoteAgentProfile {
        crate::external_agent_a2a::build_profile(RemoteAgentProfile {
            schema_version: SCHEMA_VERSION,
            id: "runtime.agent".into(),
            revision: 1,
            display_name: "Runtime agent".into(),
            protocol_kind: crate::external_coding_agent_adapter::ExternalAgentProtocolKind::A2a {
                protocol_version: A2A_PROTOCOL_VERSION,
            },
            endpoint: crate::external_agent_a2a::RemoteAgentEndpoint {
                origin: "https://agents.example.com".into(),
                allow_loopback: false,
                allowed_ports: vec![],
            },
            auth_profile_ref: None,
            skill_allowlist: vec!["code.review".into()],
            capability_allowlist: vec!["agent.execute".into()],
            privacy_ceiling: PrivacyClass::Sensitive,
            concurrency_limit: 1,
            enabled: true,
            content_hash: String::new(),
        })
        .unwrap()
    }

    #[test]
    fn public_dns_policy_is_checked_after_resolution() {
        let mut profile = profile();
        assert!(validate_resolved_address(&profile, "127.0.0.1".parse().unwrap()).is_err());
        profile.endpoint.allow_loopback = true;
        assert!(validate_resolved_address(&profile, "127.0.0.1".parse().unwrap()).is_ok());
    }

    #[test]
    fn card_and_artifacts_are_bounded_and_redacted() {
        let profile = profile();
        let body = br#"{"skills":[{"id":"code.review","capability":"agent.execute"}],"auth_schemes":["bearer"]}"#;
        let card = parse_card_response(&profile, body, 1, 1000).unwrap();
        assert!(card.raw_hash.len() == 64);
        let artifacts = validate_inline_artifacts(
            &[("report.txt".into(), "text/plain".into(), b"ok".to_vec())],
            1,
        )
        .unwrap();
        assert_eq!(artifacts[0].bytes, 2);
        assert!(validate_inline_artifacts(
            &[("https://evil".into(), "text/plain".into(), vec![])],
            1
        )
        .is_err());
    }

    #[test]
    fn ambiguous_transport_never_becomes_submitting_again() {
        assert_eq!(
            transport_state(TransportOutcome::UnknownOutcome),
            RemoteTaskState::UnknownOutcome
        );
    }

    #[test]
    fn recover_after_restart_covers_all_nonterminal_and_terminal_states() {
        let mut registry = A2aRuntimeRegistry::default();
        let p = profile();
        registry.register_profile(p.clone()).unwrap();
        let body = br#"{"skills":[{"id":"code.review","capability":"agent.execute"}],"auth_schemes":["bearer"]}"#;
        registry.refresh_card(&p.id, body, 10, 60_000).unwrap();

        let states = [
            ("reg", RemoteTaskState::Registered, None),
            ("submitting", RemoteTaskState::Submitting, None),
            ("submitted", RemoteTaskState::Submitted, Some("r-sub")),
            ("input", RemoteTaskState::InputRequired, Some("r-inp")),
            ("auth", RemoteTaskState::AuthRequired, Some("r-auth")),
            ("running", RemoteTaskState::Running, Some("r-run")),
            (
                "cancel_req",
                RemoteTaskState::CancelRequested,
                Some("r-can"),
            ),
            ("done", RemoteTaskState::Completed, Some("r-done")),
            ("rejected", RemoteTaskState::Rejected, Some("r-rej")),
            ("failed", RemoteTaskState::Failed, Some("r-fail")),
            ("cancelled", RemoteTaskState::Cancelled, Some("r-cx")),
        ];

        for (idx, (suffix, target_state, remote_id)) in states.iter().enumerate() {
            let delegation_id = format!("del-{suffix}");
            let (_, _, binding) = registry
                .register_task(
                    &p.id,
                    delegation_id.clone(),
                    "grant-1".into(),
                    "agent.execute".into(),
                    "inspect",
                    PrivacyClass::Sensitive,
                    format!("idem-{suffix}"),
                    2,
                    format!("cap-{suffix}"),
                    vec!["ns.default".into()],
                    "a".repeat(64),
                    "b".repeat(64),
                    100 + idx as i64,
                )
                .unwrap();
            let mut rev = binding.revision;
            if *target_state != RemoteTaskState::Registered {
                let b = registry
                    .transition(&binding.binding_id, RemoteTaskState::Submitting, rev, 200)
                    .unwrap();
                rev = b.revision;
            }
            if let Some(rid) = remote_id {
                let b = registry
                    .reconcile(&binding.binding_id, (*rid).to_owned(), rev, 210)
                    .unwrap();
                let b = registry
                    .transition(
                        &binding.binding_id,
                        RemoteTaskState::Submitted,
                        b.revision,
                        211,
                    )
                    .unwrap();
                rev = b.revision;
            }
            if !matches!(
                target_state,
                RemoteTaskState::Registered
                    | RemoteTaskState::Submitting
                    | RemoteTaskState::Submitted
            ) {
                if *target_state == RemoteTaskState::Completed {
                    let b = registry
                        .transition(&binding.binding_id, RemoteTaskState::Running, rev, 215)
                        .unwrap();
                    rev = b.revision;
                } else if *target_state == RemoteTaskState::Cancelled {
                    let b = registry
                        .transition(
                            &binding.binding_id,
                            RemoteTaskState::CancelRequested,
                            rev,
                            215,
                        )
                        .unwrap();
                    rev = b.revision;
                }
                registry
                    .transition(&binding.binding_id, *target_state, rev, 220)
                    .unwrap();
            }
        }

        let recovered = registry.recover_after_restart(500);
        assert_eq!(recovered.len(), 7);
        assert_eq!(
            registry.bindings["binding:del-reg"].state,
            RemoteTaskState::Unavailable
        );
        assert_eq!(
            registry.bindings["binding:del-submitting"].state,
            RemoteTaskState::Unavailable
        );
        for suffix in ["submitted", "input", "auth", "running", "cancel_req"] {
            assert_eq!(
                registry.bindings[&format!("binding:del-{suffix}")].state,
                RemoteTaskState::UnknownOutcome
            );
        }
        assert_eq!(
            registry.bindings["binding:del-done"].state,
            RemoteTaskState::Completed
        );
        assert_eq!(
            registry.bindings["binding:del-cancelled"].state,
            RemoteTaskState::Cancelled
        );
    }

    async fn spawn_one_shot_http(response: String) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });
        addr
    }

    fn loopback_profile(port: u16) -> RemoteAgentProfile {
        crate::external_agent_a2a::build_profile(RemoteAgentProfile {
            schema_version: SCHEMA_VERSION,
            id: "loopback.agent".into(),
            revision: 1,
            display_name: "Loopback agent".into(),
            protocol_kind: crate::external_coding_agent_adapter::ExternalAgentProtocolKind::A2a {
                protocol_version: A2A_PROTOCOL_VERSION,
            },
            endpoint: crate::external_agent_a2a::RemoteAgentEndpoint {
                origin: format!("http://127.0.0.1:{port}"),
                allow_loopback: true,
                allowed_ports: vec![port],
            },
            auth_profile_ref: None,
            skill_allowlist: vec!["code.review".into()],
            capability_allowlist: vec!["agent.execute".into()],
            privacy_ceiling: PrivacyClass::Sensitive,
            concurrency_limit: 1,
            enabled: true,
            content_hash: String::new(),
        })
        .unwrap()
    }

    #[tokio::test]
    async fn http_transport_enforces_redirect_off_card_budget_and_unknown_outcome() {
        let transport = A2aHttpTransport::default();

        // 1. Redirect 302 must fail closed.
        let addr = spawn_one_shot_http(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/other\r\nContent-Length: 0\r\n\r\n"
                .into(),
        )
        .await;
        let p = loopback_profile(addr.port());
        let err = transport.fetch_card(&p, 1, 1000).await.unwrap_err();
        assert_eq!(err, A2aRuntimeError::RedirectDenied);

        // 2. Oversized card must fail closed.
        let huge = "a".repeat(MAX_CARD_RESPONSE_BYTES + 16);
        let addr = spawn_one_shot_http(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            huge.len(),
            huge
        ))
        .await;
        let p = loopback_profile(addr.port());
        let err = transport.fetch_card(&p, 1, 1000).await.unwrap_err();
        assert_eq!(err, A2aRuntimeError::CardTooLarge);

        // 3. Valid card succeeds.
        let card_body = br#"{"skills":[{"id":"code.review","capability":"agent.execute"}],"auth_schemes":["bearer"]}"#;
        let addr = spawn_one_shot_http(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            card_body.len(),
            std::str::from_utf8(card_body).unwrap()
        ))
        .await;
        let p = loopback_profile(addr.port());
        let card = transport.fetch_card(&p, 10, 60_000).await.unwrap();
        assert_eq!(card.profile_id, "loopback.agent");

        // 4. Submit 500 -> UnknownOutcome (never blind retry).
        let addr = spawn_one_shot_http(
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".into(),
        )
        .await;
        let p = loopback_profile(addr.port());
        let mut reg = A2aRuntimeRegistry::default();
        reg.register_profile(p.clone()).unwrap();
        reg.refresh_card(&p.id, card_body, 10, 60_000).unwrap();
        let (req, cap, _) = reg
            .register_task(
                &p.id,
                "del-http".into(),
                "grant-1".into(),
                "agent.execute".into(),
                "check",
                PrivacyClass::Sensitive,
                "idem-http".into(),
                1,
                "cap-http".into(),
                vec!["ns:default".into()],
                "a".repeat(64),
                "b".repeat(64),
                20,
            )
            .unwrap();
        let outcome = transport.submit(&p, &req, &cap).await.unwrap();
        assert_eq!(outcome, TransportOutcome::UnknownOutcome);

        // 5. Path traversal in task id is rejected before transport.
        assert_eq!(
            transport.status(&p, "../secret").await.unwrap_err(),
            A2aRuntimeError::InvalidCard
        );
        assert_eq!(
            transport.cancel(&p, "../secret").await.unwrap_err(),
            A2aRuntimeError::InvalidCard
        );
    }
}
