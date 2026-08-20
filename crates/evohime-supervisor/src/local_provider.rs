//! Core-to-supervisor contract for the local provider lifecycle.
//!
//! The process adapter is intentionally supervisor-owned.  This module keeps
//! the bounded, testable state machine separate from the Windows Job Object
//! plumbing in `windows_supervisor.rs`.

use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[cfg(windows)]
use tokio::process::{Child, Command};

#[cfg(windows)]
use crate::windows_supervisor::JobObject;

pub const PORT_FIRST: u16 = 49_152;
pub const PORT_LAST: u16 = 49_252;
pub const MAX_PORT_ATTEMPTS: usize = 8;
pub const SESSION_TTL_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState { Starting, Running, Stopping, Stopped }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus { Ready, Degraded, Stale, Unavailable }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalError { ModelNotFound, PortUnavailable, AlreadyCancelled, InvalidRequest, ResourceLimitExceeded, Timeout, Cancelled, AuthenticationFailed, SessionExpired }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits { pub adapter_memory_bytes: u64, pub runtime_memory_bytes: u64, pub adapter_cpu_percent: u8, pub runtime_cpu_percent: u8 }

impl Default for ResourceLimits {
    fn default() -> Self { Self { adapter_memory_bytes: 512 * 1024 * 1024, runtime_memory_bytes: 4 * 1024 * 1024 * 1024, adapter_cpu_percent: 25, runtime_cpu_percent: 75 } }
}

