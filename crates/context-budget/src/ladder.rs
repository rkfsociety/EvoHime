//! Лестница сокращения контекста (этап 01.1).
//!
//! Лестница конечна, упорядочена и задана заранее. Каждый уровень применяется
//! не более одного раза за сборку и обязан строго уменьшать суммарный размер
//! (`selected_optional_tokens + reserves`); уровень, не давший уменьшения,
//! считается исчерпанным немедленно. Поэтому число итераций ограничено длиной
//! лестницы и цикл завершается всегда.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::item::{ContextItem, DropReason};
use crate::profile::ModelContextProfile;

/// Уровень лестницы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LadderLevel {
    /// L1: expired / duplicate / superseded.
    ExpiredDuplicateSuperseded,
    /// L2: low-priority optional.
    LowPriorityOptional,
    /// L3: самые старые завершённые tool outputs.
    StaleToolOutputs,
    /// L4: offload крупных item в artifact store.
    OffloadLargeItems,
    /// L5: сжатие истории.
    CompressHistory,
    /// L6: отказ от необязательных резервов.
    ReleaseOptionalReserves,
}

impl LadderLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExpiredDuplicateSuperseded => "l1_expired_duplicate_superseded",
            Self::LowPriorityOptional => "l2_low_priority_optional",
            Self::StaleToolOutputs => "l3_stale_tool_outputs",
            Self::OffloadLargeItems => "l4_offload_large_items",
            Self::CompressHistory => "l5_compress_history",
            Self::ReleaseOptionalReserves => "l6_release_optional_reserves",
        }
    }

    /// Полный упорядоченный список уровней.
    pub fn order() -> [Self; 6] {
        [
            Self::ExpiredDuplicateSuperseded,
            Self::LowPriorityOptional,
            Self::StaleToolOutputs,
            Self::OffloadLargeItems,
            Self::CompressHistory,
            Self::ReleaseOptionalReserves,
        ]
    }
}

/// Результат выгрузки item в artifact store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffloadOutcome {
    /// Locator артефакта.
    pub locator: String,
    /// Bounded summary, остающийся в контексте вместо содержимого.
    pub summary_tokens: u32,
    /// Сколько байт ушло в store.
    pub offloaded_bytes: u64,
}

/// Возможность выгрузки крупных item (реализуется этапом 01.2).
pub trait OffloadSink {
    /// Проба доступности. Недоступный store означает, что уровень L4 немедленно
    /// считается исчерпанным, а в ledger пишется diagnostic.
    fn available(&self) -> bool;

    fn offload(&mut self, item: &ContextItem) -> Result<OffloadOutcome, String>;
}

/// Результат сжатия набора item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryOutcome {
    pub summary_id: String,
    pub source_ids: Vec<String>,
    pub summary_tokens: u32,
    pub summarizer_version: String,
    /// Применён ли deterministic fallback вместо вызова summarizer.
    pub fallback: bool,
    pub fallback_reason: Option<String>,
}

/// Возможность сжатия истории (реализуется этапом 01.3).
pub trait Summarizer {
    fn available(&self) -> bool;

    fn summarize(&mut self, items: &[ContextItem]) -> Result<SummaryOutcome, String>;
}

/// Пустая реализация: используется, пока 01.2/01.3 не подключены.
pub struct NoOffload;

impl OffloadSink for NoOffload {
    fn available(&self) -> bool {
        false
    }

    fn offload(&mut self, _item: &ContextItem) -> Result<OffloadOutcome, String> {
        Err("artifact store is not available".to_string())
    }
}

/// Пустая реализация summarizer.
pub struct NoSummarizer;

impl Summarizer for NoSummarizer {
    fn available(&self) -> bool {
        false
    }

    fn summarize(&mut self, _items: &[ContextItem]) -> Result<SummaryOutcome, String> {
        Err("summarizer is not available".to_string())
    }
}

/// Diagnostic уровня лестницы. Bounded: только идентификаторы и причины.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderDiagnostic {
    pub level: LadderLevel,
    pub code: String,
    pub detail: String,
}

