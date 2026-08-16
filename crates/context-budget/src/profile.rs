//! `ModelContextProfile`, каталог профилей и бюджетная арифметика (этап 01.1).

use serde::{Deserialize, Serialize};

/// `schema_version` контракта профиля.
pub const MODEL_CONTEXT_PROFILE_SCHEMA_VERSION: u32 = 1;

/// Версия стратегии сборки контекста. Входит в `context_ledger_hash`.
pub const STRATEGY_VERSION: &str = "strategy-1";

/// Каталог профилей, поставляемый со сборкой Core.
const BUILTIN_CATALOG: &str = include_str!("../profiles.json");

/// Профиль модели. Все значения — в токенах, кроме `offload_threshold_bytes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelContextProfile {
    pub schema_version: u32,
    /// Версия профиля. Любое изменение значений — новый `profile_version`:
    /// правка «на месте» запрещена, потому что версия входит в hash ledger.
    pub profile_version: String,
    pub provider: String,
    pub model: String,
    pub max_context_tokens: u32,
    pub target_tokens: u32,
    pub soft_limit_tokens: u32,
    pub hard_limit_tokens: u32,
    pub absolute_mvc_max_limit: u32,
    pub tool_schema_reserve: u32,
    pub tool_call_reserve: u32,
    pub final_answer_reserve: u32,
    pub streaming_reserve: u32,
    pub retry_reserve: u32,
    pub low_priority_cutoff: u8,
    pub offload_threshold_bytes: u64,
}

/// Нарушение правил валидности профиля. Невалидный профиль не используется.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("profile {profile_version}: ordering violated ({detail})")]
    Ordering {
        profile_version: String,
        detail: String,
    },
    #[error("profile {profile_version}: target_tokens + reserves_total > soft_limit_tokens ({target} + {reserves} > {soft})")]
    TargetWithReserves {
        profile_version: String,
        target: u32,
        reserves: u32,
        soft: u32,
    },
    #[error("profile {profile_version}: absolute_mvc_max_limit + reserves_total > hard_limit_tokens ({mvc} + {reserves} > {hard})")]
    MvcWithReserves {
        profile_version: String,
        mvc: u32,
        reserves: u32,
        hard: u32,
    },
}

impl ModelContextProfile {
    /// `reserves_total = tool_schema + tool_call + final_answer + streaming + retry`.
    pub fn reserves_total(&self) -> u32 {
        self.tool_schema_reserve
            .saturating_add(self.tool_call_reserve)
            .saturating_add(self.final_answer_reserve)
            .saturating_add(self.streaming_reserve)
            .saturating_add(self.retry_reserve)
    }

    /// Проверка правил валидности. Выполняется при загрузке, а не в момент сборки.
    pub fn validate(&self) -> Result<(), ProfileError> {
        let reserves = self.reserves_total();
        if !(0 < self.target_tokens
            && self.target_tokens < self.soft_limit_tokens
            && self.soft_limit_tokens < self.hard_limit_tokens
            && self.hard_limit_tokens <= self.max_context_tokens)
        {
            return Err(ProfileError::Ordering {
                profile_version: self.profile_version.clone(),
                detail: format!(
                    "0 < {} < {} < {} <= {}",
                    self.target_tokens,
                    self.soft_limit_tokens,
                    self.hard_limit_tokens,
                    self.max_context_tokens
                ),
            });
        }
        if self.target_tokens.saturating_add(reserves) > self.soft_limit_tokens {
            return Err(ProfileError::TargetWithReserves {
                profile_version: self.profile_version.clone(),
                target: self.target_tokens,
                reserves,
                soft: self.soft_limit_tokens,
            });
        }
        if self.absolute_mvc_max_limit.saturating_add(reserves) > self.hard_limit_tokens {
            return Err(ProfileError::MvcWithReserves {
                profile_version: self.profile_version.clone(),
                mvc: self.absolute_mvc_max_limit,
                reserves,
                hard: self.hard_limit_tokens,
            });
        }
        Ok(())
    }

    /// Верхняя граница необязательной части:
    /// `hard_limit_tokens - reserves_total - mandatory_tokens`.
    pub fn optional_ceiling(&self, mandatory_tokens: u32) -> u32 {
        self.hard_limit_tokens
            .saturating_sub(self.reserves_total())
            .saturating_sub(mandatory_tokens)
    }

