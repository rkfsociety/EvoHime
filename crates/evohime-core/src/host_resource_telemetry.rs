//! Core-owned, bounded host resource snapshot and pressure contract.
//!
//! Collectors are adapters only. They submit validated samples here; this
//! module owns freshness, pressure semantics and the bounded recent history.
//! Missing sensors remain explicit and never become a healthy zero.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use thiserror::Error;

pub const CONTRACT_VERSION: &str = "host-resource-telemetry/v1";
pub const POLICY_REVISION: &str = "host-pressure-policy/v1";
pub const MAX_HISTORY: usize = 120;
pub const MAX_REASONS: usize = 8;
pub const MAX_REASON_BYTES: usize = 96;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Available,
    Unavailable,
    PermissionDenied,
    Unsupported,
    TransientFailure,
    Stale,
    InvalidSample,
    NotCollected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    Unknown,
    Normal,
    Elevated,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeasuredMetric {
    pub value: Option<f64>,
    pub unit: String,
    pub source: String,
    pub observed_at_ms: i64,
    pub freshness_ms: u64,
    pub status: MetricStatus,
}

impl MeasuredMetric {
    pub fn unavailable(unit: &str, source: &str, now_ms: i64, status: MetricStatus) -> Self {
        Self {
            value: None,
            unit: unit.into(),
            source: source.into(),
            observed_at_ms: now_ms,
            freshness_ms: 0,
            status,
        }
    }

