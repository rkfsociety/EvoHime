//! Core-to-supervisor contract for the local provider lifecycle.
//!
//! The process adapter is intentionally supervisor-owned.  This module keeps
//! the bounded, testable state machine separate from the Windows Job Object
//! plumbing in `windows_supervisor.rs`.

use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const PORT_FIRST: u16 = 49_152;
pub const PORT_LAST: u16 = 49_252;
pub const MAX_PORT_ATTEMPTS: usize = 8;
pub const SESSION_TTL_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState { Starting, Running, Stopping, Stopped }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus { Ready, Degraded, Stale, Unavailable }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalError { ModelNotFound, PortUnavailable, AlreadyCancelled, InvalidRequest, ResourceLimitExceeded, Timeout, Cancelled }

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
struct ProcessRecord { model_id: String, state: ProcessState, port: u16, references: u32, idle_since_ms: Option<u64>, limits: ResourceLimits, sessions: BTreeMap<String, [u8; 32]> }

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
        record.sessions.insert(request_id.to_owned(), hash);
        Ok((SessionGrant { token, request_id: request_id.to_owned(), expires_at_ms: now_ms.saturating_add(SESSION_TTL_MS), port: record.port }, HealthEvent { request_id: request_id.to_owned(), model_id: model_id.to_owned(), process_state: ProcessState::Running, health_status: HealthStatus::Ready, reason: None, port: Some(record.port) }))
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn reuses_process_and_stops_idempotently() { let mut manager = LocalProviderManager::default(); let (first, _) = manager.launch("m", "r1", 0, &[], ResourceLimits::default()).unwrap(); let (second, _) = manager.launch("m", "r2", 1, &[], ResourceLimits::default()).unwrap(); assert_eq!(first.port, second.port); assert_eq!(manager.process_count(), 1); assert!(manager.stop("m", "r1", 2).is_ok()); assert!(manager.stop("m", "r2", 3).is_ok()); assert_eq!(manager.process_count(), 1); assert!(manager.stop("m", "r2", 4).is_err()); }
    #[test] fn launch_stop_race_is_cancelled() { let mut manager = LocalProviderManager::default(); manager.cancelled.insert("r".into(), true); assert_eq!(manager.launch("m", "r", 0, &[], ResourceLimits::default()), Err(LocalError::Cancelled)); }
    #[test] fn port_selection_is_bounded() { let occupied: Vec<u16> = (PORT_FIRST..PORT_FIRST + 8).collect(); assert_eq!(choose_port(&occupied), Some(PORT_FIRST + 8)); }
}