    /// Базовый fallback-профиль для неизвестной модели: `target=60%`,
    /// `soft=75%`, `hard=85%` от заявленного окна, минимум 1024 токена под
    /// tool-call и 2048 под final answer. Значения зажимаются так, чтобы
    /// правила валидности выполнялись и для маленьких окон; неизвестная модель
    /// не может обойти эти ограничения.
    pub fn fallback(provider: &str, model: &str, max_context_tokens: u32) -> Self {
        let max = max_context_tokens.max(1);
        let hard = percent(max, 85).max(1);
        let soft = percent(max, 75).max(1);

        let tool_call_reserve = 1024.max(percent(max, 1));
        let final_answer_reserve = 2048.max(percent(max, 4));
        let streaming_reserve = 512;
        let retry_reserve = 1024;
        let tool_schema_reserve = percent(max, 8).clamp(1024, 8192);
        let reserves = tool_schema_reserve
            + tool_call_reserve
            + final_answer_reserve
            + streaming_reserve
            + retry_reserve;

        let target = percent(max, 60).min(soft.saturating_sub(reserves));
        let absolute_mvc_max_limit =
            percent(max, 40).min(hard.saturating_sub(reserves));

        Self {
            schema_version: MODEL_CONTEXT_PROFILE_SCHEMA_VERSION,
            profile_version: format!("fallback-1/{max}"),
            provider: provider.to_string(),
            model: model.to_string(),
            max_context_tokens: max,
            target_tokens: target,
            soft_limit_tokens: soft,
            hard_limit_tokens: hard,
            absolute_mvc_max_limit,
            tool_schema_reserve,
            tool_call_reserve,
            final_answer_reserve,
            streaming_reserve,
            retry_reserve,
            low_priority_cutoff: crate::item::DEFAULT_LOW_PRIORITY_CUTOFF,
            offload_threshold_bytes: 32 * 1024,
        }
    }

    /// Профиль с масштабированными порогами для fallback-estimator:
    /// `hard/soft/target` умножаются на 0.70, резервы не уменьшаются.
    /// Версия профиля меняется, потому что меняются значения.
    pub fn scaled_for_fallback_estimator(&self) -> Self {
        let mut scaled = self.clone();
        scaled.profile_version = format!("{}+fallback-estimator", self.profile_version);
        scaled.hard_limit_tokens = percent(self.hard_limit_tokens, 70);
        scaled.soft_limit_tokens = percent(self.soft_limit_tokens, 70);
        scaled.target_tokens = percent(self.target_tokens, 70);
        scaled.absolute_mvc_max_limit = self
            .absolute_mvc_max_limit
            .min(scaled.hard_limit_tokens.saturating_sub(self.reserves_total()));
        scaled
    }

    /// Deterministic re-plan после context-length error провайдера:
    /// `hard_limit_tokens` уменьшается до
    /// `min(provider_window, floor(previous_hard_limit * 0.9))`, необязательные
    /// резервы сокращаются в заранее заданном порядке. Обязательная часть
    /// контекста не меняется.
    pub fn replan(&self, provider_window: Option<u32>) -> Self {
        let mut next = self.clone();
        next.profile_version = format!("{}+replan", self.profile_version);
        let reduced = percent(self.hard_limit_tokens, 90);
        next.hard_limit_tokens = match provider_window {
            Some(window) => window.min(reduced),
            None => reduced,
        }
        .max(1);
        // Необязательные резервы сокращаются в фиксированном порядке;
        // tool_call_reserve и final_answer_reserve не трогаются никогда.
        next.retry_reserve = 0;
        next.streaming_reserve = 0;
        next.soft_limit_tokens = next
            .soft_limit_tokens
            .min(next.hard_limit_tokens.saturating_sub(1));
        next.target_tokens = next
            .target_tokens
            .min(next.soft_limit_tokens.saturating_sub(next.reserves_total()));
        next.absolute_mvc_max_limit = next
            .absolute_mvc_max_limit
            .min(next.hard_limit_tokens.saturating_sub(next.reserves_total()));
        next.max_context_tokens = next.max_context_tokens.min(
            provider_window.unwrap_or(next.max_context_tokens),
        );
        next
    }
}

