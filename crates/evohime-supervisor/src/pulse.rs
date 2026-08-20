//! Truthful local Pulse digest for schedules and recovery.

// Модуль — bounded-контракт: часть его поверхности сегодня вызывается только
// собственными тестами, а вызовы из supervisor появятся при wiring этапа,
// которому контракт принадлежит. Удалять её нельзя — это и есть описанный в
// планах интерфейс.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PulseHealth {
    Healthy,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PulseDigest {
    pub health: PulseHealth,
    pub completed: u32,
    pub missed: u32,
    pub failed: u32,
    pub dead_lettered: u32,
    pub requeued: u32,
    pub summary: String,
}

impl PulseDigest {
    pub fn from_events(events: &[&str]) -> Self {
        let count = |name: &str| events.iter().filter(|event| **event == name).count() as u32;
        let failed = count("runtime.schedule_failed");
        let dead_lettered = count("runtime.schedule_dead_letter");
        let missed = count("runtime.trigger_missed");
        let completed = count("runtime.schedule_completed");
        let requeued = count("runtime.schedule_requeued");
        let health = if dead_lettered > 0 {
            PulseHealth::Failed
        } else if failed > 0 || missed > 0 {
            PulseHealth::Degraded
        } else {
            PulseHealth::Healthy
        };
        let summary = match health {
            PulseHealth::Healthy => "Все локальные запуски завершены штатно".to_string(),
            PulseHealth::Degraded => "Есть пропущенные или неуспешные запуски".to_string(),
            PulseHealth::Failed => "Есть dead-letter запуски: требуется ручное решение".to_string(),
        };
        Self {
            health,
            completed,
            missed,
            failed,
            dead_lettered,
            requeued,
            summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_never_masks_failure_as_success() {
        let digest =
            PulseDigest::from_events(&["runtime.schedule_completed", "runtime.schedule_failed"]);
        assert_eq!(digest.health, PulseHealth::Degraded);
        assert!(digest.summary.contains("неуспешные"));
    }
}