/// Итог прохода лестницы.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LadderOutcome {
    pub levels_applied: Vec<LadderLevel>,
    pub diagnostics: Vec<LadderDiagnostic>,
    pub summaries: Vec<SummaryOutcome>,
    pub offloaded_bytes: u64,
    /// Сколько токенов резервов освобождено уровнем L6.
    pub reserves_released: u32,
}

/// Изменяемое состояние сборки, над которым работает лестница.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Все кандидаты. Обязательные item распознаются по kind и не отбрасываются.
    pub items: Vec<ContextItem>,
    /// Текущий размер необязательных резервов.
    pub reserves: u32,
}

impl Selection {
    pub fn new(items: Vec<ContextItem>, reserves: u32) -> Self {
        let mut selection = Self { items, reserves };
        for item in &mut selection.items {
            item.selected = true;
            item.drop_reason = None;
        }
        selection
    }

    /// Токены выбранной обязательной части.
    pub fn mandatory_tokens(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.selected && item.is_mandatory_kind())
            .map(|item| item.estimated_tokens)
            .fold(0, u32::saturating_add)
    }

    /// Токены выбранной необязательной части.
    pub fn optional_tokens(&self) -> u32 {
        self.items
            .iter()
            .filter(|item| item.selected && !item.is_mandatory_kind())
            .map(|item| item.estimated_tokens)
            .fold(0, u32::saturating_add)
    }

    pub fn context_tokens(&self) -> u32 {
        self.mandatory_tokens()
            .saturating_add(self.optional_tokens())
    }

    /// Суммарный размер, который обязан строго уменьшаться на каждом уровне.
    fn total(&self) -> u32 {
        self.optional_tokens().saturating_add(self.reserves)
    }

    pub fn selected(&self) -> impl Iterator<Item = &ContextItem> {
        self.items.iter().filter(|item| item.selected)
    }

    pub fn dropped(&self) -> impl Iterator<Item = &ContextItem> {
        self.items.iter().filter(|item| !item.selected)
    }

    fn drop_item(&mut self, id: &str, reason: DropReason) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            item.selected = false;
            item.drop_reason = Some(reason);
        }
    }

    /// Кандидаты уровня в детерминированном порядке отбрасывания: pinned
    /// последним, затем по возрастанию `effective_priority`, `created_at`,
    /// `content_hash` и `id`.
    fn ordered_candidates<F>(&self, filter: F) -> Vec<String>
    where
        F: Fn(&ContextItem) -> bool,
    {
        let mut candidates: Vec<&ContextItem> = self
            .items
            .iter()
            .filter(|item| item.selected && !item.is_mandatory_kind() && filter(item))
            .collect();
        candidates.sort_by(|left, right| left.drop_order_key().cmp(&right.drop_order_key()));
        candidates.into_iter().map(|item| item.id.clone()).collect()
    }
}

/// Вход лестницы.
pub struct LadderContext<'a> {
    pub now: i64,
    pub profile: &'a ModelContextProfile,
    /// Цель лестницы: `context_tokens <= target_tokens`.
    pub goal_context_tokens: u32,
}

/// Проход лестницы. Возвращает состав применённых уровней и diagnostics.
pub fn run_ladder(
    selection: &mut Selection,
    context: &LadderContext<'_>,
    offload: &mut dyn OffloadSink,
    summarizer: &mut dyn Summarizer,
) -> LadderOutcome {
    let mut outcome = LadderOutcome::default();
    if goal_met(selection, context) {
        return outcome;
    }
    for level in LadderLevel::order() {
        if goal_met(selection, context) {
            break;
        }
        let before = selection.total();
        match level {
            LadderLevel::ExpiredDuplicateSuperseded => apply_l1(selection, context),
            LadderLevel::LowPriorityOptional => apply_l2(selection, context),
            LadderLevel::StaleToolOutputs => apply_l3(selection, context),
            LadderLevel::OffloadLargeItems => {
                apply_l4(selection, context, offload, &mut outcome);
            }
            LadderLevel::CompressHistory => {
                apply_l5(selection, context, summarizer, &mut outcome);
            }
            LadderLevel::ReleaseOptionalReserves => apply_l6(selection, context, &mut outcome),
        }
        if selection.total() < before {
            outcome.levels_applied.push(level);
        }
        // Уровень, не давший уменьшения, считается исчерпанным немедленно и
        // повторно не применяется: следующая итерация переходит к следующему
        // уровню, поэтому цикл ограничен длиной лестницы.
    }
    outcome
}

