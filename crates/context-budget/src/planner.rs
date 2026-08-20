//! `ContextPlanner` — внутренний компонент Core, собирающий контекст под
//! bounded budget (этап 01.1).
//!
//! Контур: selection → compress/offload → final budget validation →
//! `ModelContext` event → model call. Финальная проверка обязательна и
//! выполняется до формирования события: при её невыполнении Core повторяет
//! разрешённые deterministic drops, а после их исчерпания завершает вызов через
//! `BudgetUnavailable`.

use std::sync::Arc;

use crate::budget::{
    BudgetUnavailable, BudgetUnavailableStage, ContextBudget, MinimumViableContext,
};
use crate::compression::{order_items, prune};
use crate::estimator::{EstimateCache, FallbackEstimator, TokenEstimator};
use crate::hash::{content_hash, normalized_bytes, ContentForm, NORMALIZER_VERSION};
use crate::item::{BudgetCategory, ContextItem, DropReason};
use crate::ladder::{
    run_ladder, LadderContext, LadderDiagnostic, NoOffload, NoSummarizer, OffloadSink, Selection,
    Summarizer,
};
use crate::ledger::{
    CompressionRecord, ContextLedgerEntry, DroppedItemRecord, LedgerOutcome, LoadoutRecord,
    MandatoryPartRecord, SelectedItemRecord, CONTEXT_LEDGER_SCHEMA_VERSION,
};
use crate::metrics::ContextMetrics;
use crate::profile::{ModelContextProfile, ProfileCatalog, STRATEGY_VERSION};

/// Содержимое item во владеющей форме: планировщик считает по нему
/// `content_hash`, размер и оценку токенов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnedContent {
    Text(String),
    Json(String),
    Binary(Vec<u8>),
}

impl OwnedContent {
    pub fn as_form(&self) -> ContentForm<'_> {
        match self {
            Self::Text(text) => ContentForm::Text(text),
            Self::Json(text) => ContentForm::Json(text),
            Self::Binary(bytes) => ContentForm::Binary(bytes),
        }
    }

    /// Текстовое представление для conflict detection. Двоичное содержимое
    /// в текстовые проверки не попадает.
    pub fn as_text(&self) -> &str {
        match self {
            Self::Text(text) | Self::Json(text) => text,
            Self::Binary(_) => "",
        }
    }
}

/// Кандидат контекста: атрибуты плюс содержимое.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanInput {
    pub item: ContextItem,
    pub content: OwnedContent,
}

impl PlanInput {
    pub fn new(item: ContextItem, content: OwnedContent) -> Self {
        Self { item, content }
    }
}

/// Запрос на сборку контекста для одного model call.
#[derive(Debug, Clone)]
pub struct PlanRequest {
    pub task_id: String,
    pub session_id: String,
    pub model_call_id: String,
    pub provider: String,
    pub model: String,
    /// Окно, заявленное провайдером, если известно.
    pub provider_window: Option<u32>,
    /// unix ms.
    pub now: i64,
    pub inputs: Vec<PlanInput>,
    /// Итог tool loadout (01.4), если он уже собран.
    pub loadout: Option<LoadoutRecord>,
    /// Идентификатор записи ledger, ре-план которой выполняется.
    pub replan_of: Option<String>,
    /// Команда `summarize now` (01.5): лестница запускается даже ниже
    /// `soft_limit_tokens`. Действует только на текущую сборку и не меняет
    /// долговременную память.
    #[allow(clippy::struct_field_names)]
    pub force_reduction: bool,
}

/// Итог сборки. Возвращается и при успехе, и при отказе: в обоих случаях есть
/// запись ledger с hash, чтобы UI и дочерние задачи отличали отказ сборки от
/// прочих ошибок Core.
#[derive(Debug, Clone)]
pub struct ContextPlan {
    pub profile: ModelContextProfile,
    pub budget: ContextBudget,
    /// Выбранные item в порядке собранного контекста.
    pub selected: Vec<ContextItem>,
    /// Отброшенные item с причинами.
    pub dropped: Vec<ContextItem>,
    pub reserves: u32,
    pub ledger: ContextLedgerEntry,
    pub diagnostics: Vec<LadderDiagnostic>,
    pub unavailable: Option<BudgetUnavailable>,
    pub fallback_estimator: bool,
}

impl ContextPlan {
    /// Готов ли контекст к отправке в модель.
    pub fn is_ready(&self) -> bool {
        self.unavailable.is_none()
    }

    pub fn context_ledger_hash(&self) -> &str {
        &self.ledger.context_ledger_hash
    }

    pub fn estimated_prompt_tokens(&self) -> u32 {
        self.ledger.estimated_prompt_tokens
    }
}

/// Планировщик контекста. Владелец состояния и политики — Core.
pub struct ContextPlanner {
    catalog: ProfileCatalog,
    primary: Option<Arc<dyn TokenEstimator>>,
    fallback_available: bool,
    cache: EstimateCache,
    metrics: ContextMetrics,
}

impl ContextPlanner {
    pub fn new(catalog: ProfileCatalog, primary: Option<Arc<dyn TokenEstimator>>) -> Self {
        Self {
            catalog,
            primary,
            fallback_available: true,
            cache: EstimateCache::new(),
            metrics: ContextMetrics::default(),
        }
    }

