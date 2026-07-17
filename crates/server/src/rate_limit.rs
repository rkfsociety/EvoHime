//! Rate limits + concurrency caps (Stage 7.11).
//!
//! Protects local DoS via mass session/task/worker-job create.
//!
//! Env (0 = disable that limit):
//! - `EVOHIME_RATE_LIMIT_SESSION_PER_MIN` (default 30)
//! - `EVOHIME_RATE_LIMIT_TASK_PER_MIN` (default 60)
//! - `EVOHIME_RATE_LIMIT_WORKER_JOB_PER_MIN` (default 30)
//! - `EVOHIME_MAX_CONCURRENT_TASKS` (default 16)
//! - `EVOHIME_MAX_CONCURRENT_WORKER_JOBS` (default 32)
//! - `EVOHIME_RATE_LIMIT_DISABLED=1` — turn everything off

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

const WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimitBucket {
    SessionCreate,
    TaskStart,
    WorkerJobCreate,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    pub session_per_minute: u32,
    pub task_per_minute: u32,
    pub worker_job_per_minute: u32,
    pub max_concurrent_tasks: usize,
    pub max_concurrent_worker_jobs: usize,
    pub disabled: bool,
}

impl RateLimitConfig {
    pub fn from_env() -> Self {
        let disabled = env_flag_true("EVOHIME_RATE_LIMIT_DISABLED");
        Self {
            session_per_minute: env_u32("EVOHIME_RATE_LIMIT_SESSION_PER_MIN", 30),
            task_per_minute: env_u32("EVOHIME_RATE_LIMIT_TASK_PER_MIN", 60),
            worker_job_per_minute: env_u32("EVOHIME_RATE_LIMIT_WORKER_JOB_PER_MIN", 30),
            max_concurrent_tasks: env_usize("EVOHIME_MAX_CONCURRENT_TASKS", 16),
            max_concurrent_worker_jobs: env_usize("EVOHIME_MAX_CONCURRENT_WORKER_JOBS", 32),
            disabled,
        }
    }

    fn per_minute(&self, bucket: LimitBucket) -> u32 {
        match bucket {
            LimitBucket::SessionCreate => self.session_per_minute,
            LimitBucket::TaskStart => self.task_per_minute,
            LimitBucket::WorkerJobCreate => self.worker_job_per_minute,
        }
    }
}

#[derive(Debug)]
struct WindowCounter {
    started: Instant,
    count: u32,
}

