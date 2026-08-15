//! Compression и pruning (этап 01.3).
//!
//! Собственного внешнего контракта у этапа нет: наружу уходят только записи
//! ledger из 01.1 — `drop_reason`, связь `summary_id -> source_ids`, версия и
//! параметры summarizer, fallback-флаг и причина fallback.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::item::{ContextItem, DropReason, ItemKind, ScratchpadStatus, Trust};
use crate::ladder::{SummaryOutcome, Summarizer};

/// Версия стратегии сжатия и pruning.
pub const COMPRESSION_STRATEGY_VERSION: &str = "compress-1";

/// Уровень иерархии прав. Больше значит выше: новая запись не может понизить
/// более высокий уровень, а recency и trust работают только как тай-брейк
/// внутри одного уровня.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyLevel {
    /// recovered / unverified.
    RecoveredUnverified,
    /// history и данные инструментов.
    HistoryToolData,
    /// подтверждённые решения и факты задачи.
    ConfirmedTaskFacts,
    /// явные ограничения пользователя.
    UserConstraints,
    /// системные инструкции.
    SystemInstructions,
    /// safety/hard-deny и approval policy.
    SafetyApproval,
}

impl HierarchyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecoveredUnverified => "recovered_unverified",
            Self::HistoryToolData => "history_tool_data",
            Self::ConfirmedTaskFacts => "confirmed_task_facts",
            Self::UserConstraints => "user_constraints",
            Self::SystemInstructions => "system_instructions",
            Self::SafetyApproval => "safety_approval",
        }
    }

    /// Уровень item. `recovered`/`unverified` всегда опускается на нижний
    /// уровень независимо от kind: непроверенная запись не действует как
    /// инструкция.
    pub fn of(item: &ContextItem) -> Self {
        if item.scratchpad_status == Some(ScratchpadStatus::Recovered)
            || item.trust == Trust::Unverified
        {
            return Self::RecoveredUnverified;
        }
        match item.kind {
            ItemKind::SafetyPolicy | ItemKind::ApprovalPolicy => Self::SafetyApproval,
            ItemKind::SystemInstruction | ItemKind::Cancellation => Self::SystemInstructions,
            ItemKind::UserPrompt | ItemKind::UserConstraint => Self::UserConstraints,
            ItemKind::Decision | ItemKind::Memory | ItemKind::Scratchpad => {
                Self::ConfirmedTaskFacts
            }
            ItemKind::History
            | ItemKind::ToolResult
            | ItemKind::PendingToolCall
            | ItemKind::ToolSchema
            | ItemKind::Evidence
            | ItemKind::Summary => Self::HistoryToolData,
        }
    }
}

/// Детерминированный порядок item в собранном контексте: уровень иерархии по
/// убыванию, затем trust по убыванию, затем свежесть, затем `content_hash` и
/// `id` лексикографически. Сортировка стабильна, поэтому одинаковый вход даёт
/// одинаковый порядок и одинаковый `context_ledger_hash`.
pub fn order_items(items: &mut [ContextItem]) {
    items.sort_by(|left, right| {
        HierarchyLevel::of(right)
            .cmp(&HierarchyLevel::of(left))
            .then_with(|| right.trust.cmp(&left.trust))
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.content_hash.cmp(&right.content_hash))
            .then_with(|| left.id.cmp(&right.id))
    });
}

/// Исход сравнения двух записей одного ключа.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Существенный конфликт: нужен пользовательский confirmation.
    UserConfirm,
    /// Побеждает более доверенная запись (только внутри одного уровня).
    KeepHigherTrust,
    /// Побеждает более свежая запись (только внутри одного уровня).
    KeepNewer,
    /// Конфликта нет: запись вытеснена по смыслу новой ревизией.
    Superseded,
}

impl ConflictResolution {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserConfirm => "user_confirm",
            Self::KeepHigherTrust => "keep_higher_trust",
            Self::KeepNewer => "keep_newer",
            Self::Superseded => "superseded",
        }
    }
}