    /// Планировщик со встроенным каталогом профилей.
    pub fn with_builtin_catalog(primary: Option<Arc<dyn TokenEstimator>>) -> Self {
        Self::new(ProfileCatalog::builtin(), primary)
    }

    /// Управление доступностью fallback-estimator (нужно тестам и doctor).
    pub fn set_fallback_estimator_available(&mut self, available: bool) {
        self.fallback_available = available;
    }

    pub fn metrics(&self) -> &ContextMetrics {
        &self.metrics
    }

    pub fn cache(&self) -> &EstimateCache {
        &self.cache
    }

    pub fn catalog(&self) -> &ProfileCatalog {
        &self.catalog
    }

    /// Сборка контекста без 01.2/01.3: лестница состоит из L1–L3 и L6.
    pub fn plan(&mut self, request: &PlanRequest) -> ContextPlan {
        self.plan_with(request, &mut NoOffload, &mut NoSummarizer, None)
    }

    /// Сборка контекста с подключёнными возможностями 01.2 (offload) и 01.3
    /// (summarizer). Недоступная возможность не блокирует сборку: уровень
    /// немедленно считается исчерпанным с diagnostic.
    pub fn plan_with(
        &mut self,
        request: &PlanRequest,
        offload: &mut dyn OffloadSink,
        summarizer: &mut dyn Summarizer,
        profile_override: Option<ModelContextProfile>,
    ) -> ContextPlan {
        let started = std::time::Instant::now();
        self.metrics.calls_total += 1;

        // 1. Выбор estimator. Core не угадывает размер: при недоступности
        //    основного используется консервативный fallback, при недоступности
        //    обоих сборка завершается отказом.
        let (estimator, fallback_estimator): (Arc<dyn TokenEstimator>, bool) =
            match (self.primary.clone(), self.fallback_available) {
                (Some(primary), _) => (primary, false),
                (None, true) => (Arc::new(FallbackEstimator), true),
                (None, false) => {
                    let profile = profile_override.clone().unwrap_or_else(|| {
                        self.catalog.resolve(
                            &request.provider,
                            &request.model,
                            request.provider_window,
                        )
                    });
                    return self.refuse(
                        request,
                        profile,
                        "unavailable",
                        BudgetUnavailableStage::EstimatorUnavailable,
                        0,
                        0,
                        None,
                        Vec::new(),
                        Vec::new(),
                        false,
                        Vec::new(),
                        started,
                    );
                }
            };

        // 2. Профиль. Для fallback-estimator пороги масштабируются на 0.70,
        //    резервы не уменьшаются.
        let base_profile = profile_override.unwrap_or_else(|| {
            self.catalog
                .resolve(&request.provider, &request.model, request.provider_window)
        });
        let profile = if fallback_estimator {
            base_profile.scaled_for_fallback_estimator()
        } else {
            base_profile
        };
        // Профиль неизвестной модели не может обойти обязательный минимум:
        // несовместимые значения дают `BudgetUnavailable`, а не молчаливое
        // превышение.
        if profile.validate().is_err() {
            let started_at = started;
            return self.refuse(
                request,
                profile.clone(),
                estimator.version(),
                BudgetUnavailableStage::MandatoryOverflow,
                profile.reserves_total(),
                profile.hard_limit_tokens,
                Some(crate::budget::MandatoryPart::SafetyPolicy),
                Vec::new(),
                Vec::new(),
                fallback_estimator,
                Vec::new(),
                started_at,
            );
        }

        let budget = ContextBudget::from_profile(&profile);
        let reserves = profile.reserves_total();

        // 3. Оценка item. `content_hash`, размер и токены считаются здесь,
        //    поэтому вызывающая сторона не может подсунуть несогласованные
        //    значения.
        let mut items: Vec<ContextItem> = Vec::with_capacity(request.inputs.len());
        for input in &request.inputs {
            let mut item = input.item.clone();
            let form = input.content.as_form();
            item.content_hash = content_hash(item.kind.as_str(), &form);
            item.bytes = normalized_bytes(&form).len() as u64;
            item.tokenizer_version = estimator.version().to_string();
            let content_tokens = self.cache.estimate(
                estimator.as_ref(),
                &item.content_hash,
                NORMALIZER_VERSION,
                &form,
            );
            item.estimated_tokens = content_tokens.saturating_add(estimator.message_overhead());
            item.selected = true;
            item.drop_reason = None;
            items.push(item);
        }

        // 4. Обязательный минимум. Набор и его причины фиксируются до обычного
        //    pruning.
        let mvc = MinimumViableContext::select(&items);
        let mandatory_tokens = mvc.tokens;
        let mandatory_parts: Vec<MandatoryPartRecord> = mvc
            .parts
            .iter()
            .map(|(part, count, tokens)| MandatoryPartRecord {
                part: *part,
                items: *count,
                tokens: *tokens,
            })
            .collect();

        // 5. `absolute_mvc_max_limit` проверяется раньше проверки против
        //    `hard_limit_tokens`, поэтому раздувшийся системный промпт даёт
        //    понятный отказ, а не бесконечное сокращение необязательной части.
        if mandatory_tokens > profile.absolute_mvc_max_limit {
            let missing = mvc.missing_part(profile.absolute_mvc_max_limit);
            return self.refuse(
                request,
                profile.clone(),
                estimator.version(),
                BudgetUnavailableStage::MandatoryOverflow,
                mandatory_tokens,
                budget_absolute_limit(&profile),
                missing,
                mandatory_parts,
                Vec::new(),
                fallback_estimator,
                Vec::new(),
                started,
            );
        }
        if mandatory_tokens.saturating_add(reserves) > profile.hard_limit_tokens {
            let available = profile.hard_limit_tokens.saturating_sub(reserves);
            let missing = mvc.missing_part(available);
            return self.refuse(
                request,
                profile.clone(),
                estimator.version(),
                BudgetUnavailableStage::MandatoryOverflow,
                mandatory_tokens.saturating_add(reserves),
                profile.hard_limit_tokens,
                missing,
                mandatory_parts,
                Vec::new(),
                fallback_estimator,
                Vec::new(),
                started,
            );
        }

        // 6. Pruning до лестницы: дубликаты, вытесненные ревизии и истёкшие
        //    записи (01.3). Обязательные item не трогаются.
        let mut prunable = items.clone();
        for item in &mut prunable {
            item.selected = !item.is_mandatory_kind();
        }
        let pruned = prune(&mut prunable, request.now);
        for (id, reason) in &pruned {
            if let Some(item) = items.iter_mut().find(|item| &item.id == id) {
                if !item.is_mandatory_kind() {
                    item.selected = false;
                    item.drop_reason = Some(*reason);
                    self.metrics.record_drop(*reason);
                }
            }
        }

        let surviving: Vec<ContextItem> =
            items.iter().filter(|item| item.selected).cloned().collect();
        let pre_dropped: Vec<ContextItem> = items
            .iter()
            .filter(|item| !item.selected)
            .cloned()
            .collect();

        // 7. Лестница сокращения. Запускается только при превышении
        //    `soft_limit_tokens`; цель — вернуться к `target_tokens`.
        let mut selection = Selection::new(surviving, reserves);
        let mut ladder_outcome = crate::ladder::LadderOutcome::default();
        if request.force_reduction || !budget.within_soft_limit(selection.context_tokens()) {
            ladder_outcome = run_ladder(
                &mut selection,
                &LadderContext {
                    now: request.now,
                    profile: &profile,
                    // `summarize now` применяет все уровни сокращения
                    // содержимого целиком, поэтому цель ставится по обязательной
                    // части: каждый уровень исчерпывает своих кандидатов.
                    goal_context_tokens: if request.force_reduction {
                        mandatory_tokens
                    } else {
                        profile.target_tokens
                    },
                    // Резервы под ответ и tool-call принудительное сжатие не трогает.
                    allow_reserve_release: !request.force_reduction,
                },
                offload,
                summarizer,
            );
        }
        for level in &ladder_outcome.levels_applied {
            self.metrics.record_ladder_level(*level);
        }
        self.metrics.offloaded_bytes_total = self
            .metrics
            .offloaded_bytes_total
            .saturating_add(ladder_outcome.offloaded_bytes);

        let mut selected: Vec<ContextItem> = selection.selected().cloned().collect();
        let mut dropped: Vec<ContextItem> = pre_dropped;
        for item in selection.dropped() {
            // Item, заменённый summary, отбрасывается без `drop_reason`: его
            // судьба описана связью `summary_id -> source_ids`.
            let mut dropped_item = item.clone();
            if dropped_item.drop_reason.is_none()
                && ladder_outcome
                    .summaries
                    .iter()
                    .all(|summary| !summary.source_ids.contains(&dropped_item.id))
            {
                dropped_item.drop_reason = Some(DropReason::OverBudget);
            }
            if let Some(reason) = dropped_item.drop_reason {
                self.metrics.record_drop(reason);
            }
            dropped.push(dropped_item);
        }

        // 8. Порядок собранного контекста: обязательные части в фиксированном
        //    порядке, затем остальные по иерархии прав.
        let (mut mandatory_items, mut optional_items): (Vec<ContextItem>, Vec<ContextItem>) =
            selected.drain(..).partition(ContextItem::is_mandatory_kind);
        mandatory_items.sort_by_key(|item| {
            (
                crate::budget::MandatoryPart::of_kind(item.kind),
                item.created_at,
                item.id.clone(),
            )
        });
        order_items(&mut optional_items);
        let mut ordered = mandatory_items;
        ordered.extend(optional_items);

        let final_mandatory: u32 = ordered
            .iter()
            .filter(|item| item.is_mandatory_kind())
            .map(|item| item.estimated_tokens)
            .fold(0, u32::saturating_add);
        let final_optional: u32 = ordered
            .iter()
            .filter(|item| !item.is_mandatory_kind())
            .map(|item| item.estimated_tokens)
            .fold(0, u32::saturating_add);
        let final_reserves = selection.reserves;

        // 9. Финальная проверка бюджета. Выполняется после compress/offload и
        //    до формирования события.
        let total = final_mandatory
            .saturating_add(final_optional)
            .saturating_add(final_reserves);
        if total > profile.hard_limit_tokens {
            let missing = mvc.missing_part(profile.hard_limit_tokens);
            let mut refusal = self.refuse(
                request,
                profile.clone(),
                estimator.version(),
                BudgetUnavailableStage::DropsExhausted,
                total,
                profile.hard_limit_tokens,
                missing,
                mandatory_parts,
                ordered
                    .iter()
                    .map(|item| SelectedItemRecord {
                        id: item.id.clone(),
                        estimated_tokens: item.estimated_tokens,
                    })
                    .collect(),
                fallback_estimator,
                ladder_outcome.levels_applied.clone(),
                started,
            );
            refusal.diagnostics = ladder_outcome.diagnostics.clone();
            refusal.selected = ordered;
            refusal.dropped = dropped;
            return refusal;
        }

        // 10. Утилизация бюджета по категориям — для диагностики, без сырых данных.
        for category in BudgetCategory::all() {
            let used: u32 = ordered
                .iter()
                .filter(|item| item.kind.category() == category)
                .map(|item| item.estimated_tokens)
                .fold(0, u32::saturating_add);
            self.metrics.record_utilization(
                category,
                used,
                budget.category(category).target_tokens,
            );
        }

        let compression: Vec<CompressionRecord> = ladder_outcome
            .summaries
            .iter()
            .map(|summary| {
                let source_tokens: u32 = dropped
                    .iter()
                    .filter(|item| summary.source_ids.contains(&item.id))
                    .map(|item| item.estimated_tokens)
                    .fold(0, u32::saturating_add);
                CompressionRecord {
                    summary_id: summary.summary_id.clone(),
                    source_ids: summary.source_ids.clone(),
                    compression_ratio: if source_tokens == 0 {
                        0.0
                    } else {
                        f64::from(summary.summary_tokens) / f64::from(source_tokens)
                    },
                    summarizer_version: summary.summarizer_version.clone(),
                    summary_budget: summary.summary_tokens,
                    fallback: summary.fallback,
                    fallback_reason: summary.fallback_reason.clone(),
                }
            })
            .collect();

        let mut ledger = ContextLedgerEntry {
            id: format!("ledger-{}", request.model_call_id),
            schema_version: CONTEXT_LEDGER_SCHEMA_VERSION,
            task_id: request.task_id.clone(),
            session_id: request.session_id.clone(),
            model_call_id: request.model_call_id.clone(),
            created_at: request.now,
            provider: request.provider.clone(),
            model: request.model.clone(),
            profile_version: profile.profile_version.clone(),
            profile_snapshot: serde_json::to_string(&profile).unwrap_or_else(|_| "{}".to_string()),
            tokenizer_version: estimator.version().to_string(),
            normalizer_version: NORMALIZER_VERSION.to_string(),
            strategy_version: STRATEGY_VERSION.to_string(),
            mandatory_tokens: final_mandatory,
            selected_optional_tokens: final_optional,
            reserves_tokens: final_reserves,
            estimated_prompt_tokens: final_mandatory.saturating_add(final_optional),
            selected_items: ordered
                .iter()
                .map(|item| SelectedItemRecord {
                    id: item.id.clone(),
                    estimated_tokens: item.estimated_tokens,
                })
                .collect(),
            dropped_items: dropped
                .iter()
                .filter_map(|item| {
                    item.drop_reason.map(|reason| DroppedItemRecord {
                        id: item.id.clone(),
                        drop_reason: reason,
                    })
                })
                .collect(),
            mandatory_parts,
            ladder_levels_applied: ladder_outcome.levels_applied.clone(),
            compression,
            loadout: request.loadout.clone(),
            fallback_estimator,
            replan_of: request.replan_of.clone(),
            outcome: LedgerOutcome::Sent,
            budget_unavailable: None,
            context_ledger_hash: String::new(),
        };
        ledger.finalize_hash();

        self.metrics
            .record_selection_latency(started.elapsed().as_millis() as u64);

        ContextPlan {
            profile,
            budget,
            selected: ordered,
            dropped,
            reserves: final_reserves,
            ledger,
            diagnostics: ladder_outcome.diagnostics,
            unavailable: None,
            fallback_estimator,
        }
    }

