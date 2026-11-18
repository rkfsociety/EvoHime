//! Process-local memory of the last provider rate-limit window.
//!
//! The gateway records the cooldown it learned from a provider reply so a
//! background loop does not have to repeat the failed request to discover the
//! same limit. The signal is intentionally process-local and bounded: it is a
//! scheduling hint, never a durable budget or an authorization decision.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Maximum number of model signals kept in memory.
const MAX_SIGNALS: usize = 64;

fn registry() -> &'static Mutex<HashMap<String, i64>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records that `model` should not be called again before `until_ms`.
///
/// The entry is dropped as soon as the window is behind the local clock.
pub fn record_rate_limit(model: &str, until_ms: i64) {
    let model = model.trim();
    if model.is_empty() {
        return;
    }
    let now = now_ms();
    if until_ms <= now {
        clear_rate_limit(model);
        return;
    }
    let Ok(mut signals) = registry().lock() else {
        return;
    };
    signals.retain(|_, until| *until > now);
    if signals.len() >= MAX_SIGNALS && !signals.contains_key(model) {
        if let Some(oldest) = signals
            .iter()
            .min_by_key(|(_, until)| **until)
            .map(|(key, _)| key.clone())
        {
            signals.remove(&oldest);
        }
    }
    signals.insert(model.to_owned(), until_ms);
}

/// Forgets a recorded window after a successful request.
pub fn clear_rate_limit(model: &str) {
    let Ok(mut signals) = registry().lock() else {
        return;
    };
    signals.remove(model.trim());
}

/// Returns the remaining rate-limit window for `model`, if one is still active.
pub fn rate_limited_until(model: &str) -> Option<i64> {
    let now = now_ms();
    let Ok(mut signals) = registry().lock() else {
        return None;
    };
    signals.retain(|_, until| *until > now);
    signals.get(model.trim()).copied()
}

/// Returns a snapshot of all currently active model cooldown deadlines in Unix milliseconds.
pub fn active_rate_limits() -> HashMap<String, i64> {
    let now = now_ms();
    let Ok(mut signals) = registry().lock() else {
        return HashMap::new();
    };
    signals.retain(|_, until| *until > now);
    signals.clone()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

/// Records a cooldown relative to the current local clock.
pub(crate) fn record_rate_limit_for(model: &str, duration: std::time::Duration) {
    let delta = i64::try_from(duration.as_millis()).unwrap_or(i64::MAX);
    record_rate_limit(model, now_ms().saturating_add(delta));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_clears_and_expires_windows() {
        let model = "rate-limit-test-model";
        clear_rate_limit(model);
        assert_eq!(rate_limited_until(model), None);

        record_rate_limit_for(model, std::time::Duration::from_secs(60));
        assert!(rate_limited_until(model).is_some());

        clear_rate_limit(model);
        assert_eq!(rate_limited_until(model), None);

        record_rate_limit_for(model, std::time::Duration::ZERO);
        assert_eq!(rate_limited_until(model), None);
    }

    #[test]
    fn blank_models_and_past_windows_are_ignored() {
        record_rate_limit("", 1);
        assert_eq!(rate_limited_until(""), None);
        record_rate_limit("past-window-model", 1);
        assert_eq!(rate_limited_until("past-window-model"), None);
    }
}