/// Обнаруженный конфликт. Помечается label `conflicting`, а не разрешается
/// silent override.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub key: String,
    pub left_id: String,
    pub right_id: String,
    pub resolution: ConflictResolution,
    /// Bounded причина: чем именно записи расходятся.
    pub detail: String,
}

/// Детерминированное сравнение содержимого двух записей одного ключа.
/// Semantic-детекция противоречий не применяется: неоднозначные случаи
/// помечаются `conflicting` и решаются пользователем.
pub fn detect_conflict(
    key: &str,
    left: &ContextItem,
    left_content: &str,
    right: &ContextItem,
    right_content: &str,
) -> Option<Conflict> {
    if left.content_hash == right.content_hash {
        return None;
    }
    let left_level = HierarchyLevel::of(left);
    let right_level = HierarchyLevel::of(right);

    let mut differences = Vec::new();
    if extract_numbers(left_content) != extract_numbers(right_content) {
        differences.push("numbers");
    }
    if extract_paths(left_content) != extract_paths(right_content) {
        differences.push("paths");
    }
    if extract_identifiers(left_content) != extract_identifiers(right_content) {
        differences.push("identifiers");
    }
    if has_negation(left_content) != has_negation(right_content) {
        differences.push("negation");
    }
    if differences.is_empty() {
        // Другое содержимое без расхождения по числам, путям, идентификаторам
        // и отрицанию — это новая ревизия того же ключа, а не противоречие.
        return Some(Conflict {
            key: key.to_string(),
            left_id: left.id.clone(),
            right_id: right.id.clone(),
            resolution: ConflictResolution::Superseded,
            detail: "revision without substantive divergence".to_string(),
        });
    }

    // Между уровнями свежесть не даёт преимущества: более низкий уровень не
    // может понизить более высокий.
    let resolution = if left_level != right_level {
        ConflictResolution::UserConfirm
    } else if left.trust != right.trust {
        ConflictResolution::KeepHigherTrust
    } else if left.created_at != right.created_at {
        ConflictResolution::KeepNewer
    } else {
        ConflictResolution::UserConfirm
    };

    // Существенные факты по умолчанию требуют подтверждения пользователя.
    let resolution = if matches!(left.kind, ItemKind::Decision) || matches!(right.kind, ItemKind::Decision)
    {
        ConflictResolution::UserConfirm
    } else {
        resolution
    };

    Some(Conflict {
        key: key.to_string(),
        left_id: left.id.clone(),
        right_id: right.id.clone(),
        resolution,
        detail: differences.join(","),
    })
}

fn extract_numbers(text: &str) -> Vec<String> {
    let mut numbers = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() || (character == '.' && !current.is_empty()) {
            current.push(character);
        } else if !current.is_empty() {
            numbers.push(current.trim_end_matches('.').to_string());
            current.clear();
        }
    }
    if !current.is_empty() {
        numbers.push(current.trim_end_matches('.').to_string());
    }
    numbers.sort();
    numbers
}

fn extract_paths(text: &str) -> Vec<String> {
    let mut paths: Vec<String> = text
        .split_whitespace()
        .filter(|token| token.contains('/') || token.contains('\\'))
        .map(|token| token.trim_matches(|c: char| c == ',' || c == '.' || c == ';').to_string())
        .collect();
    paths.sort();
    paths
}

fn extract_identifiers(text: &str) -> Vec<String> {
    let mut identifiers: Vec<String> = text
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .filter(|token| {
            token.len() >= 8 && token.chars().any(|c| c.is_ascii_digit()) && token.chars().any(|c| c.is_alphabetic())
        })
        .map(str::to_string)
        .collect();
    identifiers.sort();
    identifiers.dedup();
    identifiers
}

fn has_negation(text: &str) -> bool {
    let lowered = text.to_lowercase();
    ["не ", "нет", "никогда", " not ", " no ", "never", "запрещ"]
        .iter()
        .any(|marker| lowered.contains(marker))
}