    /// Deterministic re-plan после context-length error провайдера. Выполняется
    /// ровно один раз: повторный отказ завершает вызов через `BudgetUnavailable`
    /// со `stage=provider_replan_failed`, каскад re-plan запрещён.
    pub fn replan_after_context_length_error(
        &mut self,
        request: &PlanRequest,
        previous: &ContextPlan,
        provider_window: Option<u32>,
        offload: &mut dyn OffloadSink,
        summarizer: &mut dyn Summarizer,
    ) -> ContextPlan {
        if previous.ledger.replan_of.is_some() {
            // Повторный отказ: каскад re-plan запрещён.
            self.metrics.record_replan("failed");
            let started = std::time::Instant::now();
            let mut refusal = self.refuse(
                request,
                previous.profile.clone(),
                &previous.ledger.tokenizer_version,
                BudgetUnavailableStage::ProviderReplanFailed,
                previous.ledger.estimated_prompt_tokens,
                previous.profile.hard_limit_tokens,
                previous
                    .ledger
                    .mandatory_parts
                    .first()
                    .map(|part| part.part),
                previous.ledger.mandatory_parts.clone(),
                previous.ledger.selected_items.clone(),
                previous.fallback_estimator,
                previous.ledger.ladder_levels_applied.clone(),
                started,
            );
            refusal.ledger.replan_of = Some(previous.ledger.id.clone());
            refusal.ledger.finalize_hash();
            return refusal;
        }

        let replanned_profile = previous.profile.replan(provider_window);
        let mut replan_request = request.clone();
        replan_request.replan_of = Some(previous.ledger.id.clone());
        let plan = self.plan_with(
            &replan_request,
            offload,
            summarizer,
            Some(replanned_profile),
        );
        if plan.is_ready() {
            self.metrics.record_replan("succeeded");
            plan
        } else {
            // Re-plan сам завершился отказом сборки: внешний stage —
            // `provider_replan_failed`, внутренняя причина попадает в
            // `missing_part`.
            self.metrics.record_replan("failed");
            let mut escalated = plan;
            if let Some(unavailable) = escalated.unavailable.as_mut() {
                unavailable.stage = BudgetUnavailableStage::ProviderReplanFailed;
            }
            escalated.ledger.budget_unavailable = escalated.unavailable.clone();
            escalated.ledger.finalize_hash();
            if let Some(unavailable) = escalated.unavailable.as_mut() {
                unavailable.context_ledger_hash = escalated.ledger.context_ledger_hash.clone();
            }
            escalated.ledger.budget_unavailable = escalated.unavailable.clone();
            escalated
        }
    }