fn percent(value: u32, percent: u32) -> u32 {
    ((u64::from(value) * u64::from(percent)) / 100) as u32
}

/// Запись каталога: профиль плюс правило сопоставления provider/model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Префикс имени модели; пустая строка совпадает с любой моделью провайдера.
    #[serde(default)]
    pub model_prefix: String,
    #[serde(flatten)]
    pub profile: ModelContextProfile,
}

/// Каталог профилей. Загружается из декларативного файла, поставляемого со
/// сборкой Core, и может перекрываться пользовательским конфигом того же формата.
#[derive(Debug, Clone, Default)]
pub struct ProfileCatalog {
    entries: Vec<CatalogEntry>,
    /// Отклонённые записи с причиной — попадают в diagnostic, а не роняют загрузку.
    rejected: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct CatalogFile {
    #[serde(default)]
    entries: Vec<CatalogEntry>,
}

impl ProfileCatalog {
    /// Каталог, поставляемый со сборкой Core.
    pub fn builtin() -> Self {
        Self::from_json(BUILTIN_CATALOG).unwrap_or_default()
    }

    /// Разбор каталога. Невалидные профили отклоняются с diagnostic, остальные
    /// записи остаются доступными.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let file: CatalogFile = serde_json::from_str(json)?;
        let mut catalog = Self::default();
        for entry in file.entries {
            match entry.profile.validate() {
                Ok(()) => catalog.entries.push(entry),
                Err(error) => catalog
                    .rejected
                    .push((entry.profile.profile_version.clone(), error.to_string())),
            }
        }
        Ok(catalog)
    }

    /// Наложение пользовательского каталога: записи с тем же provider/prefix
    /// заменяются, новые добавляются.
    pub fn overlay(&mut self, other: Self) {
        for entry in other.entries {
            match self
                .entries
                .iter_mut()
                .find(|slot| {
                    slot.profile.provider == entry.profile.provider
                        && slot.model_prefix == entry.model_prefix
                })
            {
                Some(slot) => *slot = entry,
                None => self.entries.push(entry),
            }
        }
        self.rejected.extend(other.rejected);
    }

    /// Отклонённые при загрузке профили: `(profile_version, причина)`.
    pub fn rejected(&self) -> &[(String, String)] {
        &self.rejected
    }

    /// Выбор профиля по provider/model. Побеждает самое длинное совпадение
    /// префикса модели; при отсутствии совпадения используется fallback-профиль.
    pub fn resolve(
        &self,
        provider: &str,
        model: &str,
        provider_window: Option<u32>,
    ) -> ModelContextProfile {
        let matched = self
            .entries
            .iter()
            .filter(|entry| {
                entry.profile.provider == provider
                    && (entry.model_prefix.is_empty() || model.starts_with(&entry.model_prefix))
            })
            .max_by_key(|entry| entry.model_prefix.len());
        match matched {
            // Встроенный профиль — предположение, а окно из каталога провайдера
            // — факт. Пока факта нет, побеждает профиль; как только провайдер
            // назвал окно и оно расходится с профилем, бюджет пересчитывается
            // под реальное окно по тем же пропорциям.
            Some(entry)
                if provider_window
                    .is_none_or(|window| window == entry.profile.max_context_tokens) =>
            {
                let mut profile = entry.profile.clone();
                profile.model = model.to_string();
                profile
            }
            Some(_) | None => ModelContextProfile::fallback(
                provider,
                model,
                provider_window.unwrap_or(DEFAULT_UNKNOWN_WINDOW),
            ),
        }
    }
}

