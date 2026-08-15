//! Versioned tokenizer/estimator и кэш оценки (этап 01.1).
//!
//! Estimator обязан быть консервативным: его оценка не ниже фактического usage
//! провайдера. Занижение считается дефектом, а не допустимой погрешностью.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::hash::{normalized_bytes, ContentForm};

/// Целевая относительная погрешность на верхней стороне для основного estimator.
pub const MAX_ESTIMATOR_DRIFT: f64 = 0.05;

/// Допустимый over-estimate для fallback-estimator.
pub const MAX_FALLBACK_ESTIMATOR_DRIFT: f64 = 1.00;

/// Правило округления оценки. Округление всегда вверх.
fn ceil_div(value: u64, divisor: u64) -> u32 {
    let divisor = divisor.max(1);
    u32::try_from(value.div_ceil(divisor)).unwrap_or(u32::MAX)
}

/// Контракт оценки размера контекста.
pub trait TokenEstimator: Send + Sync {
    /// Имя estimator (совпадает с семейством токенизатора).
    fn name(&self) -> &str;

    /// Версия оценки. Входит в кэш-ключ и в `context_ledger_hash`.
    fn version(&self) -> &str;

    /// Версия chat-template, применяемого к сообщениям.
    fn chat_template_version(&self) -> &str;

    /// Оценка одного элемента контекста без учёта chat-template.
    fn estimate_content(&self, form: &ContentForm<'_>) -> u32;

    /// Накладные расходы chat-template на одно сообщение.
    fn message_overhead(&self) -> u32;

    /// Оценка одной tool schema, включая её overhead.
    fn estimate_tool_schema(&self, schema_json: &str) -> u32;

    /// Является ли estimator консервативным fallback'ом.
    fn is_fallback(&self) -> bool {
        false
    }
}

/// Основной model-specific estimator. Пока провайдеры не отдают собственный
/// токенизатор, используется эвристика с явно заявленной консервативностью:
/// коэффициент байт-на-токен подобран так, чтобы оценка не опускалась ниже
/// фактического usage на фикстурах.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeuristicEstimator {
    name: String,
    version: String,
    chat_template_version: String,
    /// Число байт UTF-8, приходящихся на один токен.
    bytes_per_token: u32,
    /// Фиксированная надбавка на элемент.
    per_item_overhead: u32,
    message_overhead: u32,
    /// Надбавка на одну tool schema.
    tool_schema_overhead: u32,
}

impl HeuristicEstimator {
    /// Профиль оценки по умолчанию: 3 байта на токен плюс фиксированные
    /// надбавки. Для латиницы это заметный over-estimate, для кириллицы и
    /// CJK — граница, ниже которой фактический usage не опускается.
    pub fn default_for(model: &str) -> Self {
        Self {
            name: format!("heuristic/{model}"),
            version: "heuristic-1".to_string(),
            chat_template_version: "chat-1".to_string(),
            bytes_per_token: 3,
            per_item_overhead: 8,
            message_overhead: 4,
            tool_schema_overhead: 16,
        }
    }
}

impl TokenEstimator for HeuristicEstimator {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn chat_template_version(&self) -> &str {
        &self.chat_template_version
    }

    fn estimate_content(&self, form: &ContentForm<'_>) -> u32 {
        let bytes = normalized_bytes(form).len() as u64;
        ceil_div(bytes, u64::from(self.bytes_per_token)).saturating_add(self.per_item_overhead)
    }

    fn message_overhead(&self) -> u32 {
        self.message_overhead
    }

    fn estimate_tool_schema(&self, schema_json: &str) -> u32 {
        ceil_div(schema_json.len() as u64, u64::from(self.bytes_per_token))
            .saturating_add(self.tool_schema_overhead)
    }
}

/// Консервативный fallback-estimator. Спецификация зафиксирована планом:
/// `estimated_tokens = ceil(utf8_bytes / 2) + 16`, 8 токенов на сообщение
/// chat-template и `ceil(utf8_bytes(schema) / 2)` на tool-schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FallbackEstimator;

impl TokenEstimator for FallbackEstimator {
    fn name(&self) -> &str {
        "fallback"
    }

    fn version(&self) -> &str {
        "fallback-1"
    }

    fn chat_template_version(&self) -> &str {
        "fallback-chat-1"
    }

    fn estimate_content(&self, form: &ContentForm<'_>) -> u32 {
        ceil_div(normalized_bytes(form).len() as u64, 2).saturating_add(16)
    }

    fn message_overhead(&self) -> u32 {
        8
    }

    fn estimate_tool_schema(&self, schema_json: &str) -> u32 {
        ceil_div(schema_json.len() as u64, 2)
    }

    fn is_fallback(&self) -> bool {
        true
    }
}

/// Ключ кэша оценки: `content_hash` + версии tokenizer, нормализатора и
/// chat-template. Смена любого из компонентов не даёт стухший кэш-хит.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EstimateCacheKey {
    pub content_hash: String,
    pub tokenizer_version: String,
    pub normalizer_version: String,
    pub chat_template_version: String,
}

/// Кэш оценки для неизменных item в пределах сборки и между сборками.
#[derive(Debug, Default)]
pub struct EstimateCache {
    entries: HashMap<EstimateCacheKey, u32>,
    hits: u64,
    misses: u64,
}

