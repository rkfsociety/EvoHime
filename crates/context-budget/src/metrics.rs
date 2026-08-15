//! Метрики и пороги alerting этапа 01.1.
//!
//! Диагностика намеренно не содержит сырой prompt, тело памяти или raw tool
//! output: только ids, counts, hashes, policy labels, bounded reasons и
//! числовые показатели.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::budget::BudgetUnavailableStage;
use crate::item::{BudgetCategory, DropReason};
use crate::ladder::LadderLevel;

/// Окно, на котором считаются пороги alerting.
pub const ALERT_WINDOW_CALLS: usize = 100;
/// Порог p95 относительной погрешности оценки.
pub const ALERT_ESTIMATOR_DRIFT_P95: f64 = 0.05;
/// Порог доли вызовов с re-plan.
pub const ALERT_REPLAN_SHARE: f64 = 0.01;

/// Счётчики этапа. Значения агрегируются Core и выгружаются в observability.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextMetrics {
    /// `context_drops_total{reason}`.
    pub drops_total: BTreeMap<String, u64>,
    /// `context_budget_utilization` по категориям: наблюдения доли бюджета.
    pub budget_utilization: BTreeMap<String, Vec<f64>>,
    /// `context_estimator_drift`: наблюдения относительной погрешности.
    pub estimator_drift: Vec<f64>,
    /// `context_ladder_level_applied_total{level}`.
    pub ladder_level_applied_total: BTreeMap<String, u64>,
    /// `context_replan_total{outcome}`.
    pub replan_total: BTreeMap<String, u64>,
    /// `context_budget_unavailable_total{stage}`.
    pub budget_unavailable_total: BTreeMap<String, u64>,
    /// `context_selection_latency_ms`.
    pub selection_latency_ms: Vec<u64>,
    /// `context_offloaded_bytes_total`.
    pub offloaded_bytes_total: u64,
    /// `context_ledger_pruned_total`.
    pub ledger_pruned_total: u64,
    /// Число зафиксированных занижений оценки. Любое значение > 0 — alert.
    pub estimator_under_estimate_total: u64,
    /// Число неудачных записей ledger. Любое значение > 0 — alert.
    pub ledger_write_failed_total: u64,
    /// Число изолированных `recovered` записей scratchpad (01.2).
    pub recovery_items_isolated_total: u64,
    /// Число отклонённых вызовов инструментов вне loadout (01.4).
    pub loadout_miss_total: u64,
    /// Общее число сборок контекста, попавших в окно.
    pub calls_total: u64,
}

impl ContextMetrics {
    pub fn record_drop(&mut self, reason: DropReason) {
        *self
            .drops_total
            .entry(reason.as_str().to_string())
            .or_default() += 1;
    }

    pub fn record_ladder_level(&mut self, level: LadderLevel) {
        *self
            .ladder_level_applied_total
            .entry(level.as_str().to_string())
            .or_default() += 1;
    }

    pub fn record_budget_unavailable(&mut self, stage: BudgetUnavailableStage) {
        *self
            .budget_unavailable_total
            .entry(stage.as_str().to_string())
            .or_default() += 1;
    }

    pub fn record_replan(&mut self, outcome: &str) {
        *self.replan_total.entry(outcome.to_string()).or_default() += 1;
    }

    pub fn record_utilization(&mut self, category: BudgetCategory, used: u32, budget: u32) {
        if budget == 0 {
            return;
        }
        self.budget_utilization
            .entry(category.as_str().to_string())
            .or_default()
            .push(f64::from(used) / f64::from(budget));
    }

    pub fn record_estimator_drift(&mut self, relative: f64) {
        self.estimator_drift.push(relative);
        if relative < 0.0 {
            self.estimator_under_estimate_total += 1;
        }
    }

    pub fn record_selection_latency(&mut self, millis: u64) {
        self.selection_latency_ms.push(millis);
    }

    /// p95 наблюдений. Возвращает `None` для пустого набора.
    pub fn p95(values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let index = ((sorted.len() as f64) * 0.95).ceil() as usize;
        sorted.get(index.saturating_sub(1)).copied()
    }

    /// Активные alert'ы по объявленным порогам.
    pub fn alerts(&self) -> Vec<String> {
        let mut alerts = Vec::new();
        let window: Vec<f64> = self
            .estimator_drift
            .iter()
            .rev()
            .take(ALERT_WINDOW_CALLS)
            .copied()
            .collect();
        if let Some(p95) = Self::p95(&window) {
            if p95 > ALERT_ESTIMATOR_DRIFT_P95 {
                alerts.push(format!("estimator_drift_p95={p95:.4}"));
            }
        }
        if self.estimator_under_estimate_total > 0 {
            alerts.push(format!(
                "estimator_under_estimate={}",
                self.estimator_under_estimate_total
            ));
        }
        let replans: u64 = self.replan_total.values().sum();
        let window_calls = self.calls_total.min(ALERT_WINDOW_CALLS as u64);
        if window_calls > 0 {
            let share = replans as f64 / window_calls as f64;
            if share > ALERT_REPLAN_SHARE {
                alerts.push(format!("replan_share={share:.4}"));
            }
        }
        if let Some(count) = self
            .budget_unavailable_total
            .get(BudgetUnavailableStage::EstimatorUnavailable.as_str())
        {
            if *count > 0 {
                alerts.push(format!("estimator_unavailable={count}"));
            }
        }
        if self.ledger_write_failed_total > 0 {
            alerts.push(format!(
                "ledger_write_failed={}",
                self.ledger_write_failed_total
            ));
        }
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p95_of_a_uniform_window_is_the_top_observation() {
        let values: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(ContextMetrics::p95(&values), Some(95.0));
    }

    #[test]
    fn under_estimate_always_raises_an_alert() {
        let mut metrics = ContextMetrics {
            calls_total: 100,
            ..Default::default()
        };
        metrics.record_estimator_drift(-0.01);
        assert!(metrics
            .alerts()
            .iter()
            .any(|alert| alert.starts_with("estimator_under_estimate=")));
    }

    #[test]
    fn drift_within_five_percent_raises_no_alert() {
        let mut metrics = ContextMetrics {
            calls_total: 100,
            ..Default::default()
        };
        for _ in 0..100 {
            metrics.record_estimator_drift(0.02);
        }
        assert!(metrics.alerts().is_empty(), "{:?}", metrics.alerts());
    }

    #[test]
    fn replan_share_above_one_percent_raises_an_alert() {
        let mut metrics = ContextMetrics {
            calls_total: 100,
            ..Default::default()
        };
        metrics.record_replan("failed");
        metrics.record_replan("succeeded");
        assert!(metrics
            .alerts()
            .iter()
            .any(|alert| alert.starts_with("replan_share=")));
    }

    #[test]
    fn estimator_unavailable_always_raises_an_alert() {
        let mut metrics = ContextMetrics {
            calls_total: 10,
            ..Default::default()
        };
        metrics.record_budget_unavailable(BudgetUnavailableStage::EstimatorUnavailable);
        assert!(metrics
            .alerts()
            .iter()
            .any(|alert| alert.starts_with("estimator_unavailable=")));
    }
}