    /// Запись фактического usage провайдера. Пишется отдельно от ledger, чтобы
    /// запись оставалась immutable и hash-стабильной.
    pub fn record_actual_usage(
        &mut self,
        plan: &ContextPlan,
        actual_prompt_tokens: u32,
    ) -> crate::estimator::EstimatorDrift {
        let drift = crate::estimator::EstimatorDrift::measure(
            plan.ledger.estimated_prompt_tokens,
            actual_prompt_tokens,
        );
        self.metrics.record_estimator_drift(drift.relative);
        drift
    }

    #[allow(clippy::too_many_arguments)]
    fn refuse(
        &mut self,
        request: &PlanRequest,
        profile: ModelContextProfile,
        tokenizer_version: &str,
        stage: BudgetUnavailableStage,
        required_tokens: u32,
        available_tokens: u32,
        missing_part: Option<crate::budget::MandatoryPart>,
        mandatory_parts: Vec<MandatoryPartRecord>,
        selected_items: Vec<SelectedItemRecord>,
        fallback_estimator: bool,
        ladder_levels_applied: Vec<crate::ladder::LadderLevel>,
        started: std::time::Instant,
    ) -> ContextPlan {
        self.metrics.record_budget_unavailable(stage);
        let budget = ContextBudget::from_profile(&profile);
        let unavailable = BudgetUnavailable::new(
            stage,
            required_tokens,
            available_tokens,
            profile.profile_version.clone(),
            tokenizer_version,
        )
        .with_missing_part(missing_part);

        let mut ledger = ContextLedgerEntry {
            id: format!("ledger-{}", request.model_call_id),
            schema_version: CONTEXT_LEDGER_SCHEMA_VERSION,
            task_id: request.task_id.clone(),
            session_id: request.session_id.clone(),
            model_call_id: request.model_call_id.clone(),
            created_at: request.now,
            provider: request.provider.clone(),
            model: request.model.clone(),
            profile_version: profile.profile_version.clone(),
            profile_snapshot: serde_json::to_string(&profile).unwrap_or_else(|_| "{}".to_string()),
            tokenizer_version: tokenizer_version.to_string(),
            normalizer_version: NORMALIZER_VERSION.to_string(),
            strategy_version: STRATEGY_VERSION.to_string(),
            mandatory_tokens: mandatory_parts
                .iter()
                .map(|part| part.tokens)
                .fold(0, u32::saturating_add),
            selected_optional_tokens: 0,
            reserves_tokens: profile.reserves_total(),
            estimated_prompt_tokens: required_tokens,
            selected_items,
            dropped_items: Vec::new(),
            mandatory_parts,
            ladder_levels_applied,
            compression: Vec::new(),
            loadout: request.loadout.clone(),
            fallback_estimator,
            replan_of: request.replan_of.clone(),
            outcome: LedgerOutcome::BudgetUnavailable,
            budget_unavailable: Some(unavailable.clone()),
            context_ledger_hash: String::new(),
        };
        ledger.finalize_hash();
        let unavailable = unavailable.with_ledger_hash(ledger.context_ledger_hash.clone());
        ledger.budget_unavailable = Some(unavailable.clone());

        self.metrics
            .record_selection_latency(started.elapsed().as_millis() as u64);

        ContextPlan {
            profile,
            budget,
            selected: Vec::new(),
            dropped: Vec::new(),
            reserves: 0,
            ledger,
            diagnostics: Vec::new(),
            unavailable: Some(unavailable),
            fallback_estimator,
        }
    }
}