impl ResourceLimits {
    pub fn validate(self) -> Result<Self, LocalError> {
        if self.adapter_memory_bytes == 0 || self.adapter_memory_bytes > 1024 * 1024 * 1024 || self.runtime_memory_bytes == 0 || self.runtime_memory_bytes > 12 * 1024 * 1024 * 1024 || self.adapter_cpu_percent > 100 || self.runtime_cpu_percent > 100 { return Err(LocalError::InvalidRequest); }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionGrant { pub token: Vec<u8>, pub request_id: String, pub expires_at_ms: u64, pub port: u16 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthEvent { pub request_id: String, pub model_id: String, pub process_state: ProcessState, pub health_status: HealthStatus, pub reason: Option<&'static str>, pub port: Option<u16> }

#[derive(Debug)]
struct SessionRecord { hash: [u8; 32], expires_at_ms: u64, used: bool }

#[derive(Debug)]
struct ProcessRecord { model_id: String, state: ProcessState, port: u16, references: u32, idle_since_ms: Option<u64>, limits: ResourceLimits, sessions: BTreeMap<String, SessionRecord> }

#[derive(Debug, Default)]
pub struct LocalProviderManager { processes: BTreeMap<String, ProcessRecord>, cancelled: BTreeMap<String, bool> }

impl LocalProviderManager {
    pub fn launch(&mut self, model_id: &str, request_id: &str, now_ms: u64, occupied_ports: &[u16], limits: ResourceLimits) -> Result<(SessionGrant, HealthEvent), LocalError> {
        if model_id.trim().is_empty() || request_id.trim().is_empty() { return Err(LocalError::InvalidRequest); }
        let limits = limits.validate()?;
        if self.cancelled.remove(request_id).is_some() { return Err(LocalError::Cancelled); }
        let port = if let Some(record) = self.processes.get(model_id) { record.port } else { choose_port(occupied_ports).ok_or(LocalError::PortUnavailable)? };
        let record = self.processes.entry(model_id.to_owned()).or_insert_with(|| ProcessRecord { model_id: model_id.to_owned(), state: ProcessState::Starting, port, references: 0, idle_since_ms: None, limits, sessions: BTreeMap::new() });
        record.state = ProcessState::Running;
        record.references = record.references.saturating_add(1);
        record.idle_since_ms = None;
        let mut token = vec![0u8; 32]; OsRng.fill_bytes(&mut token);
        let mut hash = [0u8; 32]; hash.copy_from_slice(&Sha256::digest(&token));
        let expires_at_ms = now_ms.saturating_add(SESSION_TTL_MS);
        record.sessions.insert(request_id.to_owned(), SessionRecord { hash, expires_at_ms, used: false });
        Ok((SessionGrant { token, request_id: request_id.to_owned(), expires_at_ms, port: record.port }, HealthEvent { request_id: request_id.to_owned(), model_id: model_id.to_owned(), process_state: ProcessState::Running, health_status: HealthStatus::Ready, reason: None, port: Some(record.port) }))
    }

    /// Authenticates a launch grant exactly once. The grant is consumed at
    /// request admission, so a response that takes longer than the session TTL
    /// does not invalidate an already admitted request.
    pub fn authenticate(&mut self, model_id: &str, request_id: &str, token: &[u8], now_ms: u64) -> Result<u16, LocalError> {
        let record = self.processes.get_mut(model_id).ok_or(LocalError::ModelNotFound)?;
        let session = record.sessions.get_mut(request_id).ok_or(LocalError::AuthenticationFailed)?;
        if now_ms > session.expires_at_ms { return Err(LocalError::SessionExpired); }
        if session.used || Sha256::digest(token).as_slice() != session.hash { return Err(LocalError::AuthenticationFailed); }
        session.used = true;
        Ok(record.port)
    }

    pub fn stop(&mut self, model_id: &str, request_id: &str, now_ms: u64) -> Result<HealthEvent, LocalError> {
        let Some(record) = self.processes.get_mut(model_id) else { return Ok(HealthEvent { request_id: request_id.to_owned(), model_id: model_id.to_owned(), process_state: ProcessState::Stopped, health_status: HealthStatus::Unavailable, reason: Some("already_cancelled"), port: None }); };
        if record.sessions.remove(request_id).is_none() { self.cancelled.insert(request_id.to_owned(), true); return Err(LocalError::AlreadyCancelled); }
        record.references = record.references.saturating_sub(1);
        if record.references == 0 { record.idle_since_ms = Some(now_ms); record.state = ProcessState::Stopping; record.state = ProcessState::Stopped; }
        let stopped = record.state == ProcessState::Stopped;
        Ok(HealthEvent { request_id: request_id.to_owned(), model_id: model_id.to_owned(), process_state: record.state, health_status: if stopped { HealthStatus::Unavailable } else { HealthStatus::Ready }, reason: None, port: Some(record.port) })
    }

    pub fn reap_idle(&mut self, now_ms: u64, idle_timeout_ms: u64) -> Vec<HealthEvent> {
        let ids: Vec<String> = self.processes.iter().filter(|(_, p)| p.references == 0 && p.idle_since_ms.is_some_and(|at| now_ms.saturating_sub(at) >= idle_timeout_ms)).map(|(id, _)| id.clone()).collect();
        ids.into_iter().filter_map(|id| self.processes.remove(&id).map(|p| HealthEvent { request_id: String::new(), model_id: p.model_id, process_state: ProcessState::Stopped, health_status: HealthStatus::Unavailable, reason: None, port: Some(p.port) })).collect()
    }

    pub fn process_count(&self) -> usize { self.processes.len() }
}

pub fn choose_port(occupied: &[u16]) -> Option<u16> {
    (PORT_FIRST..=PORT_LAST).filter(|port| !occupied.contains(port)).take(MAX_PORT_ATTEMPTS).next()
}

/// Supervisor-owned adapter process. The renderer never supplies an
/// executable or command line: the supervisor reads the configured adapter
/// path from its own environment and passes only the selected model and port.
#[cfg(windows)]
pub struct LocalAdapterProcess {
    child: Child,
    _job: JobObject,
}

#[cfg(windows)]
impl LocalAdapterProcess {
    pub async fn spawn(model_id: &str, port: u16) -> Result<Self, LocalError> {
        Self::spawn_with_limits(model_id, port, ResourceLimits::default()).await
    }

    pub async fn spawn_with_limits(model_id: &str, port: u16, limits: ResourceLimits) -> Result<Self, LocalError> {
        if model_id.trim().is_empty() || !(PORT_FIRST..=PORT_LAST).contains(&port) {
            return Err(LocalError::InvalidRequest);
        }
        let limits = limits.validate()?;
        let executable = std::env::var_os("EVOHIME_LOCAL_ADAPTER_EXE")
            .ok_or(LocalError::ModelNotFound)?;
        let job = JobObject::create_with_limits(Some(limits.adapter_memory_bytes), Some(limits.adapter_cpu_percent)).map_err(|_| LocalError::ResourceLimitExceeded)?;
        let mut child = Command::new(executable)
            .arg("--model-id")
            .arg(model_id)
            .arg("--port")
            .arg(port.to_string())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| LocalError::ModelNotFound)?;
        if job.assign(&child).is_err() {
            let _ = child.start_kill();
            return Err(LocalError::ResourceLimitExceeded);
        }
        Ok(Self { child, _job: job })
    }

    pub async fn stop(&mut self) -> Result<(), LocalError> {
        self.child.start_kill().map_err(|_| LocalError::AlreadyCancelled)?;
        let _ = self.child.wait().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn reuses_process_and_stops_idempotently() { let mut manager = LocalProviderManager::default(); let (first, _) = manager.launch("m", "r1", 0, &[], ResourceLimits::default()).unwrap(); let (second, _) = manager.launch("m", "r2", 1, &[], ResourceLimits::default()).unwrap(); assert_eq!(first.port, second.port); assert_eq!(manager.process_count(), 1); assert!(manager.stop("m", "r1", 2).is_ok()); assert!(manager.stop("m", "r2", 3).is_ok()); assert_eq!(manager.process_count(), 1); assert!(manager.stop("m", "r2", 4).is_err()); }
    #[test] fn launch_stop_race_is_cancelled() { let mut manager = LocalProviderManager::default(); manager.cancelled.insert("r".into(), true); assert_eq!(manager.launch("m", "r", 0, &[], ResourceLimits::default()), Err(LocalError::Cancelled)); }
    #[test] fn session_grant_is_single_use_and_time_bounded() {
        let mut manager = LocalProviderManager::default();
        let (grant, _) = manager.launch("m", "r", 1_000, &[], ResourceLimits::default()).unwrap();
        assert_eq!(manager.authenticate("m", "r", &grant.token, 1_001), Ok(grant.port));
        assert_eq!(manager.authenticate("m", "r", &grant.token, 1_002), Err(LocalError::AuthenticationFailed));
        let (expired, _) = manager.launch("m", "r2", 1_000, &[], ResourceLimits::default()).unwrap();
        assert_eq!(manager.authenticate("m", "r2", &expired.token, expired.expires_at_ms + 1), Err(LocalError::SessionExpired));
    }
    #[test] fn port_selection_is_bounded() { let occupied: Vec<u16> = (PORT_FIRST..PORT_FIRST + 8).collect(); assert_eq!(choose_port(&occupied), Some(PORT_FIRST + 8)); }
}