/// Разграничение причин удаления при pruning до запуска лестницы.
/// `duplicate` — совпадение `content_hash`; остаётся экземпляр с более высоким
/// уровнем иерархии, при равенстве — более свежий. `superseded` — новая ревизия
/// того же `parent_id`/ключа при другом содержимом. `expired` — истёк TTL или
/// retention независимо от содержимого.
pub fn prune(items: &mut [ContextItem], now: i64) -> Vec<(String, DropReason)> {
    let mut decisions: Vec<(String, DropReason)> = Vec::new();

    // Expired считается первым: истечение не зависит от содержимого.
    for item in items.iter_mut() {
        if item.ttl_expired(now) || item.retention_expired(now) {
            item.selected = false;
            item.drop_reason = Some(DropReason::Expired);
            decisions.push((item.id.clone(), DropReason::Expired));
        }
    }

    // Duplicate: побеждает более высокий уровень иерархии, при равенстве —
    // более свежий, затем детерминированный тай-брейк по id.
    let mut winners: HashMap<String, String> = HashMap::new();
    let mut ranked: Vec<(String, String, HierarchyLevel, i64)> = items
        .iter()
        .filter(|item| item.selected)
        .map(|item| {
            (
                item.content_hash.clone(),
                item.id.clone(),
                HierarchyLevel::of(item),
                item.created_at,
            )
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then_with(|| right.3.cmp(&left.3))
            .then_with(|| left.1.cmp(&right.1))
    });
    for (hash, id, _, _) in ranked {
        winners.entry(hash).or_insert(id);
    }
    for item in items.iter_mut() {
        if !item.selected {
            continue;
        }
        if winners
            .get(&item.content_hash)
            .is_some_and(|winner| winner != &item.id)
        {
            item.selected = false;
            item.drop_reason = Some(DropReason::Duplicate);
            decisions.push((item.id.clone(), DropReason::Duplicate));
        }
    }

    // Superseded: новая ревизия того же `parent_id` при другом содержимом.
    let mut newest: HashMap<String, (u32, i64, String)> = HashMap::new();
    for item in items.iter().filter(|item| item.selected) {
        let Some(parent) = &item.parent_id else {
            continue;
        };
        let candidate = (item.version, item.created_at, item.id.clone());
        newest
            .entry(parent.clone())
            .and_modify(|current| {
                if candidate > *current {
                    *current = candidate.clone();
                }
            })
            .or_insert(candidate);
    }
    for item in items.iter_mut() {
        if !item.selected {
            continue;
        }
        let Some(parent) = item.parent_id.clone() else {
            continue;
        };
        if newest
            .get(&parent)
            .is_some_and(|winner| winner.2 != item.id)
        {
            item.selected = false;
            item.drop_reason = Some(DropReason::Superseded);
            decisions.push((item.id.clone(), DropReason::Superseded));
        }
    }

    decisions.sort();
    decisions
}

/// Параметры bounded summarizer: собственный `summary_budget`, входной лимит и
/// запрет tool calls/retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SummarizerConfig {
    pub summary_budget_tokens: u32,
    pub input_limit_tokens: u32,
    pub version: String,
    /// Вызов summarizer не может вызывать инструменты и не повторяется.
    pub tools_allowed: bool,
    pub retries_allowed: bool,
}

impl Default for SummarizerConfig {
    fn default() -> Self {
        Self {
            summary_budget_tokens: 512,
            input_limit_tokens: 16_384,
            version: COMPRESSION_STRATEGY_VERSION.to_string(),
            tools_allowed: false,
            retries_allowed: false,
        }
    }
}

/// Результат вызова модели-суммаризатора до проверок.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawSummary {
    pub summary_id: String,
    pub source_ids: Vec<String>,
    pub estimated_tokens: u32,
    /// Текст summary — нужен только для schema/policy-проверки, наружу не идёт.
    pub text: String,
}

/// Модельный summarizer. Реализация подставляется Core: это вызов того же model
/// gateway с отдельным low-cost profile.
pub trait SummaryModel {
    fn available(&self) -> bool;