fn goal_met(selection: &Selection, context: &LadderContext<'_>) -> bool {
    selection.context_tokens() <= context.goal_context_tokens
}

/// L1: истёкшие TTL/retention, дубликаты по `content_hash` и вытесненные ревизии.
fn apply_l1(selection: &mut Selection, context: &LadderContext<'_>) {
    let mut seen_hashes: HashSet<String> = HashSet::new();
    // Обязательные item занимают свои hash первыми: дубликат обязательного
    // item отбрасывается, а не наоборот.
    for item in selection.items.iter().filter(|item| item.is_mandatory_kind()) {
        seen_hashes.insert(item.content_hash.clone());
    }
    // Последняя ревизия по каждому `parent_id` остаётся, предыдущие вытесняются.
    let mut newest_revision: HashMap<String, (u32, i64, String)> = HashMap::new();
    for item in selection.items.iter().filter(|item| item.selected) {
        if let Some(parent) = &item.parent_id {
            let candidate = (item.version, item.created_at, item.id.clone());
            newest_revision
                .entry(parent.clone())
                .and_modify(|current| {
                    if candidate.0 > current.0
                        || (candidate.0 == current.0 && candidate.1 > current.1)
                        || (candidate.0 == current.0
                            && candidate.1 == current.1
                            && candidate.2 > current.2)
                    {
                        *current = candidate.clone();
                    }
                })
                .or_insert(candidate);
        }
    }

    let ordered = selection.ordered_candidates(|_| true);
    // Дубликаты определяются по порядку выбора: первым остаётся item с более
    // высоким `effective_priority`, поэтому обходим в обратном порядке.
    let mut keep_order: Vec<String> = ordered.clone();
    keep_order.reverse();
    let mut duplicates: Vec<String> = Vec::new();
    for id in keep_order {
        let Some(item) = selection.items.iter().find(|item| item.id == id) else {
            continue;
        };
        if !seen_hashes.insert(item.content_hash.clone()) {
            duplicates.push(id);
        }
    }

    for id in ordered {
        if goal_met(selection, context) {
            return;
        }
        let Some(item) = selection.items.iter().find(|item| item.id == id) else {
            continue;
        };
        if !item.selected {
            continue;
        }
        let reason = if item.ttl_expired(context.now) || item.retention_expired(context.now) {
            Some(DropReason::Expired)
        } else if duplicates.contains(&id) {
            Some(DropReason::Duplicate)
        } else if item
            .parent_id
            .as_ref()
            .and_then(|parent| newest_revision.get(parent))
            .is_some_and(|newest| newest.2 != item.id)
        {
            Some(DropReason::Superseded)
        } else {
            None
        };
        if let Some(reason) = reason {
            selection.drop_item(&id, reason);
        }
    }
}

/// L2: `effective_priority < profile.low_priority_cutoff`, item не входит в MVC.
fn apply_l2(selection: &mut Selection, context: &LadderContext<'_>) {
    let cutoff = context.profile.low_priority_cutoff;
    let candidates =
        selection.ordered_candidates(|item| item.effective_priority() < cutoff);
    for id in candidates {
        if goal_met(selection, context) {
            return;
        }
        selection.drop_item(&id, DropReason::LowPriority);
    }
}

/// L3: самые старые завершённые пары tool-call/result.
fn apply_l3(selection: &mut Selection, context: &LadderContext<'_>) {
    let mut candidates: Vec<&ContextItem> = selection
        .items
        .iter()
        .filter(|item| {
            item.selected
                && !item.is_mandatory_kind()
                && item.kind == crate::item::ItemKind::ToolResult
                && item.tool_pair_complete
        })
        .collect();
    // Сортировка по `created_at` возрастанию; pinned остаётся последним.
    candidates.sort_by(|left, right| {
        left.pinned
            .cmp(&right.pinned)
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.content_hash.cmp(&right.content_hash))
            .then_with(|| left.id.cmp(&right.id))
    });
    let ids: Vec<String> = candidates.into_iter().map(|item| item.id.clone()).collect();
    for id in ids {
        if goal_met(selection, context) {
            return;
        }
        selection.drop_item(&id, DropReason::StaleToolOutput);
    }
}