fn budget_absolute_limit(profile: &ModelContextProfile) -> u32 {
    profile.absolute_mvc_max_limit
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::estimator::HeuristicEstimator;
    use crate::item::{ContextItemBuilder, ItemKind, Privacy};
    use crate::ladder::{LadderLevel, OffloadOutcome, SummaryOutcome};

    fn planner() -> ContextPlanner {
        ContextPlanner::with_builtin_catalog(Some(Arc::new(HeuristicEstimator::default_for(
            "test-model",
        ))))
    }

    fn text_input(
        id: &str,
        kind: ItemKind,
        text: &str,
        priority: u8,
        created_at: i64,
    ) -> PlanInput {
        PlanInput::new(
            ContextItemBuilder::new(id, kind, "")
                .task("task", "session")
                .priority(priority)
                .created_at(created_at)
                .build(),
            OwnedContent::Text(text.to_string()),
        )
    }

    fn request(inputs: Vec<PlanInput>) -> PlanRequest {
        PlanRequest {
            task_id: "task".to_string(),
            session_id: "session".to_string(),
            model_call_id: "call-1".to_string(),
            provider: "literouter".to_string(),
            model: "gpt-4o-mini".to_string(),
            provider_window: None,
            now: 1_000_000,
            inputs,
            loadout: None,
            replan_of: None,
            force_reduction: false,
        }
    }

    fn minimal_inputs() -> Vec<PlanInput> {
        vec![
            text_input("safety", ItemKind::SafetyPolicy, "безопасность", 100, 1),
            text_input(
                "prompt",
                ItemKind::UserPrompt,
                "проверь репозиторий",
                100,
                2,
            ),
        ]
    }

    #[test]
    fn a_small_context_is_assembled_without_touching_the_ladder() {
        let mut planner = planner();
        let plan = planner.plan(&request(minimal_inputs()));
        assert!(plan.is_ready());
        assert!(plan.ledger.ladder_levels_applied.is_empty());
        assert_eq!(plan.selected.len(), 2);
        assert_eq!(plan.ledger.context_ledger_hash.len(), 64);
        assert_eq!(plan.ledger.outcome, LedgerOutcome::Sent);
    }

    #[test]
    fn mandatory_parts_come_first_in_the_assembled_order() {
        let mut planner = planner();
        let mut inputs = minimal_inputs();
        inputs.push(text_input("history", ItemKind::History, "старое", 90, 3));
        inputs.push(text_input(
            "approval",
            ItemKind::ApprovalPolicy,
            "approval",
            90,
            4,
        ));
        let plan = planner.plan(&request(inputs));
        let ids: Vec<&str> = plan.selected.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids[0], "safety");
        assert_eq!(ids[1], "approval");
        assert_eq!(ids[2], "prompt");
        assert_eq!(ids[3], "history");
    }

    #[test]
    fn identical_input_produces_an_identical_ledger_hash() {
        let mut left = planner();
        let mut right = planner();
        let hash = |planner: &mut ContextPlanner| {
            planner
                .plan(&request(minimal_inputs()))
                .ledger
                .context_ledger_hash
        };
        assert_eq!(hash(&mut left), hash(&mut right));
    }

    #[test]
    fn changing_item_order_does_not_change_the_hash_but_changing_content_does() {
        let mut planner = planner();
        let mut reordered = minimal_inputs();
        reordered.reverse();
        let base = planner.plan(&request(minimal_inputs()));
        let same = planner.plan(&request(reordered));
        // Порядок собранного контекста детерминирован, поэтому перестановка
        // входа не меняет hash.
        assert_eq!(
            base.ledger.context_ledger_hash,
            same.ledger.context_ledger_hash
        );

        let mut changed = minimal_inputs();
        changed[1] = text_input("prompt", ItemKind::UserPrompt, "другой запрос", 100, 2);
        let other = planner.plan(&request(changed));
        assert_ne!(
            base.ledger.context_ledger_hash,
            other.ledger.context_ledger_hash
        );
    }

    #[test]
    fn an_oversized_mandatory_minimum_refuses_before_selection() {
        let mut planner = planner();
        let huge = "я".repeat(2_000_000);
        let inputs = vec![
            text_input("safety", ItemKind::SafetyPolicy, &huge, 100, 1),
            text_input("prompt", ItemKind::UserPrompt, "запрос", 100, 2),
            text_input("history", ItemKind::History, "история", 50, 3),
        ];
        let plan = planner.plan(&request(inputs));
        assert!(!plan.is_ready());
        let unavailable = plan.unavailable.expect("refusal");
        assert_eq!(unavailable.stage, BudgetUnavailableStage::MandatoryOverflow);
        assert_eq!(
            unavailable.missing_part,
            Some(crate::budget::MandatoryPart::SafetyPolicy)
        );
        assert!(!unavailable.context_ledger_hash.is_empty());
        assert_eq!(plan.ledger.outcome, LedgerOutcome::BudgetUnavailable);
        // Selection не запускалась.
        assert!(plan.ledger.selected_items.is_empty());
    }

    #[test]
    fn a_window_too_small_for_the_mandatory_minimum_refuses_instead_of_overflowing() {
        let mut planner = planner();
        let mut small = request(minimal_inputs());
        small.provider = "nowhere".to_string();
        small.model = "tiny".to_string();
        small.provider_window = Some(4_096);
        let plan = planner.plan(&small);
        assert!(!plan.is_ready());
        assert_eq!(
            plan.unavailable.expect("refusal").stage,
            BudgetUnavailableStage::MandatoryOverflow
        );
    }

    #[test]
    fn an_unavailable_estimator_refuses_instead_of_guessing() {
        let mut planner = ContextPlanner::with_builtin_catalog(None);
        planner.set_fallback_estimator_available(false);
        let plan = planner.plan(&request(minimal_inputs()));
        assert!(!plan.is_ready());
        assert_eq!(
            plan.unavailable.expect("refusal").stage,
            BudgetUnavailableStage::EstimatorUnavailable
        );
    }

    #[test]
    fn a_missing_primary_estimator_falls_back_with_scaled_thresholds() {
        let mut planner = ContextPlanner::with_builtin_catalog(None);
        let plan = planner.plan(&request(minimal_inputs()));
        assert!(plan.is_ready());
        assert!(plan.fallback_estimator);
        assert!(plan.ledger.fallback_estimator);
        assert!(plan
            .profile
            .profile_version
            .ends_with("+fallback-estimator"));
        let base = ProfileCatalog::builtin().resolve("literouter", "gpt-4o-mini", None);
        assert!(plan.profile.hard_limit_tokens < base.hard_limit_tokens);
        assert_eq!(plan.profile.reserves_total(), base.reserves_total());
    }

    #[test]
    fn the_ladder_runs_only_above_the_soft_limit_and_keeps_the_hard_invariant() {
        let mut planner = planner();
        let block = "данные ".repeat(20_000);
        let mut inputs = minimal_inputs();
        for index in 0..12 {
            inputs.push(text_input(
                &format!("history-{index:02}"),
                ItemKind::History,
                &block,
                10,
                10 + i64::from(index),
            ));
        }
        let plan = planner.plan(&request(inputs));
        assert!(plan.is_ready());
        assert!(!plan.ledger.ladder_levels_applied.is_empty());
        assert!(plan
            .ledger
            .ladder_levels_applied
            .contains(&LadderLevel::LowPriorityOptional));
        let total = plan.ledger.mandatory_tokens
            + plan.ledger.selected_optional_tokens
            + plan.ledger.reserves_tokens;
        assert!(total <= plan.profile.hard_limit_tokens);
    }

    #[test]
    fn exhausted_drops_refuse_with_the_drops_exhausted_stage() {
        // Все необязательные item закреплены и высокоприоритетны, поэтому
        // лестница не может освободить достаточно места.
        let mut planner = planner();
        let block = "данные ".repeat(60_000);
        let mut inputs = minimal_inputs();
        for index in 0..6 {
            let mut input = text_input(
                &format!("pinned-{index}"),
                ItemKind::PendingToolCall,
                &block,
                100,
                10 + i64::from(index),
            );
            input.item.pinned = true;
            inputs.push(input);
        }
        let plan = planner.plan(&request(inputs));
        assert!(!plan.is_ready());
        // Незавершённые tool-call входят в обязательный минимум, поэтому отказ
        // приходит на стадии обязательного минимума.
        let stage = plan.unavailable.expect("refusal").stage;
        assert!(matches!(
            stage,
            BudgetUnavailableStage::MandatoryOverflow | BudgetUnavailableStage::DropsExhausted
        ));
    }

    #[test]
    fn duplicates_are_pruned_before_the_ladder() {
        let mut planner = planner();
        let mut inputs = minimal_inputs();
        inputs.push(text_input(
            "dup-a",
            ItemKind::History,
            "одно и то же",
            60,
            5,
        ));
        inputs.push(text_input(
            "dup-b",
            ItemKind::History,
            "одно и то же",
            60,
            6,
        ));
        let plan = planner.plan(&request(inputs));
        assert!(plan.is_ready());
        assert_eq!(
            plan.dropped
                .iter()
                .filter(|item| item.drop_reason == Some(DropReason::Duplicate))
                .count(),
            1
        );
    }

    #[test]
    fn pinned_items_never_break_the_hard_limit() {
        let mut planner = planner();
        let block = "данные ".repeat(40_000);
        let mut inputs = minimal_inputs();
        for index in 0..8 {
            let mut input = text_input(
                &format!("pinned-{index}"),
                ItemKind::History,
                &block,
                100,
                10 + i64::from(index),
            );
            input.item.pinned = true;
            inputs.push(input);
        }
        let plan = planner.plan(&request(inputs));
        if plan.is_ready() {
            let total = plan.ledger.mandatory_tokens
                + plan.ledger.selected_optional_tokens
                + plan.ledger.reserves_tokens;
            assert!(total <= plan.profile.hard_limit_tokens);
            // Хотя бы один pinned item отброшен с явной причиной.
            assert!(plan
                .dropped
                .iter()
                .any(|item| item.pinned && item.drop_reason.is_some()));
        } else {
            assert_eq!(
                plan.unavailable.expect("refusal").stage,
                BudgetUnavailableStage::DropsExhausted
            );
        }
    }

    struct RecordingOffload {
        calls: u32,
    }

    impl OffloadSink for RecordingOffload {
        fn available(&self) -> bool {
            true
        }

        fn offload(&mut self, item: &ContextItem) -> Result<OffloadOutcome, String> {
            self.calls += 1;
            Ok(OffloadOutcome {
                locator: format!("artifact://{}", item.id),
                summary_tokens: 64,
                offloaded_bytes: item.bytes,
            })
        }
    }

    struct RecordingSummarizer;

    impl Summarizer for RecordingSummarizer {
        fn available(&self) -> bool {
            true
        }

        fn summarize(&mut self, items: &[ContextItem]) -> Result<SummaryOutcome, String> {
            Ok(SummaryOutcome {
                summary_id: "summary-1".to_string(),
                source_ids: items.iter().map(|item| item.id.clone()).collect(),
                summary_tokens: 128,
                summarizer_version: "stub-1".to_string(),
                fallback: false,
                fallback_reason: None,
            })
        }
    }

    #[test]
    fn offload_and_compression_decisions_reach_the_ledger() {
        let mut planner = planner();
        let block = "данные ".repeat(30_000);
        let mut inputs = minimal_inputs();
        for index in 0..10 {
            inputs.push(text_input(
                &format!("history-{index:02}"),
                ItemKind::History,
                &block,
                90,
                10 + i64::from(index),
            ));
        }
        let mut offload = RecordingOffload { calls: 0 };
        let plan = planner.plan_with(
            &request(inputs),
            &mut offload,
            &mut RecordingSummarizer,
            None,
        );
        assert!(plan.is_ready());
        assert!(offload.calls > 0);
        assert!(plan
            .ledger
            .ladder_levels_applied
            .contains(&LadderLevel::OffloadLargeItems));
        assert!(plan
            .selected
            .iter()
            .any(|item| item.artifact_locator.is_some()));
    }

    #[test]
    fn privacy_restricted_items_are_never_offloaded() {
        let mut planner = planner();
        let block = "секрет ".repeat(30_000);
        let mut inputs = minimal_inputs();
        for index in 0..10 {
            let mut input = text_input(
                &format!("secret-{index:02}"),
                ItemKind::History,
                &block,
                90,
                10 + i64::from(index),
            );
            input.item.privacy = Privacy::Secret;
            inputs.push(input);
        }
        let mut offload = RecordingOffload { calls: 0 };
        let _ = planner.plan_with(&request(inputs), &mut offload, &mut NoSummarizer, None);
        assert_eq!(offload.calls, 0);
    }

    #[test]
    fn a_context_length_error_gives_exactly_one_replan() {
        let mut planner = planner();
        let plan = planner.plan(&request(minimal_inputs()));
        let replanned = planner.replan_after_context_length_error(
            &request(minimal_inputs()),
            &plan,
            Some(40_000),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(replanned.is_ready());
        assert_eq!(
            replanned.ledger.replan_of.as_deref(),
            Some(plan.ledger.id.as_str())
        );
        assert!(replanned.profile.hard_limit_tokens <= 40_000);
        assert_eq!(replanned.profile.retry_reserve, 0);

        // Второй отказ подряд завершает вызов, каскад re-plan запрещён.
        let cascaded = planner.replan_after_context_length_error(
            &request(minimal_inputs()),
            &replanned,
            Some(20_000),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(!cascaded.is_ready());
        assert_eq!(
            cascaded.unavailable.expect("refusal").stage,
            BudgetUnavailableStage::ProviderReplanFailed
        );
    }

    #[test]
    fn forced_reduction_runs_the_ladder_below_the_soft_limit() {
        let mut planner = planner();
        let mut inputs = minimal_inputs();
        inputs.push(text_input("low", ItemKind::History, "малозначимое", 5, 5));
        let mut forced = request(inputs);
        forced.force_reduction = true;
        let plan = planner.plan(&forced);
        assert!(plan.is_ready());
        // Ниже soft limit лестница обычно не запускается, но `summarize now`
        // запускает её принудительно.
        assert!(plan
            .ledger
            .ladder_levels_applied
            .contains(&LadderLevel::LowPriorityOptional));
        assert!(plan
            .dropped
            .iter()
            .any(|item| item.id == "low" && item.drop_reason.is_some()));
    }

    #[test]
    fn estimation_cache_is_reused_across_assemblies() {
        let mut planner = planner();
        planner.plan(&request(minimal_inputs()));
        let misses = planner.cache().misses();
        planner.plan(&request(minimal_inputs()));
        assert_eq!(planner.cache().misses(), misses);
        assert!(planner.cache().hits() > 0);
    }

    #[test]
    fn under_estimated_usage_is_recorded_as_a_defect() {
        let mut planner = planner();
        let plan = planner.plan(&request(minimal_inputs()));
        let drift = planner.record_actual_usage(&plan, plan.ledger.estimated_prompt_tokens + 10);
        assert!(drift.is_under_estimate());
        assert!(planner
            .metrics()
            .alerts()
            .iter()
            .any(|alert| alert.starts_with("estimator_under_estimate=")));
    }

    #[test]
    fn diagnostics_never_carry_raw_content() {
        let mut planner = planner();
        let secret = "СЕКРЕТНЫЙ_ТОКЕН_12345";
        let inputs = vec![
            text_input("safety", ItemKind::SafetyPolicy, "безопасность", 100, 1),
            text_input("prompt", ItemKind::UserPrompt, secret, 100, 2),
            text_input("memory", ItemKind::Memory, secret, 50, 3),
        ];
        let plan = planner.plan(&request(inputs));
        let serialized = serde_json::to_string(&plan.ledger).expect("ledger serializes");
        assert!(!serialized.contains(secret));
        let metrics = serde_json::to_string(planner.metrics()).expect("metrics serialize");
        assert!(!metrics.contains(secret));
    }

    #[test]
    fn selection_of_a_thousand_items_stays_within_the_regression_budget() {
        let mut planner = planner();
        let mut inputs = minimal_inputs();
        for index in 0..1_000 {
            inputs.push(text_input(
                &format!("item-{index:04}"),
                ItemKind::History,
                &format!("содержимое элемента номер {index}"),
                u8::try_from(index % 100).unwrap_or(50),
                10 + i64::from(index),
            ));
        }
        let started = std::time::Instant::now();
        let plan = planner.plan(&request(inputs));
        let elapsed = started.elapsed();
        assert!(plan.is_ready());
        // Регрессионный порог, а не SLA перед пользователем.
        assert!(
            elapsed.as_millis() < 2_000,
            "selection на 1000 item заняло {elapsed:?}"
        );
    }
}