impl EstimateCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Возвращает оценку из кэша либо вычисляет её и запоминает.
    pub fn estimate(
        &mut self,
        estimator: &dyn TokenEstimator,
        content_hash: &str,
        normalizer_version: &str,
        form: &ContentForm<'_>,
    ) -> u32 {
        let key = EstimateCacheKey {
            content_hash: content_hash.to_string(),
            tokenizer_version: estimator.version().to_string(),
            normalizer_version: normalizer_version.to_string(),
            chat_template_version: estimator.chat_template_version().to_string(),
        };
        if let Some(cached) = self.entries.get(&key) {
            self.hits += 1;
            return *cached;
        }
        self.misses += 1;
        let estimated = estimator.estimate_content(form);
        self.entries.insert(key, estimated);
        estimated
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Расхождение оценки с фактическим usage провайдера.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EstimatorDrift {
    pub estimated_prompt_tokens: u32,
    pub actual_prompt_tokens: u32,
    /// Относительная погрешность: положительная — over-estimate.
    pub relative: f64,
}

impl EstimatorDrift {
    pub fn measure(estimated_prompt_tokens: u32, actual_prompt_tokens: u32) -> Self {
        let relative = if actual_prompt_tokens == 0 {
            0.0
        } else {
            (f64::from(estimated_prompt_tokens) - f64::from(actual_prompt_tokens))
                / f64::from(actual_prompt_tokens)
        };
        Self {
            estimated_prompt_tokens,
            actual_prompt_tokens,
            relative,
        }
    }

    /// Занижение оценки — дефект, а не допустимая погрешность.
    pub fn is_under_estimate(&self) -> bool {
        self.estimated_prompt_tokens < self.actual_prompt_tokens
    }

    /// Превышен ли допустимый over-estimate для данного режима.
    pub fn exceeds_budget(&self, fallback_mode: bool) -> bool {
        let limit = if fallback_mode {
            MAX_FALLBACK_ESTIMATOR_DRIFT
        } else {
            MAX_ESTIMATOR_DRIFT
        };
        self.relative > limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::NORMALIZER_VERSION;

    #[test]
    fn fallback_estimator_matches_the_declared_formula() {
        let estimator = FallbackEstimator;
        // "abcd" — 4 байта: ceil(4/2) + 16 = 18.
        assert_eq!(estimator.estimate_content(&ContentForm::Text("abcd")), 18);
        assert_eq!(estimator.message_overhead(), 8);
        assert_eq!(estimator.estimate_tool_schema("{}"), 1);
    }

    #[test]
    fn fallback_estimator_rounds_up() {
        let estimator = FallbackEstimator;
        // 5 байт: ceil(5/2) = 3.
        assert_eq!(estimator.estimate_content(&ContentForm::Text("abcde")), 19);
    }

    #[test]
    fn fallback_estimate_never_falls_below_a_realistic_usage() {
        let estimator = FallbackEstimator;
        // Худший реалистичный случай — примерно 1 токен на 2 байта UTF-8.
        for text in [
            "короткий текст",
            "a much longer english sentence with many words",
            "混合したテキストとASCII",
        ] {
            let estimated = estimator.estimate_content(&ContentForm::Text(text));
            let worst_case_actual = ceil_div(text.len() as u64, 2);
            assert!(
                estimated >= worst_case_actual,
                "{text}: {estimated} < {worst_case_actual}"
            );
        }
    }

    #[test]
    fn cache_reuses_estimates_for_unchanged_items() {
        let estimator = HeuristicEstimator::default_for("m");
        let mut cache = EstimateCache::new();
        let form = ContentForm::Text("payload");
        let first = cache.estimate(&estimator, "hash", NORMALIZER_VERSION, &form);
        let second = cache.estimate(&estimator, "hash", NORMALIZER_VERSION, &form);
        assert_eq!(first, second);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn cache_is_invalidated_by_tokenizer_normalizer_and_template_versions() {
        let primary = HeuristicEstimator::default_for("m");
        let mut cache = EstimateCache::new();
        let form = ContentForm::Text("payload");
        cache.estimate(&primary, "hash", NORMALIZER_VERSION, &form);

        // Другой tokenizer_version.
        cache.estimate(&FallbackEstimator, "hash", NORMALIZER_VERSION, &form);
        // Другой normalizer_version.
        cache.estimate(&primary, "hash", "norm-2", &form);
        // Другая версия chat-template.
        let mut other_template = primary.clone();
        other_template.chat_template_version = "chat-2".to_string();
        cache.estimate(&other_template, "hash", NORMALIZER_VERSION, &form);

        assert_eq!(cache.misses(), 4);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), 4);
    }

    #[test]
    fn drift_detects_under_estimate_as_a_defect() {
        let drift = EstimatorDrift::measure(90, 100);
        assert!(drift.is_under_estimate());
        assert!(drift.relative < 0.0);
    }

    #[test]
    fn drift_over_five_percent_is_reported_for_the_primary_estimator() {
        let drift = EstimatorDrift::measure(110, 100);
        assert!(!drift.is_under_estimate());
        assert!(drift.exceeds_budget(false));
        assert!(!drift.exceeds_budget(true));
    }
}
