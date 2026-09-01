//! Core-owned contract for the sandboxed agentic browser session.
//!
//! The browser adapter is deliberately not authoritative: it receives these
//! bounded commands and reports typed observations back to Core. References
//! are ephemeral and can never be used across a page revision or takeover.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const CONTRACT_VERSION: u32 = 1;
pub const MAX_URL_CHARS: usize = 8 * 1024;
pub const MAX_TEXT_CHARS: usize = 16 * 1024;
pub const MAX_ELEMENTS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Starting,
    Ready,
    Active,
    Closing,
    Closed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlOwner {
    Agent,
    Human,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserSession {
    pub schema_version: u32,
    pub session_id: Uuid,
    pub conversation_id: String,
    pub run_id: Option<String>,
    pub state: SessionState,
    pub revision: u64,
    pub control_owner: ControlOwner,
    pub control_generation: u64,
    pub profile_policy: String,
    pub network_policy: String,
    pub policy_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageRef {
    pub session_id: Uuid,
    pub revision: u64,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementRef {
    pub page: PageRef,
    pub ref_id: String,
    pub role: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSnapshot {
    pub page: PageRef,
    pub url_projection: String,
    pub title: String,
    pub text: String,
    pub elements: Vec<ElementRef>,
    pub artifact_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserError {
    InvalidInput,
    UnsupportedVersion,
    StaleElementRef,
    ControlTaken,
    PolicyDenied,
    LegacyDisabled,
    Unavailable,
    UnknownOutcome,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
    #[error("unsupported browser contract version")]
    UnsupportedVersion,
    #[error("invalid browser session input: {0}")]
    InvalidInput(String),
    #[error("stale element reference")]
    StaleElementRef,
    #[error("human control is active")]
    ControlTaken,
}

impl BrowserSession {
    pub fn new(
        conversation_id: impl Into<String>,
        run_id: Option<String>,
        policy_hash: impl Into<String>,
    ) -> Result<Self, ContractError> {
        let conversation_id = conversation_id.into();
        if conversation_id.is_empty() || conversation_id.len() > 128 {
            return Err(ContractError::InvalidInput("conversation_id".into()));
        }
        Ok(Self {
            schema_version: CONTRACT_VERSION,
            session_id: Uuid::new_v4(),
            conversation_id,
            run_id,
            state: SessionState::Created,
            revision: 0,
            control_owner: ControlOwner::Agent,
            control_generation: 0,
            profile_policy: "ephemeral_clean".into(),
            network_policy: "public_internet".into(),
            policy_hash: policy_hash.into(),
        })
    }

    pub fn transition(&mut self, next: SessionState) -> Result<(), ContractError> {
        let valid = matches!(
            (self.state, next),
            (SessionState::Created, SessionState::Starting)
                | (
                    SessionState::Starting,
                    SessionState::Ready | SessionState::Failed
                )
                | (
                    SessionState::Ready,
                    SessionState::Active | SessionState::Closing | SessionState::Failed
                )
                | (
                    SessionState::Active,
                    SessionState::Closing | SessionState::Failed
                )
                | (SessionState::Closing, SessionState::Closed)
                | (
                    SessionState::Failed,
                    SessionState::Closing | SessionState::Closed
                )
        );
        if !valid {
            return Err(ContractError::InvalidInput(
                "invalid_state_transition".into(),
            ));
        }
        self.state = next;
        Ok(())
    }

    pub fn take_control(&mut self) -> Result<(), ContractError> {
        if self.state != SessionState::Ready && self.state != SessionState::Active {
            return Err(ContractError::InvalidInput("session_not_active".into()));
        }
        self.control_owner = ControlOwner::Human;
        self.control_generation = self.control_generation.saturating_add(1);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn return_control(&mut self) -> Result<(), ContractError> {
        if self.control_owner != ControlOwner::Human {
            return Err(ContractError::InvalidInput(
                "human_control_not_active".into(),
            ));
        }
        self.control_owner = ControlOwner::Agent;
        self.control_generation = self.control_generation.saturating_add(1);
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn validate_page(&self, page: &PageRef) -> Result<(), ContractError> {
        if page.session_id != self.session_id || page.revision != self.revision {
            return Err(ContractError::StaleElementRef);
        }
        Ok(())
    }

    pub fn validate_agent_mutation(&self, page: &PageRef) -> Result<(), ContractError> {
        self.validate_page(page)?;
        if self.control_owner != ControlOwner::Agent {
            return Err(ContractError::ControlTaken);
        }
        Ok(())
    }
}

pub fn fingerprint(session_id: Uuid, revision: u64, url: &str, title: &str) -> String {
    let mut h = Sha256::new();
    h.update(session_id.as_bytes());
    h.update(revision.to_le_bytes());
    h.update(url.as_bytes());
    h.update(title.as_bytes());
    hex::encode(h.finalize())
}

pub fn validate_snapshot(snapshot: &PageSnapshot) -> Result<(), ContractError> {
    if snapshot.url_projection.len() > MAX_URL_CHARS
        || snapshot.title.len() > MAX_TEXT_CHARS
        || snapshot.text.len() > MAX_TEXT_CHARS
        || snapshot.elements.len() > MAX_ELEMENTS
    {
        return Err(ContractError::InvalidInput("snapshot_bounds".into()));
    }
    if snapshot
        .elements
        .iter()
        .any(|element| element.page != snapshot.page)
    {
        return Err(ContractError::StaleElementRef);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_and_takeover_fence_mutations() {
        let mut session =
            BrowserSession::new("conversation", Some("run".into()), "policy").unwrap();
        session.transition(SessionState::Starting).unwrap();
        session.transition(SessionState::Ready).unwrap();
        let page = PageRef {
            session_id: session.session_id,
            revision: session.revision,
            fingerprint: "f".into(),
        };
        session.take_control().unwrap();
        assert_eq!(
            session.validate_agent_mutation(&page),
            Err(ContractError::StaleElementRef)
        );
        session.return_control().unwrap();
        assert_eq!(
            session.validate_agent_mutation(&page),
            Err(ContractError::StaleElementRef)
        );
    }

    #[test]
    fn wrong_session_and_oversized_snapshot_fail_closed() {
        let session = BrowserSession::new("conversation", None, "policy").unwrap();
        let page = PageRef {
            session_id: Uuid::new_v4(),
            revision: 0,
            fingerprint: "f".into(),
        };
        assert_eq!(
            session.validate_page(&page),
            Err(ContractError::StaleElementRef)
        );
        let page = PageRef {
            session_id: session.session_id,
            revision: 0,
            fingerprint: "f".into(),
        };
        let snapshot = PageSnapshot {
            page,
            url_projection: "x".repeat(MAX_URL_CHARS + 1),
            title: String::new(),
            text: String::new(),
            elements: vec![],
            artifact_ref: None,
        };
        assert!(matches!(
            validate_snapshot(&snapshot),
            Err(ContractError::InvalidInput(_))
        ));
    }
}