    pub fn validate(&self, now_ms: i64) -> Result<(), TelemetryError> {
        if self.unit.len() > 32
            || self.source.len() > 96
            || self.observed_at_ms <= 0
            || self.observed_at_ms > now_ms.saturating_add(60_000)
        {
            return Err(TelemetryError::InvalidSample);
        }
        if self
            .value
            .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(TelemetryError::InvalidSample);
        }
        if self.status == MetricStatus::Available && self.value.is_none() {
            return Err(TelemetryError::InvalidSample);
        }
        Ok(())
    }

    pub fn is_current(&self, now_ms: i64, max_age_ms: u64) -> bool {
        self.status == MetricStatus::Available
            && now_ms >= self.observed_at_ms
            && (now_ms - self.observed_at_ms) as u64
                <= max_age_ms.min(self.freshness_ms.max(max_age_ms))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostResourceSnapshot {
    pub contract_version: String,
    pub snapshot_id: String,
    pub observed_at_ms: i64,
    pub cpu_percent: MeasuredMetric,
    pub memory_available_percent: MeasuredMetric,
    pub storage_free_percent: MeasuredMetric,
    pub pressure: PressureLevel,
    pub reasons: Vec<String>,
}

impl HostResourceSnapshot {
    pub fn validate(&self, now_ms: i64) -> Result<(), TelemetryError> {
        if self.contract_version != CONTRACT_VERSION
            || self.snapshot_id.is_empty()
            || self.snapshot_id.len() > 128
            || self.observed_at_ms <= 0
        {
            return Err(TelemetryError::InvalidSample);
        }
        if self.reasons.len() > MAX_REASONS
            || self
                .reasons
                .iter()
                .any(|reason| reason.is_empty() || reason.len() > MAX_REASON_BYTES)
        {
            return Err(TelemetryError::InvalidSample);
        }
        self.cpu_percent.validate(now_ms)?;
        self.memory_available_percent.validate(now_ms)?;
        self.storage_free_percent.validate(now_ms)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PressurePolicy {
    pub revision: &'static str,
    pub max_age_ms: u64,
    pub memory_elevated_below_percent: f64,
    pub memory_high_below_percent: f64,
    pub memory_critical_below_percent: f64,
    pub storage_high_below_percent: f64,
    pub storage_critical_below_percent: f64,
    pub cpu_high_above_percent: f64,
}

impl Default for PressurePolicy {
    fn default() -> Self {
        Self {
            revision: POLICY_REVISION,
            max_age_ms: 5_000,
            memory_elevated_below_percent: 25.0,
            memory_high_below_percent: 12.0,
            memory_critical_below_percent: 5.0,
            storage_high_below_percent: 10.0,
            storage_critical_below_percent: 3.0,
            cpu_high_above_percent: 95.0,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TelemetryError {
    #[error("invalid telemetry sample")]
    InvalidSample,
    #[error("snapshot is stale or unavailable")]
    UnknownPressure,
}

pub fn evaluate(
    snapshot: &HostResourceSnapshot,
    policy: PressurePolicy,
    now_ms: i64,
) -> Result<PressureLevel, TelemetryError> {
    snapshot.validate(now_ms)?;
    let metrics = [
        &snapshot.cpu_percent,
        &snapshot.memory_available_percent,
        &snapshot.storage_free_percent,
    ];
    if metrics
        .iter()
        .any(|metric| !metric.is_current(now_ms, policy.max_age_ms))
    {
        return Err(TelemetryError::UnknownPressure);
    }
    let memory = snapshot
        .memory_available_percent
        .value
        .ok_or(TelemetryError::UnknownPressure)?;
    let storage = snapshot
        .storage_free_percent
        .value
        .ok_or(TelemetryError::UnknownPressure)?;
    let cpu = snapshot
        .cpu_percent
        .value
        .ok_or(TelemetryError::UnknownPressure)?;
    let level = if memory < policy.memory_critical_below_percent
        || storage < policy.storage_critical_below_percent
    {
        PressureLevel::Critical
    } else if memory < policy.memory_high_below_percent
        || storage < policy.storage_high_below_percent
        || cpu > policy.cpu_high_above_percent
    {
        PressureLevel::High
    } else if memory < policy.memory_elevated_below_percent {
        PressureLevel::Elevated
    } else {
        PressureLevel::Normal
    };
    Ok(level)
}

#[derive(Debug, Clone)]
pub struct HostTelemetryService {
    policy: PressurePolicy,
    history: VecDeque<HostResourceSnapshot>,
}

impl HostTelemetryService {
    pub fn new(policy: PressurePolicy) -> Self {
        Self {
            policy,
            history: VecDeque::with_capacity(MAX_HISTORY),
        }
    }
    pub fn record(
        &mut self,
        mut snapshot: HostResourceSnapshot,
        now_ms: i64,
    ) -> Result<PressureLevel, TelemetryError> {
        let level = evaluate(&snapshot, self.policy, now_ms)?;
        snapshot.pressure = level;
        if self.history.len() == MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(snapshot);
        Ok(level)
    }
    pub fn latest(&self) -> Option<&HostResourceSnapshot> {
        self.history.back()
    }
    pub fn history(&self) -> impl Iterator<Item = &HostResourceSnapshot> {
        self.history.iter()
    }
    pub fn policy(&self) -> PressurePolicy {
        self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metric(value: f64, now: i64) -> MeasuredMetric {
        MeasuredMetric {
            value: Some(value),
            unit: "%".into(),
            source: "fixture".into(),
            observed_at_ms: now,
            freshness_ms: 5_000,
            status: MetricStatus::Available,
        }
    }
    fn snapshot(now: i64, memory: f64, storage: f64) -> HostResourceSnapshot {
        HostResourceSnapshot {
            contract_version: CONTRACT_VERSION.into(),
            snapshot_id: "s1".into(),
            observed_at_ms: now,
            cpu_percent: metric(20.0, now),
            memory_available_percent: metric(memory, now),
            storage_free_percent: metric(storage, now),
            pressure: PressureLevel::Unknown,
            reasons: vec![],
        }
    }

    #[test]
    fn unavailable_sensor_is_not_a_zero_or_normal_result() {
        let now = 1_000;
        let mut value = snapshot(now, 50.0, 50.0);
        value.cpu_percent =
            MeasuredMetric::unavailable("%", "fixture", now, MetricStatus::Unsupported);
        assert_eq!(
            evaluate(&value, PressurePolicy::default(), now),
            Err(TelemetryError::UnknownPressure)
        );
    }

    #[test]
    fn pressure_is_deterministic_and_history_is_bounded() {
        let now = 1_000;
        let mut service = HostTelemetryService::new(PressurePolicy::default());
        assert_eq!(
            service.record(snapshot(now, 4.0, 50.0), now),
            Ok(PressureLevel::Critical)
        );
        for index in 0..MAX_HISTORY + 4 {
            let mut item = snapshot(now + index as i64 + 1, 50.0, 50.0);
            item.snapshot_id = index.to_string();
            assert_eq!(
                service.record(item, now + index as i64 + 1),
                Ok(PressureLevel::Normal)
            );
        }
        assert_eq!(service.history().count(), MAX_HISTORY);
    }
}