    fn summarize(&mut self, items: &[ContextItem], config: &SummarizerConfig)
        -> Result<RawSummary, String>;
}

/// Bounded summarizer с deterministic fallback. Если модель недоступна,
/// превышает бюджет или возвращает invalid output, применяется fallback без
/// каскадного повторного запуска.
pub struct BoundedSummarizer<M: SummaryModel> {
    model: Option<M>,
    config: SummarizerConfig,
    /// Уже был выполнен один вызов модели: каскадный повтор запрещён.
    model_attempted: bool,
}

impl<M: SummaryModel> BoundedSummarizer<M> {
    pub fn new(model: Option<M>, config: SummarizerConfig) -> Self {
        Self {
            model,
            config,
            model_attempted: false,
        }
    }

    pub fn config(&self) -> &SummarizerConfig {
        &self.config
    }

    /// Deterministic fallback: сохраняются system/policy/approval/user
    /// constraints, подтверждённые факты, числа, пути, отрицания и валидные пары
    /// tool-call/result; середина сообщения не режется.
    fn fallback_summary(&self, items: &[ContextItem], reason: &str) -> SummaryOutcome {
        // Fallback не режет содержимое: он выбирает, какие целые item заменить
        // одной ссылкой. Сначала идут expired/duplicate/low-priority, затем
        // самые старые tool outputs.
        let mut candidates: Vec<&ContextItem> = items
            .iter()
            .filter(|item| {
                HierarchyLevel::of(item) <= HierarchyLevel::HistoryToolData
                    && item.tool_pair_complete
                    && !item.is_mandatory_kind()
            })
            .collect();
        candidates.sort_by(|left, right| {
            left.effective_priority()
                .cmp(&right.effective_priority())
                .then_with(|| left.created_at.cmp(&right.created_at))
                .then_with(|| left.content_hash.cmp(&right.content_hash))
                .then_with(|| left.id.cmp(&right.id))
        });
        let source_ids: Vec<String> = candidates.iter().map(|item| item.id.clone()).collect();
        let summary_id = format!(
            "summary-fallback-{}",
            crate::hash::sha256_hex(&source_ids.join(","))
                .chars()
                .take(16)
                .collect::<String>()
        );
        SummaryOutcome {
            summary_id,
            source_ids,
            // Fallback не вызывает модель, поэтому размер ссылки фиксирован и
            // заведомо меньше исходных item.
            summary_tokens: 64.min(self.config.summary_budget_tokens),
            summarizer_version: format!("{}+fallback", self.config.version),
            fallback: true,
            fallback_reason: Some(bounded_reason(reason)),
        }
    }

    /// Summary принимается только после schema-, estimator- и policy-проверки;
    /// при частично испорченном результате весь результат отклоняется, исходные
    /// items не удаляются.
    fn validate(&self, raw: &RawSummary, items: &[ContextItem]) -> Result<(), String> {
        if raw.summary_id.is_empty() {
            return Err("summary_id is empty".to_string());
        }
        if raw.source_ids.is_empty() {
            return Err("source_ids is empty".to_string());
        }
        if raw.text.trim().is_empty() {
            return Err("summary text is empty".to_string());
        }
        if raw.estimated_tokens > self.config.summary_budget_tokens {
            return Err(format!(
                "summary exceeds summary_budget: {} > {}",
                raw.estimated_tokens, self.config.summary_budget_tokens
            ));
        }
        for id in &raw.source_ids {
            let Some(item) = items.iter().find(|item| &item.id == id) else {
                return Err(format!("unknown source id {id}"));
            };
            if item.is_mandatory_kind() {
                return Err(format!("mandatory item {id} cannot be summarized"));
            }
            if !item.tool_pair_complete {
                return Err(format!("incomplete tool pair {id} cannot be summarized"));
            }
        }
        Ok(())
    }
}

impl<M: SummaryModel> Summarizer for BoundedSummarizer<M> {
    fn available(&self) -> bool {
        // Fallback доступен всегда, поэтому уровень L5 не пропускается только
        // из-за отсутствия модели.
        true
    }

