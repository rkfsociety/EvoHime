//! Static ranking and cooldown-aware fallback selection for free LiteRouter models.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Ranked list of free LiteRouter models ordered from highest to lowest preference.
const RANKED_FREE_MODELS: &[(u32, &str)] = &[
    (1, "llama-3.3-70b-instruct-turbo-free"),
    (2, "llama-3.1-8b-instruct-free"),
    (3, "qwen-2.5-72b-instruct-free"),
    (4, "deepseek-r1-free"),
    (5, "gemma-2-27b-it-free"),
];

/// Returns the static list of free LiteRouter models sorted by rank (best first).
pub fn get_ranked_models() -> Vec<String> {
    let mut entries: Vec<(u32, &str)> = RANKED_FREE_MODELS.to_vec();
    entries.sort_by_key(|(rank, _)| *rank);
    entries
        .into_iter()
        .map(|(_, model)| model.to_owned())
        .collect()
}

/// Returns the highest-ranked model other than `current` that has no active cooldown.
///
/// A model is considered on cooldown when `cooldowns.get(model)` holds a Unix
/// millisecond timestamp strictly greater than the current wall-clock time.
pub fn next_available_model(current: &str, cooldowns: &HashMap<String, i64>) -> Option<String> {
    let now = now_ms();
    let current = current.trim();
    get_ranked_models().into_iter().find(|model| {
        model != current
            && cooldowns
                .get(model.as_str())
                .copied()
                .is_none_or(|until_ms| until_ms <= now)
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranked_models_start_with_llama_3_3_70b_free() {
        let models = get_ranked_models();
        assert!(!models.is_empty());
        assert_eq!(models[0], "llama-3.3-70b-instruct-turbo-free");
        assert_eq!(models[1], "llama-3.1-8b-instruct-free");
    }

    #[test]
    fn next_available_model_skips_current_and_active_cooldowns() {
        let now = now_ms();
        let cooldowns = HashMap::from([
            ("llama-3.3-70b-instruct-turbo-free".to_owned(), now + 60_000),
            ("llama-3.1-8b-instruct-free".to_owned(), now + 60_000),
        ]);
        let next = next_available_model("llama-3.3-70b-instruct-turbo-free", &cooldowns);
        assert_eq!(next.as_deref(), Some("qwen-2.5-72b-instruct-free"));
    }

    #[test]
    fn next_available_model_returns_none_when_all_others_are_cooling_down() {
        let now = now_ms();
        let cooldowns: HashMap<String, i64> = get_ranked_models()
            .into_iter()
            .map(|model| (model, now + 60_000))
            .collect();
        assert_eq!(
            next_available_model("llama-3.3-70b-instruct-turbo-free", &cooldowns),
            None
        );
    }
}