/// L4: offload крупных item в artifact store.
fn apply_l4(
    selection: &mut Selection,
    context: &LadderContext<'_>,
    offload: &mut dyn OffloadSink,
    outcome: &mut LadderOutcome,
) {
    if !offload.available() {
        outcome.diagnostics.push(LadderDiagnostic {
            level: LadderLevel::OffloadLargeItems,
            code: "artifact_store_unavailable".to_string(),
            detail: "artifact store capability probe failed".to_string(),
        });
        return;
    }
    let threshold = context.profile.offload_threshold_bytes;
    let candidates = selection.ordered_candidates(|item| {
        item.bytes > threshold && item.privacy.allows_offload() && item.artifact_locator.is_none()
    });
    for id in candidates {
        if goal_met(selection, context) {
            return;
        }
        let Some(item) = selection.items.iter().find(|item| item.id == id).cloned() else {
            continue;
        };
        match offload.offload(&item) {
            Ok(result) => {
                if result.summary_tokens >= item.estimated_tokens {
                    // Выгрузка, не уменьшающая размер, бессмысленна: пропускаем.
                    continue;
                }
                if let Some(slot) = selection.items.iter_mut().find(|slot| slot.id == id) {
                    slot.estimated_tokens = result.summary_tokens;
                    slot.artifact_locator = Some(result.locator.clone());
                    slot.drop_reason = Some(DropReason::Offloaded);
                }
                outcome.offloaded_bytes = outcome
                    .offloaded_bytes
                    .saturating_add(result.offloaded_bytes);
            }
            Err(error) => {
                // Отказ внутри уже начатого уровня: изменения уровня не
                // применяются к этому item, уровень помечается исчерпанным,
                // исходный item остаётся выбранным.
                outcome.diagnostics.push(LadderDiagnostic {
                    level: LadderLevel::OffloadLargeItems,
                    code: "artifact_write_failed".to_string(),
                    detail: bounded_detail(&error),
                });
                return;
            }
        }
    }
}

/// L5: сжатие истории.
fn apply_l5(
    selection: &mut Selection,
    // Уровень заменяет набор item одним summary целиком, поэтому промежуточная
    // сверка с целью лестницы внутри уровня не нужна.
    _context: &LadderContext<'_>,
    summarizer: &mut dyn Summarizer,
    outcome: &mut LadderOutcome,
) {
    if !summarizer.available() {
        outcome.diagnostics.push(LadderDiagnostic {
            level: LadderLevel::CompressHistory,
            code: "summarizer_unavailable".to_string(),
            detail: "summarizer capability probe failed".to_string(),
        });
        return;
    }
    let candidates: Vec<ContextItem> = selection
        .items
        .iter()
        .filter(|item| {
            item.selected
                && !item.is_mandatory_kind()
                && matches!(
                    item.kind,
                    crate::item::ItemKind::History | crate::item::ItemKind::ToolResult
                )
        })
        .cloned()
        .collect();
    if candidates.len() < 2 {
        outcome.diagnostics.push(LadderDiagnostic {
            level: LadderLevel::CompressHistory,
            code: "summarizer_input_too_small".to_string(),
            detail: format!("{} compressible items", candidates.len()),
        });
        return;
    }
    match summarizer.summarize(&candidates) {
        Ok(summary) => {
            let replaced_tokens: u32 = candidates
                .iter()
                .filter(|item| summary.source_ids.contains(&item.id))
                .map(|item| item.estimated_tokens)
                .fold(0, u32::saturating_add);
            if summary.summary_tokens >= replaced_tokens {
                outcome.diagnostics.push(LadderDiagnostic {
                    level: LadderLevel::CompressHistory,
                    code: "summary_not_smaller".to_string(),
                    detail: format!("{} >= {}", summary.summary_tokens, replaced_tokens),
                });
                return;
            }
            // Исходные items остаются source of truth в ledger/artifact store;
            // summary — только projection текущего model call.
            for id in &summary.source_ids {
                if let Some(slot) = selection.items.iter_mut().find(|item| &item.id == id) {
                    slot.selected = false;
                    slot.drop_reason = None;
                }
            }
            let template = candidates
                .iter()
                .find(|item| summary.source_ids.contains(&item.id))
                .cloned();
            if let Some(template) = template {
                let mut summary_item = template;
                summary_item.id = summary.summary_id.clone();
                summary_item.kind = crate::item::ItemKind::Summary;
                summary_item.estimated_tokens = summary.summary_tokens;
                summary_item.selected = true;
                summary_item.drop_reason = None;
                summary_item.pinned = false;
                selection.items.push(summary_item);
            }
            outcome.summaries.push(summary);
        }
        Err(error) => {
            outcome.diagnostics.push(LadderDiagnostic {
                level: LadderLevel::CompressHistory,
                code: "summarizer_failed".to_string(),
                detail: bounded_detail(&error),
            });
        }
    }
}