#[derive(Debug)]
pub struct RateLimiter {
    config: RateLimitConfig,
    windows: Mutex<HashMap<LimitBucket, WindowCounter>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateLimitError {
    pub message: String,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl RateLimiter {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            windows: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_env() -> Self {
        Self::new(RateLimitConfig::from_env())
    }

    /// Record one create/start in the sliding fixed window. `Ok(())` if allowed.
    pub fn check_rate(&self, bucket: LimitBucket) -> Result<(), RateLimitError> {
        if self.config.disabled {
            return Ok(());
        }
        let limit = self.config.per_minute(bucket);
        if limit == 0 {
            return Ok(());
        }
        let mut windows = self
            .windows
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = Instant::now();
        let entry = windows.entry(bucket).or_insert_with(|| WindowCounter {
            started: now,
            count: 0,
        });
        if now.duration_since(entry.started) >= WINDOW {
            entry.started = now;
            entry.count = 0;
        }
        if entry.count >= limit {
            return Err(RateLimitError {
                message: format!(
                    "rate limit exceeded for {}: max {limit} per minute",
                    bucket_label(bucket)
                ),
            });
        }
        entry.count += 1;
        Ok(())
    }

    pub fn check_concurrent_tasks(&self, current: usize) -> Result<(), RateLimitError> {
        if self.config.disabled {
            return Ok(());
        }
        let max = self.config.max_concurrent_tasks;
        if max == 0 || current < max {
            Ok(())
        } else {
            Err(RateLimitError {
                message: format!("too many concurrent tasks: max {max}"),
            })
        }
    }

    pub fn check_concurrent_worker_jobs(&self, active: usize) -> Result<(), RateLimitError> {
        if self.config.disabled {
            return Ok(());
        }
        let max = self.config.max_concurrent_worker_jobs;
        if max == 0 || active < max {
            Ok(())
        } else {
            Err(RateLimitError {
                message: format!("too many concurrent worker jobs: max {max}"),
            })
        }
    }

    pub fn allow_session_create(&self) -> Result<(), RateLimitError> {
        self.check_rate(LimitBucket::SessionCreate)
    }

    pub fn allow_task_start(&self, concurrent_tasks: usize) -> Result<(), RateLimitError> {
        self.check_concurrent_tasks(concurrent_tasks)?;
        self.check_rate(LimitBucket::TaskStart)
    }

    pub fn allow_worker_job(&self, active_jobs: usize) -> Result<(), RateLimitError> {
        self.check_concurrent_worker_jobs(active_jobs)?;
        self.check_rate(LimitBucket::WorkerJobCreate)
    }
}

fn bucket_label(bucket: LimitBucket) -> &'static str {
    match bucket {
        LimitBucket::SessionCreate => "session create",
        LimitBucket::TaskStart => "task start",
        LimitBucket::WorkerJobCreate => "worker job create",
    }
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_flag_true(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

/// Sum active worker job counts (`queued` / `running` / `retrying`).
pub fn active_worker_job_count(status_counts: &[(String, i64)]) -> usize {
    status_counts
        .iter()
        .filter(|(status, _)| {
            matches!(
                status.as_str(),
                "queued" | "running" | "retrying"
            )
        })
        .map(|(_, count)| (*count).max(0) as usize)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter(mut config: RateLimitConfig) -> RateLimiter {
        config.disabled = false;
        RateLimiter::new(config)
    }

    #[test]
    fn rate_limit_blocks_after_budget() {
        let limiter = limiter(RateLimitConfig {
            session_per_minute: 2,
            task_per_minute: 60,
            worker_job_per_minute: 30,
            max_concurrent_tasks: 16,
            max_concurrent_worker_jobs: 32,
            disabled: false,
        });
        assert!(limiter.allow_session_create().is_ok());
        assert!(limiter.allow_session_create().is_ok());
        let err = limiter.allow_session_create().expect_err("third blocked");
        assert!(err.message.contains("rate limit"));
    }

    #[test]
    fn zero_rate_disables_that_bucket() {
        let limiter = limiter(RateLimitConfig {
            session_per_minute: 0,
            task_per_minute: 60,
            worker_job_per_minute: 30,
            max_concurrent_tasks: 16,
            max_concurrent_worker_jobs: 32,
            disabled: false,
        });
        for _ in 0..5 {
            assert!(limiter.allow_session_create().is_ok());
        }
    }

    #[test]
    fn concurrency_cap_blocks() {
        let limiter = limiter(RateLimitConfig {
            session_per_minute: 30,
            task_per_minute: 60,
            worker_job_per_minute: 30,
            max_concurrent_tasks: 2,
            max_concurrent_worker_jobs: 1,
            disabled: false,
        });
        assert!(limiter.allow_task_start(0).is_ok());
        assert!(limiter.allow_task_start(1).is_ok());
        let err = limiter.allow_task_start(2).expect_err("at cap");
        assert!(err.message.contains("concurrent tasks"));
        assert!(limiter.allow_worker_job(0).is_ok());
        assert!(limiter.allow_worker_job(1).expect_err("job cap").message.contains("worker"));
    }

    #[test]
    fn disabled_flag_bypasses_all() {
        let limiter = RateLimiter::new(RateLimitConfig {
            session_per_minute: 1,
            task_per_minute: 1,
            worker_job_per_minute: 1,
            max_concurrent_tasks: 1,
            max_concurrent_worker_jobs: 1,
            disabled: true,
        });
        assert!(limiter.allow_session_create().is_ok());
        assert!(limiter.allow_session_create().is_ok());
        assert!(limiter.allow_task_start(100).is_ok());
        assert!(limiter.allow_worker_job(100).is_ok());
    }

    #[test]
    fn active_worker_job_count_sums_inflight() {
        let counts = vec![
            ("completed".into(), 9),
            ("queued".into(), 2),
            ("running".into(), 3),
            ("failed".into(), 1),
            ("retrying".into(), 1),
        ];
        assert_eq!(active_worker_job_count(&counts), 6);
    }
}