/// Окно, которое предполагается для неизвестной модели без подсказки провайдера.
pub const DEFAULT_UNKNOWN_WINDOW: u32 = 32_768;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_profiles_are_valid() {
        let catalog = ProfileCatalog::builtin();
        assert!(catalog.rejected().is_empty(), "{:?}", catalog.rejected());
        assert!(!catalog.entries.is_empty());
        for entry in &catalog.entries {
            entry.profile.validate().expect("builtin profile is valid");
        }
    }

    #[test]
    fn fallback_profile_is_valid_for_every_usable_window() {
        for window in [8_192_u32, 16_384, 32_768, 128_000, 1_000_000] {
            let profile = ModelContextProfile::fallback("unknown", "model", window);
            profile
                .validate()
                .unwrap_or_else(|error| panic!("window {window}: {error}"));
        }
    }

    #[test]
    fn a_window_too_small_for_the_declared_reserves_yields_an_invalid_profile() {
        // 1024 токена под tool-call и 2048 под final answer физически не
        // помещаются в окно 4096. Профиль не «подгоняется» молча: он не проходит
        // валидацию, и сборка обязана завершиться отказом.
        let profile = ModelContextProfile::fallback("unknown", "model", 4_096);
        assert!(profile.validate().is_err());
    }

    #[test]
    fn fallback_profile_keeps_declared_ratios_for_a_large_window() {
        let profile = ModelContextProfile::fallback("unknown", "model", 128_000);
        assert_eq!(profile.hard_limit_tokens, 108_800);
        assert_eq!(profile.soft_limit_tokens, 96_000);
        assert_eq!(profile.target_tokens, 76_800);
        assert!(profile.tool_call_reserve >= 1024);
        assert!(profile.final_answer_reserve >= 2048);
        assert_eq!(profile.absolute_mvc_max_limit, 51_200);
    }

    #[test]
    fn invalid_profile_is_rejected_at_load_time() {
        let json = r#"{"entries":[{
            "provider":"broken","model_prefix":"",
            "schema_version":1,"profile_version":"broken-1","model":"m",
            "max_context_tokens":10000,"target_tokens":9000,
            "soft_limit_tokens":9500,"hard_limit_tokens":9800,
            "absolute_mvc_max_limit":4000,"tool_schema_reserve":1000,
            "tool_call_reserve":1024,"final_answer_reserve":2048,
            "streaming_reserve":512,"retry_reserve":1024,
            "low_priority_cutoff":30,"offload_threshold_bytes":32768}]}"#;
        let catalog = ProfileCatalog::from_json(json).expect("json parses");
        assert_eq!(catalog.rejected().len(), 1);
        // Отклонённый профиль не используется: resolve уходит в fallback.
        let profile = catalog.resolve("broken", "m", Some(10_000));
        assert!(profile.profile_version.starts_with("fallback-1/"));
    }

    #[test]
    fn unknown_model_gets_the_fallback_profile() {
        let catalog = ProfileCatalog::builtin();
        let profile = catalog.resolve("nowhere", "mystery", Some(16_384));
        assert!(profile.profile_version.starts_with("fallback-1/"));
        profile.validate().expect("fallback is valid");
    }

    #[test]
    fn longest_model_prefix_wins() {
        let json = r#"{"entries":[
          {"provider":"p","model_prefix":"",
           "schema_version":1,"profile_version":"p-any","model":"",
           "max_context_tokens":128000,"target_tokens":70000,
           "soft_limit_tokens":96000,"hard_limit_tokens":108800,
           "absolute_mvc_max_limit":50000,"tool_schema_reserve":4096,
           "tool_call_reserve":1024,"final_answer_reserve":2048,
           "streaming_reserve":512,"retry_reserve":1024,
           "low_priority_cutoff":30,"offload_threshold_bytes":32768},
          {"provider":"p","model_prefix":"big-",
           "schema_version":1,"profile_version":"p-big","model":"",
           "max_context_tokens":200000,"target_tokens":100000,
           "soft_limit_tokens":150000,"hard_limit_tokens":170000,
           "absolute_mvc_max_limit":80000,"tool_schema_reserve":4096,
           "tool_call_reserve":1024,"final_answer_reserve":2048,
           "streaming_reserve":512,"retry_reserve":1024,
           "low_priority_cutoff":30,"offload_threshold_bytes":32768}]}"#;
        let catalog = ProfileCatalog::from_json(json).expect("json parses");
        assert_eq!(catalog.resolve("p", "small-1", None).profile_version, "p-any");
        assert_eq!(catalog.resolve("p", "big-1", None).profile_version, "p-big");
    }

    /// Каталог провайдера — источник правды об окне, встроенный профиль —
    /// предположение. Расхождение решается в пользу провайдера, иначе
    /// планировщик уверенно считает бюджет по устаревшей цифре.
    #[test]
    fn a_provider_window_overrides_the_builtin_profile() {
        let json = r#"{"entries":[
          {"provider":"p","model_prefix":"",
           "schema_version":1,"profile_version":"p-any","model":"",
           "max_context_tokens":128000,"target_tokens":70000,
           "soft_limit_tokens":96000,"hard_limit_tokens":108800,
           "absolute_mvc_max_limit":50000,"tool_schema_reserve":4096,
           "tool_call_reserve":1024,"final_answer_reserve":2048,
           "streaming_reserve":512,"retry_reserve":1024,
           "low_priority_cutoff":30,"offload_threshold_bytes":32768}]}"#;
        let catalog = ProfileCatalog::from_json(json).expect("json parses");

        // Провайдер молчит — остаётся встроенный профиль.
        assert_eq!(catalog.resolve("p", "m", None).max_context_tokens, 128_000);
        // Совпало — тоже: пересчитывать нечего.
        assert_eq!(
            catalog.resolve("p", "m", Some(128_000)).profile_version,
            "p-any"
        );

        let narrow = catalog.resolve("p", "m", Some(32_000));
        assert_eq!(narrow.max_context_tokens, 32_000);
        assert!(narrow.hard_limit_tokens <= 32_000);
        narrow.validate().expect("narrowed profile is valid");

        let wide = catalog.resolve("p", "m", Some(1_000_000));
        assert_eq!(wide.max_context_tokens, 1_000_000);
        wide.validate().expect("widened profile is valid");
    }

    #[test]
    fn replan_reduces_hard_limit_by_ten_percent_and_drops_optional_reserves() {
        let profile = ModelContextProfile::fallback("unknown", "model", 128_000);
        let replanned = profile.replan(None);
        assert_eq!(replanned.hard_limit_tokens, percent(108_800, 90));
        assert_eq!(replanned.retry_reserve, 0);
        assert_eq!(replanned.streaming_reserve, 0);
        assert_eq!(replanned.tool_call_reserve, profile.tool_call_reserve);
        assert_eq!(replanned.final_answer_reserve, profile.final_answer_reserve);
        replanned.validate().expect("replan stays valid");
    }

    #[test]
    fn replan_respects_the_provider_window() {
        let profile = ModelContextProfile::fallback("unknown", "model", 128_000);
        let replanned = profile.replan(Some(50_000));
        assert_eq!(replanned.hard_limit_tokens, 50_000);
    }

    #[test]
    fn fallback_estimator_scaling_uses_seventy_percent_and_keeps_reserves() {
        let profile = ModelContextProfile::fallback("unknown", "model", 128_000);
        let scaled = profile.scaled_for_fallback_estimator();
        assert_eq!(scaled.hard_limit_tokens, percent(profile.hard_limit_tokens, 70));
        assert_eq!(scaled.soft_limit_tokens, percent(profile.soft_limit_tokens, 70));
        assert_eq!(scaled.target_tokens, percent(profile.target_tokens, 70));
        assert_eq!(scaled.reserves_total(), profile.reserves_total());
        assert_ne!(scaled.profile_version, profile.profile_version);
    }

    #[test]
    fn user_catalog_overrides_the_builtin_entry() {
        let mut catalog = ProfileCatalog::builtin();
        let before = catalog.resolve("literouter", "mystery-model", None).profile_version;
        let user = ProfileCatalog::from_json(
            r#"{"entries":[{"provider":"literouter","model_prefix":"",
            "schema_version":1,"profile_version":"user-1","model":"",
            "max_context_tokens":128000,"target_tokens":60000,
            "soft_limit_tokens":90000,"hard_limit_tokens":100000,
            "absolute_mvc_max_limit":40000,"tool_schema_reserve":4096,
            "tool_call_reserve":1024,"final_answer_reserve":2048,
            "streaming_reserve":512,"retry_reserve":1024,
            "low_priority_cutoff":30,"offload_threshold_bytes":32768}]}"#,
        )
        .expect("json parses");
        catalog.overlay(user);
        let after = catalog.resolve("literouter", "mystery-model", None).profile_version;
        assert_ne!(before, after);
        assert_eq!(after, "user-1");
    }
}