/// L6: отказ от необязательных резервов в фиксированном порядке.
/// `tool_call_reserve` и `final_answer_reserve` не сокращаются никогда.
fn apply_l6(
    selection: &mut Selection,
    context: &LadderContext<'_>,
    outcome: &mut LadderOutcome,
) {
    let profile = context.profile;
    // Фиксированный порядок отказа: retry → streaming → tool_schema сверх
    // фактического размера схем.
    let optional_order = [
        profile.retry_reserve,
        profile.streaming_reserve,
        profile.tool_schema_reserve,
    ];
    let protected = profile
        .tool_call_reserve
        .saturating_add(profile.final_answer_reserve);
    let releasable = optional_order
        .iter()
        .copied()
        .fold(0_u32, u32::saturating_add)
        .min(selection.reserves.saturating_sub(protected));
    if releasable == 0 {
        return;
    }
    // Уровень освобождает резервы целиком: они не влияют на `context_tokens`,
    // поэтому частичное освобождение не приблизило бы цель лестницы, а только
    // сделало бы результат зависимым от порядка проверок.
    let mut released = 0_u32;
    for amount in optional_order {
        if released >= releasable {
            break;
        }
        let step = amount.min(releasable - released);
        released += step;
        selection.reserves = selection.reserves.saturating_sub(step);
    }
    outcome.reserves_released = released;
}