    fn summarize(&mut self, items: &[ContextItem]) -> Result<SummaryOutcome, String> {
        if items.len() < 2 {
            return Err("summarizer needs at least two items".to_string());
        }
        let input_tokens: u32 = items
            .iter()
            .map(|item| item.estimated_tokens)
            .fold(0, u32::saturating_add);
        if input_tokens > self.config.input_limit_tokens {
            return Ok(self.fallback_summary(items, "input exceeds summarizer input limit"));
        }
        if self.model_attempted {
            return Ok(self.fallback_summary(items, "summarizer already attempted for this call"));
        }
        let config = self.config.clone();
        let model_available = self.model.as_ref().is_some_and(SummaryModel::available);
        if !model_available {
            return Ok(self.fallback_summary(items, "summarizer model is unavailable"));
        }
        self.model_attempted = true;
        let raw = match self.model.as_mut().expect("model present").summarize(items, &config) {
            Ok(raw) => raw,
            Err(error) => return Ok(self.fallback_summary(items, &error)),
        };
        if let Err(error) = self.validate(&raw, items) {
            return Ok(self.fallback_summary(items, &error));
        }
        Ok(SummaryOutcome {
            summary_id: raw.summary_id,
            source_ids: raw.source_ids,
            summary_tokens: raw.estimated_tokens,
            summarizer_version: config.version,
            fallback: false,
            fallback_reason: None,
        })
    }
}

