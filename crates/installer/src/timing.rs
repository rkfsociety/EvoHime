use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallationElapsed {
    pub total: Duration,
    pub stage: Duration,
}

#[derive(Debug, Clone)]
pub struct InstallationTiming {
    installation_started: Instant,
    stage_started: Instant,
    finished_at: Option<Instant>,
}

impl InstallationTiming {
    pub fn started(now: Instant) -> Self {
        Self {
            installation_started: now,
            stage_started: now,
            finished_at: None,
        }
    }

    pub fn begin_stage(&mut self, now: Instant) {
        if self.finished_at.is_none() {
            self.stage_started = now;
        }
    }

    pub fn finish(&mut self, now: Instant) {
        if self.finished_at.is_none() {
            self.finished_at = Some(now);
        }
    }

    pub fn elapsed(&self, now: Instant) -> InstallationElapsed {
        let end = self.finished_at.unwrap_or(now);
        InstallationElapsed {
            total: end.saturating_duration_since(self.installation_started),
            stage: end.saturating_duration_since(self.stage_started),
        }
    }
}

pub fn format_elapsed(duration: Duration) -> String {
    let seconds = duration.as_secs();
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_elapsed_resets_without_resetting_total() {
        let start = Instant::now();
        let mut timing = InstallationTiming::started(start);
        timing.begin_stage(start + Duration::from_secs(65));

        let elapsed = timing.elapsed(start + Duration::from_secs(70));
        assert_eq!(elapsed.total, Duration::from_secs(70));
        assert_eq!(elapsed.stage, Duration::from_secs(5));
    }

    #[test]
    fn finish_freezes_both_counters() {
        let start = Instant::now();
        let mut timing = InstallationTiming::started(start);
        timing.begin_stage(start + Duration::from_secs(3));
        timing.finish(start + Duration::from_secs(10));

        let elapsed = timing.elapsed(start + Duration::from_secs(99));
        assert_eq!(elapsed.total, Duration::from_secs(10));
        assert_eq!(elapsed.stage, Duration::from_secs(7));
    }

    #[test]
    fn formats_hours_minutes_and_seconds() {
        assert_eq!(format_elapsed(Duration::from_secs(3_661)), "01:01:01");
    }
}