fn bounded_detail(detail: &str) -> String {
    detail.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ContextItemBuilder, ItemKind, Privacy, ScratchpadStatus};

    fn profile() -> ModelContextProfile {
        ModelContextProfile::fallback("p", "m", 128_000)
    }

    fn optional(id: &str, tokens: u32) -> ContextItem {
        ContextItemBuilder::new(id, ItemKind::History, format!("hash-{id}"))
            .sizes(u64::from(tokens) * 3, tokens)
            .created_at(1_000)
            .build()
    }

    fn context<'a>(profile: &'a ModelContextProfile, goal: u32) -> LadderContext<'a> {
        LadderContext {
            now: 10_000,
            profile,
            goal_context_tokens: goal,
        }
    }

    #[test]
    fn ladder_stops_immediately_when_the_goal_is_already_met() {
        let profile = profile();
        let mut selection = Selection::new(vec![optional("a", 10)], 100);
        let outcome = run_ladder(
            &mut selection,
            &context(&profile, 1_000),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(outcome.levels_applied.is_empty());
        assert_eq!(selection.selected().count(), 1);
    }

    #[test]
    fn l1_drops_expired_duplicate_and_superseded_items() {
        let profile = profile();
        let expired = ContextItemBuilder::new("expired", ItemKind::History, "hash-expired")
            .created_at(0)
            .ttl_ms(100)
            .sizes(30, 10)
            .build();
        let original = ContextItemBuilder::new("original", ItemKind::History, "shared")
            .created_at(1_000)
            .priority(70)
            .sizes(30, 10)
            .build();
        let duplicate = ContextItemBuilder::new("duplicate", ItemKind::History, "shared")
            .created_at(2_000)
            .priority(40)
            .sizes(30, 10)
            .build();
        let old_revision = ContextItemBuilder::new("rev1", ItemKind::Decision, "hash-rev1")
            .created_at(1_000)
            .parent("decision-key")
            .version(1)
            .sizes(30, 10)
            .build();
        let new_revision = ContextItemBuilder::new("rev2", ItemKind::Decision, "hash-rev2")
            .created_at(2_000)
            .parent("decision-key")
            .version(2)
            .sizes(30, 10)
            .build();
        let mut selection = Selection::new(
            vec![expired, original, duplicate, old_revision, new_revision],
            0,
        );
        let outcome = run_ladder(
            &mut selection,
            &context(&profile, 0),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(outcome
            .levels_applied
            .contains(&LadderLevel::ExpiredDuplicateSuperseded));
        let reason = |id: &str| {
            selection
                .items
                .iter()
                .find(|item| item.id == id)
                .and_then(|item| item.drop_reason)
        };
        assert_eq!(reason("expired"), Some(DropReason::Expired));
        assert_eq!(reason("duplicate"), Some(DropReason::Duplicate));
        assert_eq!(reason("rev1"), Some(DropReason::Superseded));
        assert_eq!(reason("rev2"), None);
    }

    #[test]
    fn l2_drops_only_items_below_the_cutoff() {
        let profile = profile();
        let low = ContextItemBuilder::new("low", ItemKind::History, "hash-low")
            .priority(10)
            .sizes(30, 100)
            .created_at(1)
            .build();
        let high = ContextItemBuilder::new("high", ItemKind::History, "hash-high")
            .priority(80)
            .sizes(30, 100)
            .created_at(2)
            .build();
        let mut selection = Selection::new(vec![low, high], 0);
        run_ladder(
            &mut selection,
            &context(&profile, 100),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert_eq!(
            selection
                .items
                .iter()
                .find(|item| item.id == "low")
                .and_then(|item| item.drop_reason),
            Some(DropReason::LowPriority)
        );
        assert!(selection
            .items
            .iter()
            .find(|item| item.id == "high")
            .is_some_and(|item| item.selected));
    }

    #[test]
    fn l3_keeps_incomplete_tool_pairs() {
        let profile = profile();
        let complete = ContextItemBuilder::new("complete", ItemKind::ToolResult, "hash-complete")
            .created_at(1)
            .sizes(30, 100)
            .build();
        let incomplete =
            ContextItemBuilder::new("incomplete", ItemKind::ToolResult, "hash-incomplete")
                .created_at(2)
                .sizes(30, 100)
                .tool_pair_complete(false)
                .build();
        let mut selection = Selection::new(vec![complete, incomplete], 0);
        run_ladder(
            &mut selection,
            &context(&profile, 100),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert_eq!(
            selection
                .items
                .iter()
                .find(|item| item.id == "complete")
                .and_then(|item| item.drop_reason),
            Some(DropReason::StaleToolOutput)
        );
        assert!(selection
            .items
            .iter()
            .find(|item| item.id == "incomplete")
            .is_some_and(|item| item.selected));
    }

    #[test]
    fn unavailable_capabilities_skip_l4_and_l5_with_diagnostics() {
        let profile = profile();
        let big = ContextItemBuilder::new("big", ItemKind::ToolResult, "hash-big")
            .sizes(1_000_000, 5_000)
            .created_at(1)
            .priority(90)
            .tool_pair_complete(false)
            .build();
        let other = ContextItemBuilder::new("other", ItemKind::History, "hash-other")
            .sizes(1_000_000, 5_000)
            .created_at(2)
            .priority(90)
            .build();
        let mut selection = Selection::new(vec![big, other], 0);
        let outcome = run_ladder(
            &mut selection,
            &context(&profile, 0),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        let codes: Vec<&str> = outcome
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        assert!(codes.contains(&"artifact_store_unavailable"));
        assert!(codes.contains(&"summarizer_unavailable"));
        assert!(!outcome.levels_applied.contains(&LadderLevel::OffloadLargeItems));
    }

    struct StubOffload;

    impl OffloadSink for StubOffload {
        fn available(&self) -> bool {
            true
        }

        fn offload(&mut self, item: &ContextItem) -> Result<OffloadOutcome, String> {
            Ok(OffloadOutcome {
                locator: format!("artifact://{}", item.id),
                summary_tokens: 32,
                offloaded_bytes: item.bytes,
            })
        }
    }

    #[test]
    fn l4_replaces_large_items_with_a_bounded_summary() {
        let profile = profile();
        let big = ContextItemBuilder::new("big", ItemKind::History, "hash-big")
            .sizes(1_000_000, 5_000)
            .created_at(1)
            .priority(90)
            .build();
        let mut selection = Selection::new(vec![big], 0);
        let outcome = run_ladder(
            &mut selection,
            &context(&profile, 100),
            &mut StubOffload,
            &mut NoSummarizer,
        );
        assert!(outcome.levels_applied.contains(&LadderLevel::OffloadLargeItems));
        let item = &selection.items[0];
        assert_eq!(item.estimated_tokens, 32);
        assert_eq!(item.drop_reason, Some(DropReason::Offloaded));
        assert_eq!(item.artifact_locator.as_deref(), Some("artifact://big"));
        assert_eq!(outcome.offloaded_bytes, 1_000_000);
    }

    #[test]
    fn l4_skips_items_that_privacy_forbids_offloading() {
        let profile = profile();
        let secret = ContextItemBuilder::new("secret", ItemKind::History, "hash-secret")
            .sizes(1_000_000, 5_000)
            .privacy(Privacy::Secret)
            .priority(90)
            .created_at(1)
            .build();
        let mut selection = Selection::new(vec![secret], 0);
        run_ladder(
            &mut selection,
            &context(&profile, 100),
            &mut StubOffload,
            &mut NoSummarizer,
        );
        assert!(selection.items[0].artifact_locator.is_none());
    }

    struct StubSummarizer;

    impl Summarizer for StubSummarizer {
        fn available(&self) -> bool {
            true
        }

        fn summarize(&mut self, items: &[ContextItem]) -> Result<SummaryOutcome, String> {
            Ok(SummaryOutcome {
                summary_id: "summary-1".to_string(),
                source_ids: items.iter().map(|item| item.id.clone()).collect(),
                summary_tokens: 64,
                summarizer_version: "stub-1".to_string(),
                fallback: false,
                fallback_reason: None,
            })
        }
    }

    #[test]
    fn l5_replaces_sources_with_one_summary_item() {
        let profile = profile();
        let mut selection = Selection::new(
            vec![
                ContextItemBuilder::new("h1", ItemKind::History, "hash-h1")
                    .priority(90)
                    .sizes(3_000, 1_000)
                    .created_at(1)
                    .build(),
                ContextItemBuilder::new("h2", ItemKind::History, "hash-h2")
                    .priority(90)
                    .sizes(3_000, 1_000)
                    .created_at(2)
                    .build(),
            ],
            0,
        );
        let outcome = run_ladder(
            &mut selection,
            &context(&profile, 100),
            &mut NoOffload,
            &mut StubSummarizer,
        );
        assert!(outcome.levels_applied.contains(&LadderLevel::CompressHistory));
        assert_eq!(outcome.summaries.len(), 1);
        assert_eq!(selection.selected().count(), 1);
        assert_eq!(selection.context_tokens(), 64);
    }

    #[test]
    fn l6_never_releases_tool_call_or_final_answer_reserves() {
        let profile = profile();
        let mut selection = Selection::new(
            vec![ContextItemBuilder::new("h", ItemKind::History, "hash-h")
                .priority(90)
                .sizes(3_000, 1_000)
                .created_at(1)
                .build()],
            profile.reserves_total(),
        );
        let mut outcome = LadderOutcome::default();
        apply_l6(&mut selection, &context(&profile, 0), &mut outcome);
        let protected = profile.tool_call_reserve + profile.final_answer_reserve;
        assert_eq!(selection.reserves, protected);
        assert_eq!(
            outcome.reserves_released,
            profile.reserves_total() - protected
        );
    }

    #[test]
    fn pinned_items_are_dropped_last_inside_a_level() {
        let profile = profile();
        let pinned = ContextItemBuilder::new("pinned", ItemKind::History, "hash-pinned")
            .priority(5)
            .pinned(true)
            .sizes(30, 100)
            .created_at(1)
            .build();
        let plain = ContextItemBuilder::new("plain", ItemKind::History, "hash-plain")
            .priority(5)
            .sizes(30, 100)
            .created_at(2)
            .build();
        let mut selection = Selection::new(vec![pinned, plain], 0);
        // Цель достигается отбрасыванием ровно одного item.
        run_ladder(
            &mut selection,
            &context(&profile, 100),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(selection
            .items
            .iter()
            .find(|item| item.id == "pinned")
            .is_some_and(|item| item.selected));
        assert!(selection
            .items
            .iter()
            .find(|item| item.id == "plain")
            .is_some_and(|item| !item.selected));
    }

    #[test]
    fn mandatory_items_are_never_dropped() {
        let profile = profile();
        let safety = ContextItemBuilder::new("safety", ItemKind::SafetyPolicy, "hash-safety")
            .priority(0)
            .sizes(30, 5_000)
            .created_at(1)
            .build();
        let mut selection = Selection::new(vec![safety], 0);
        run_ladder(
            &mut selection,
            &context(&profile, 0),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(selection.items[0].selected);
    }

    #[test]
    fn recovered_scratchpad_entries_are_low_priority_candidates() {
        let profile = profile();
        let recovered = ContextItemBuilder::new("rec", ItemKind::Scratchpad, "hash-rec")
            .priority(95)
            .scratchpad_status(ScratchpadStatus::Recovered)
            .sizes(30, 100)
            .created_at(1)
            .build();
        let mut selection = Selection::new(vec![recovered], 0);
        run_ladder(
            &mut selection,
            &context(&profile, 0),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert_eq!(
            selection.items[0].drop_reason,
            Some(DropReason::LowPriority)
        );
    }

    #[test]
    fn every_level_is_applied_at_most_once_and_the_ladder_terminates() {
        let profile = profile();
        let items: Vec<ContextItem> = (0..200)
            .map(|index| {
                ContextItemBuilder::new(
                    format!("item-{index:03}"),
                    ItemKind::History,
                    format!("hash-{index}"),
                )
                .priority(50)
                .sizes(3_000, 1_000)
                .created_at(i64::from(index))
                .build()
            })
            .collect();
        let mut selection = Selection::new(items, profile.reserves_total());
        let outcome = run_ladder(
            &mut selection,
            &context(&profile, 1_000),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert!(outcome.levels_applied.len() <= LadderLevel::order().len());
        let mut unique = outcome.levels_applied.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), outcome.levels_applied.len());
    }

    #[test]
    fn the_same_input_produces_the_same_ladder_trace() {
        let profile = profile();
        let build = || {
            Selection::new(
                (0..50)
                    .map(|index| {
                        ContextItemBuilder::new(
                            format!("item-{index:03}"),
                            ItemKind::History,
                            format!("hash-{index}"),
                        )
                        .priority(u8::try_from(index % 100).unwrap_or(50))
                        .sizes(3_000, 1_000)
                        .created_at(i64::from(index))
                        .build()
                    })
                    .collect(),
                profile.reserves_total(),
            )
        };
        let mut first = build();
        let mut second = build();
        let left = run_ladder(
            &mut first,
            &context(&profile, 5_000),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        let right = run_ladder(
            &mut second,
            &context(&profile, 5_000),
            &mut NoOffload,
            &mut NoSummarizer,
        );
        assert_eq!(left.levels_applied, right.levels_applied);
        let ids = |selection: &Selection| {
            selection
                .items
                .iter()
                .map(|item| (item.id.clone(), item.selected, item.drop_reason))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&first), ids(&second));
    }
}