fn bounded_reason(reason: &str) -> String {
    reason.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ContextItemBuilder, Privacy};

    fn item(id: &str, kind: ItemKind, hash: &str) -> ContextItem {
        ContextItemBuilder::new(id, kind, hash)
            .sizes(300, 100)
            .created_at(1_000)
            .privacy(Privacy::Workspace)
            .build()
    }

    #[test]
    fn hierarchy_puts_safety_above_everything_else() {
        let safety = item("s", ItemKind::SafetyPolicy, "h1");
        let tool = item("t", ItemKind::ToolResult, "h2");
        assert!(HierarchyLevel::of(&safety) > HierarchyLevel::of(&tool));
    }

    #[test]
    fn recovered_entries_drop_to_the_lowest_level_regardless_of_kind() {
        let mut decision = item("d", ItemKind::Decision, "h1");
        decision.scratchpad_status = Some(ScratchpadStatus::Recovered);
        assert_eq!(
            HierarchyLevel::of(&decision),
            HierarchyLevel::RecoveredUnverified
        );
    }

    #[test]
    fn fresh_tool_output_never_outranks_a_user_constraint() {
        let mut items = vec![
            {
                let mut tool = item("tool", ItemKind::ToolResult, "h-tool");
                tool.created_at = 9_999;
                tool
            },
            {
                let mut constraint = item("constraint", ItemKind::UserConstraint, "h-constraint");
                constraint.created_at = 1;
                constraint
            },
        ];
        order_items(&mut items);
        assert_eq!(items[0].id, "constraint");
    }

    #[test]
    fn ordering_is_stable_for_identical_input() {
        let build = || {
            vec![
                item("b", ItemKind::History, "h-b"),
                item("a", ItemKind::History, "h-a"),
                item("c", ItemKind::History, "h-c"),
            ]
        };
        let mut left = build();
        let mut right = build();
        order_items(&mut left);
        order_items(&mut right);
        let ids = |items: &[ContextItem]| {
            items
                .iter()
                .map(|item| item.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&left), ids(&right));
    }

    #[test]
    fn prune_separates_expired_duplicate_and_superseded() {
        let mut expired = item("expired", ItemKind::History, "h-expired");
        expired.created_at = 0;
        expired.ttl_ms = Some(10);
        expired.selected = true;

        let mut original = item("original", ItemKind::Decision, "shared");
        original.created_at = 2_000;
        original.selected = true;
        let mut duplicate = item("duplicate", ItemKind::History, "shared");
        duplicate.created_at = 3_000;
        duplicate.selected = true;

        let mut revision_one = item("rev1", ItemKind::Decision, "h-rev1");
        revision_one.parent_id = Some("key".to_string());
        revision_one.version = 1;
        revision_one.selected = true;
        let mut revision_two = item("rev2", ItemKind::Decision, "h-rev2");
        revision_two.parent_id = Some("key".to_string());
        revision_two.version = 2;
        revision_two.selected = true;

        let mut items = vec![expired, original, duplicate, revision_one, revision_two];
        let decisions = prune(&mut items, 5_000);
        let reason = |id: &str| {
            items
                .iter()
                .find(|item| item.id == id)
                .and_then(|item| item.drop_reason)
        };
        assert_eq!(reason("expired"), Some(DropReason::Expired));
        // Decision стоит выше History в иерархии, поэтому дубликат-history уходит.
        assert_eq!(reason("duplicate"), Some(DropReason::Duplicate));
        assert_eq!(reason("original"), None);
        assert_eq!(reason("rev1"), Some(DropReason::Superseded));
        assert_eq!(reason("rev2"), None);
        assert_eq!(decisions.len(), 3);
    }

    #[test]
    fn number_divergence_is_a_substantive_conflict() {
        let left = item("l", ItemKind::Memory, "h-l");
        let right = item("r", ItemKind::Memory, "h-r");
        let conflict = detect_conflict("entity/port", &left, "порт 8080", &right, "порт 9090")
            .expect("conflict detected");
        assert_eq!(conflict.detail, "numbers");
        assert_ne!(conflict.resolution, ConflictResolution::Superseded);
    }

    #[test]
    fn negation_divergence_is_a_substantive_conflict() {
        let left = item("l", ItemKind::Memory, "h-l");
        let right = item("r", ItemKind::Memory, "h-r");
        let conflict = detect_conflict("entity/flag", &left, "сервис включён", &right, "сервис не включён")
            .expect("conflict detected");
        assert!(conflict.detail.contains("negation"));
    }

    #[test]
    fn identical_content_is_never_a_conflict() {
        let left = item("l", ItemKind::Memory, "same");
        let right = item("r", ItemKind::Memory, "same");
        assert!(detect_conflict("k", &left, "a", &right, "a").is_none());
    }

    #[test]
    fn cross_level_conflicts_require_user_confirmation() {
        let policy = item("policy", ItemKind::SafetyPolicy, "h-policy");
        let tool = item("tool", ItemKind::ToolResult, "h-tool");
        let conflict = detect_conflict("k", &policy, "лимит 10", &tool, "лимит 20")
            .expect("conflict detected");
        assert_eq!(conflict.resolution, ConflictResolution::UserConfirm);
    }

    #[test]
    fn same_level_conflicts_fall_back_to_trust_then_recency() {
        let mut left = item("l", ItemKind::Memory, "h-l");
        left.trust = Trust::Confirmed;
        let mut right = item("r", ItemKind::Memory, "h-r");
        right.trust = Trust::External;
        let by_trust = detect_conflict("k", &left, "лимит 10", &right, "лимит 20")
            .expect("conflict detected");
        assert_eq!(by_trust.resolution, ConflictResolution::KeepHigherTrust);

        right.trust = Trust::Confirmed;
        right.created_at = left.created_at + 1;
        let by_recency = detect_conflict("k", &left, "лимит 10", &right, "лимит 20")
            .expect("conflict detected");
        assert_eq!(by_recency.resolution, ConflictResolution::KeepNewer);
    }

    struct StubModel {
        available: bool,
        result: Result<RawSummary, String>,
        calls: u32,
    }

    impl SummaryModel for StubModel {
        fn available(&self) -> bool {
            self.available
        }

        fn summarize(
            &mut self,
            _items: &[ContextItem],
            _config: &SummarizerConfig,
        ) -> Result<RawSummary, String> {
            self.calls += 1;
            self.result.clone()
        }
    }

    fn compressible() -> Vec<ContextItem> {
        vec![
            item("h1", ItemKind::History, "h-1"),
            item("h2", ItemKind::History, "h-2"),
        ]
    }

    #[test]
    fn valid_model_summary_is_accepted() {
        let mut summarizer = BoundedSummarizer::new(
            Some(StubModel {
                available: true,
                result: Ok(RawSummary {
                    summary_id: "s1".to_string(),
                    source_ids: vec!["h1".to_string(), "h2".to_string()],
                    estimated_tokens: 40,
                    text: "краткое изложение".to_string(),
                }),
                calls: 0,
            }),
            SummarizerConfig::default(),
        );
        let outcome = summarizer.summarize(&compressible()).expect("summary");
        assert!(!outcome.fallback);
        assert_eq!(outcome.summary_tokens, 40);
    }

    #[test]
    fn unavailable_model_produces_a_deterministic_fallback() {
        let mut summarizer = BoundedSummarizer::new(
            Some(StubModel {
                available: false,
                result: Err("unused".to_string()),
                calls: 0,
            }),
            SummarizerConfig::default(),
        );
        let outcome = summarizer.summarize(&compressible()).expect("summary");
        assert!(outcome.fallback);
        assert!(outcome.fallback_reason.is_some());
        assert_eq!(outcome.source_ids.len(), 2);
    }

    #[test]
    fn summary_over_budget_is_rejected_and_falls_back() {
        let mut summarizer = BoundedSummarizer::new(
            Some(StubModel {
                available: true,
                result: Ok(RawSummary {
                    summary_id: "s1".to_string(),
                    source_ids: vec!["h1".to_string(), "h2".to_string()],
                    estimated_tokens: 100_000,
                    text: "слишком длинно".to_string(),
                }),
                calls: 0,
            }),
            SummarizerConfig::default(),
        );
        let outcome = summarizer.summarize(&compressible()).expect("summary");
        assert!(outcome.fallback);
        assert!(outcome
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("summary_budget")));
    }

    #[test]
    fn partially_invalid_summary_is_rejected_entirely() {
        let mut summarizer = BoundedSummarizer::new(
            Some(StubModel {
                available: true,
                result: Ok(RawSummary {
                    summary_id: "s1".to_string(),
                    source_ids: vec!["h1".to_string(), "missing".to_string()],
                    estimated_tokens: 40,
                    text: "частично корректно".to_string(),
                }),
                calls: 0,
            }),
            SummarizerConfig::default(),
        );
        let outcome = summarizer.summarize(&compressible()).expect("summary");
        assert!(outcome.fallback);
    }

    #[test]
    fn the_model_is_never_called_twice_in_one_assembly() {
        let mut summarizer = BoundedSummarizer::new(
            Some(StubModel {
                available: true,
                result: Err("boom".to_string()),
                calls: 0,
            }),
            SummarizerConfig::default(),
        );
        let first = summarizer.summarize(&compressible()).expect("summary");
        let second = summarizer.summarize(&compressible()).expect("summary");
        assert!(first.fallback && second.fallback);
        assert_eq!(
            summarizer.model.as_ref().expect("model present").calls,
            1,
            "каскадный повторный запуск summarizer запрещён"
        );
    }

    #[test]
    fn fallback_never_summarizes_mandatory_items_or_incomplete_tool_pairs() {
        let summarizer: BoundedSummarizer<StubModel> =
            BoundedSummarizer::new(None, SummarizerConfig::default());
        let mut items = compressible();
        items.push(item("safety", ItemKind::SafetyPolicy, "h-safety"));
        let mut incomplete = item("incomplete", ItemKind::ToolResult, "h-incomplete");
        incomplete.tool_pair_complete = false;
        items.push(incomplete);
        let outcome = summarizer.fallback_summary(&items, "test");
        assert!(!outcome.source_ids.contains(&"safety".to_string()));
        assert!(!outcome.source_ids.contains(&"incomplete".to_string()));
    }

    #[test]
    fn summarizer_config_forbids_tools_and_retries() {
        let config = SummarizerConfig::default();
        assert!(!config.tools_allowed);
        assert!(!config.retries_allowed);
    }
}
