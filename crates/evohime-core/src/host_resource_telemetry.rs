//! Core-owned, bounded host resource snapshot and pressure contract.
//!
//! Collectors are adapters only. They submit validated samples here; this
//! module owns freshness, pressure semantics and the bounded recent history.
//! Missing sensors remain explicit and never become a healthy zero.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use thiserror::Error;

/// Contract identifier for host resource telemetry snapshots.
pub const CONTRACT_VERSION: &str = "host-resource-telemetry/v1";
/// Revision identifier for the default host pressure thresholds.
pub const POLICY_REVISION: &str = "host-pressure-policy/v1";
/// Maximum number of recent snapshots retained by the service.
pub const MAX_HISTORY: usize = 120;
/// Maximum number of explanatory reasons attached to one snapshot.
pub const MAX_REASONS: usize = 8;
/// Maximum byte length of one pressure reason.
pub const MAX_REASON_BYTES: usize = 96;

/// Collection or validity state for a measured host resource metric.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    /// A valid recent value is available.
    Available,
    /// The collector returned no value.
    Unavailable,
    /// The collector lacks permission to read the metric.
    PermissionDenied,
    /// The host or collector does not support this metric.
    Unsupported,
    /// Collection failed temporarily.
    TransientFailure,
    /// A value exists but is older than the accepted freshness window.
    Stale,
    /// The reported sample failed validation.
    InvalidSample,
    /// Metric collection was not requested or attempted.
    NotCollected,
}

/// Aggregate resource pressure derived from current validated measurements.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    /// Pressure cannot be determined from available measurements.
    Unknown,
    /// Measurements are within normal operating thresholds.
    Normal,
    /// Available memory is below the elevated threshold.
    Elevated,
    /// CPU, memory, or storage crossed a high-pressure threshold.
    High,
    /// Memory or storage crossed a critical threshold.
    Critical,
}

/// One host measurement with provenance, freshness, and availability status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeasuredMetric {
    /// Metric value; absent when the measurement is unavailable.
    pub value: Option<f64>,
    /// Unit used by the value, such as percent or bytes.
    pub unit: String,
    /// Collector or subsystem that produced the sample.
    pub source: String,
    /// Unix timestamp in milliseconds when the sample was observed.
    pub observed_at_ms: i64,
    /// Collector-reported validity horizon in milliseconds.
    pub freshness_ms: u64,
    /// Availability and validity status for the sample.
    pub status: MetricStatus,
}

impl MeasuredMetric {
    /// Creates a metric with no value and the supplied unavailable status.
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

    /// Checks value, provenance lengths, and timestamp bounds.
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

    /// Returns whether an available sample falls within the requested age window.
    pub fn is_current(&self, now_ms: i64, max_age_ms: u64) -> bool {
        self.status == MetricStatus::Available
            && now_ms >= self.observed_at_ms
            && (now_ms - self.observed_at_ms) as u64
                <= max_age_ms.min(self.freshness_ms.max(max_age_ms))
    }
}

/// Bounded snapshot of CPU, memory, storage, and derived pressure.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostResourceSnapshot {
    /// Telemetry contract identifier.
    pub contract_version: String,
    /// Unique identifier for this sample snapshot.
    pub snapshot_id: String,
    /// Unix timestamp in milliseconds for the snapshot.
    pub observed_at_ms: i64,
    /// CPU utilization measurement as a percentage.
    pub cpu_percent: MeasuredMetric,
    /// Available memory percentage.
    pub memory_available_percent: MeasuredMetric,
    /// Free storage percentage for the measured volume.
    pub storage_free_percent: MeasuredMetric,
    /// Pressure level computed from the measurements.
    pub pressure: PressureLevel,
    /// Bounded explanations supporting the pressure level.
    pub reasons: Vec<String>,
}

impl HostResourceSnapshot {
    /// Validates snapshot identity, reasons, and all constituent measurements.
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

/// Thresholds used to derive a pressure level from host measurements.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct PressurePolicy {
    /// Revision identifier for this threshold set.
    pub revision: &'static str,
    /// Maximum acceptable measurement age in milliseconds.
    pub max_age_ms: u64,
    /// Available memory percentage below which pressure is elevated.
    pub memory_elevated_below_percent: f64,
    /// Available memory percentage below which pressure is high.
    pub memory_high_below_percent: f64,
    /// Available memory percentage below which pressure is critical.
    pub memory_critical_below_percent: f64,
    /// Free storage percentage below which pressure is high.
    pub storage_high_below_percent: f64,
    /// Free storage percentage below which pressure is critical.
    pub storage_critical_below_percent: f64,
    /// CPU utilization percentage above which pressure is high.
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

/// Invalid sample or inability to determine pressure from current sensors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TelemetryError {
    /// Sample fields or numeric values violate telemetry bounds.
    #[error("invalid telemetry sample")]
    InvalidSample,
    /// One or more required metrics are stale or unavailable.
    #[error("snapshot is stale or unavailable")]
    UnknownPressure,
}

/// Validates a snapshot and derives pressure using the supplied thresholds.
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

/// Service holding the active pressure policy and bounded recent history.
#[derive(Debug, Clone)]
pub struct HostTelemetryService {
    policy: PressurePolicy,
    history: VecDeque<HostResourceSnapshot>,
}

impl HostTelemetryService {
    /// Creates an empty telemetry service using the supplied pressure policy.
    pub fn new(policy: PressurePolicy) -> Self {
        Self {
            policy,
            history: VecDeque::with_capacity(MAX_HISTORY),
        }
    }
    /// Evaluates and records one sample, evicting the oldest when history is full.
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
    /// Returns the newest retained snapshot, if one has been recorded.
    pub fn latest(&self) -> Option<&HostResourceSnapshot> {
        self.history.back()
    }
    /// Iterates over retained snapshots from oldest to newest.
    pub fn history(&self) -> impl Iterator<Item = &HostResourceSnapshot> {
        self.history.iter()
    }
    /// Returns the policy currently used by this service.
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
