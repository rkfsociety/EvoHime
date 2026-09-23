//! `context_ledger`, его hash и запись фактического usage (этап 01.1).

use serde::{Deserialize, Serialize};

use crate::budget::{BudgetUnavailable, MandatoryPart};
use crate::hash::sha256_hex;
use crate::item::DropReason;
use crate::ladder::LadderLevel;

/// `schema_version` контракта `context_ledger`.
pub const CONTEXT_LEDGER_SCHEMA_VERSION: u32 = 1;

/// Один выбранный item в собранном контексте.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedItemRecord {
    /// Identifier of the selected context item.
    pub id: String,
    /// Token estimate included in the request ledger.
    pub estimated_tokens: u32,
}

/// Один отброшенный item и причина.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedItemRecord {
    /// Identifier of the item excluded from the context.
    pub id: String,
    /// Stable reason for excluding or replacing the item.
    pub drop_reason: DropReason,
}

/// Применённое compression-решение.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionRecord {
    /// Identifier of the generated summary item.
    pub summary_id: String,
    /// Identifiers represented by the generated summary.
    pub source_ids: Vec<String>,
    /// Отношение размера summary к размеру исходных item.
    pub compression_ratio: f64,
    /// Version of the compression strategy used for this summary.
    pub summarizer_version: String,
    #[serde(default)]
    /// Token ceiling applied to the summary output.
    pub summary_budget: u32,
    #[serde(default)]
    /// Whether deterministic fallback replaced model summarization.
    pub fallback: bool,
    #[serde(default)]
    /// Bounded reason for using the deterministic fallback, if applicable.
    pub fallback_reason: Option<String>,
}

/// Итог работы обязательного минимума: часть, число item и токены.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MandatoryPartRecord {
    /// Required context part summarized by this record.
    pub part: MandatoryPart,
    /// Number of items included in this part.
    pub items: u32,
    /// Estimated tokens contributed by this part.
    pub tokens: u32,
}

/// Итог tool loadout (этап 01.4).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LoadoutRecord {
    /// Stable identifier of the selected tool loadout.
    pub loadout_id: String,
    /// Intent classification used to select the loadout.
    pub intent: String,
    /// Version of the intent and loadout rules.
    pub rules_version: String,
    /// Rule that selected this loadout, when one matched.
    pub matched_rule: Option<String>,
    /// Tool identifiers included in the request.
    pub tool_ids: Vec<String>,
    /// Estimated tokens consumed by tool schemas.
    pub schema_tokens: u32,
    #[serde(default)]
    /// Whether a safe fallback loadout was selected.
    pub fallback: bool,
}

/// Результат сборки: отправлено или отказано.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerOutcome {
    /// Context was sent to the model provider.
    Sent,
    /// Context assembly terminated without a model call.
    BudgetUnavailable,
}

impl LedgerOutcome {
    /// Returns the stable serialized outcome name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sent => "sent",
            Self::BudgetUnavailable => "budget_unavailable",
        }
    }
}

/// Запись `context_ledger`: одна запись на один model call. Записи immutable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextLedgerEntry {
    /// Stable identifier for this ledger entry.
    pub id: String,
    /// Schema version used to serialize the entry.
    pub schema_version: u32,
    /// Task associated with the model call.
    pub task_id: String,
    /// Session associated with the model call.
    pub session_id: String,
    /// Provider request identifier for this call.
    pub model_call_id: String,
    /// unix ms.
    pub created_at: i64,
    /// Provider selected for the model call.
    pub provider: String,
    /// Model selected for the call.
    pub model: String,
    /// Version of the context profile applied to this call.
    pub profile_version: String,
    /// JSON-снимок профиля.
    pub profile_snapshot: String,
    /// Token estimator version used to estimate request size.
    pub tokenizer_version: String,
    /// Content normalization version used for item hashes.
    pub normalizer_version: String,
    /// Context planning strategy version.
    pub strategy_version: String,
    /// Tokens consumed by required context items.
    pub mandatory_tokens: u32,
    /// Tokens consumed by selected optional items.
    pub selected_optional_tokens: u32,
    /// Tokens reserved for system or output requirements.
    pub reserves_tokens: u32,
    /// Estimated total prompt tokens sent to the provider.
    pub estimated_prompt_tokens: u32,
    /// Selected context items in their exact request order.
    pub selected_items: Vec<SelectedItemRecord>,
    /// Items excluded or replaced during planning.
    pub dropped_items: Vec<DroppedItemRecord>,
    /// Per-part counts and token totals for mandatory context.
    pub mandatory_parts: Vec<MandatoryPartRecord>,
    /// Reduction-ladder levels applied to fit the context.
    pub ladder_levels_applied: Vec<LadderLevel>,
    /// Summary and source relationships created during compression.
    pub compression: Vec<CompressionRecord>,
    /// Selected tool loadout and its token cost, if tool selection applied.
    pub loadout: Option<LoadoutRecord>,
    /// Whether the fallback estimator supplied the token estimate.
    pub fallback_estimator: bool,
    /// Идентификатор записи, ре-план которой выполняется.
    pub replan_of: Option<String>,
    /// Outcome of sending the measured prompt.
    pub outcome: LedgerOutcome,
    /// Terminal budget failure detail when no provider call was made.
    pub budget_unavailable: Option<BudgetUnavailable>,
    /// Digest of the finalized context planning decisions.
    pub context_ledger_hash: String,
}

impl ContextLedgerEntry {
    /// `context_ledger_hash` покрывает ids выбранных item и их порядок, версии
    /// profile/tokenizer/нормализатора/стратегии, обязательный набор, список
    /// отброшенных item с причинами, применённые compression/pruning-решения,
    /// fallback-флаг и loadout. Одинаковый hash обязан означать одинаковый
    /// фактический вход модели.
    ///
    /// Hash считается один раз, когда состав и порядок контекста уже
    /// зафиксированы, и не является входом ни для selection, ни для лестницы.
    pub fn compute_hash(&self) -> String {
        let mut input = String::new();
        input.push_str("v1\n");
        push_field(&mut input, "profile", &self.profile_version);
        push_field(&mut input, "tokenizer", &self.tokenizer_version);
        push_field(&mut input, "normalizer", &self.normalizer_version);
        push_field(&mut input, "strategy", &self.strategy_version);
        push_field(
            &mut input,
            "fallback_estimator",
            if self.fallback_estimator { "1" } else { "0" },
        );
        push_field(&mut input, "outcome", self.outcome.as_str());

        input.push_str("mandatory\n");
        for part in &self.mandatory_parts {
            input.push_str(&format!(
                "  {}:{}:{}\n",
                part.part.as_str(),
                part.items,
                part.tokens
            ));
        }

        input.push_str("selected\n");
        for (index, item) in self.selected_items.iter().enumerate() {
            input.push_str(&format!(
                "  {index}:{}:{}\n",
                item.id, item.estimated_tokens
            ));
        }

        input.push_str("dropped\n");
        // Порядок отброшенных item нормализуется, чтобы hash зависел от состава
        // и причин, а не от порядка обхода уровней.
        let mut dropped: Vec<&DroppedItemRecord> = self.dropped_items.iter().collect();
        dropped.sort_by(|left, right| {
            left.id
                .cmp(&right.id)
                .then_with(|| left.drop_reason.as_str().cmp(right.drop_reason.as_str()))
        });
        for item in dropped {
            input.push_str(&format!("  {}:{}\n", item.id, item.drop_reason.as_str()));
        }

        input.push_str("ladder\n");
        for level in &self.ladder_levels_applied {
            input.push_str(&format!("  {}\n", level.as_str()));
        }

        input.push_str("compression\n");
        for record in &self.compression {
            input.push_str(&format!(
                "  {}:{}:{}:{}\n",
                record.summary_id,
                record.source_ids.join(","),
                record.summarizer_version,
                if record.fallback { "fallback" } else { "model" }
            ));
        }

        input.push_str("loadout\n");
        if let Some(loadout) = &self.loadout {
            input.push_str(&format!(
                "  {}:{}:{}:{}\n",
                loadout.loadout_id,
                loadout.intent,
                loadout.rules_version,
                loadout.tool_ids.join(",")
            ));
        }

        input.push_str("budget\n");
        input.push_str(&format!(
            "  {}:{}:{}\n",
            self.mandatory_tokens, self.selected_optional_tokens, self.reserves_tokens
        ));

        if let Some(unavailable) = &self.budget_unavailable {
            input.push_str(&format!(
                "unavailable {}:{}\n",
                unavailable.stage.as_str(),
                unavailable
                    .missing_part
                    .map(MandatoryPart::as_str)
                    .unwrap_or("none")
            ));
        }

        sha256_hex(&input)
    }

    /// Пересчитывает hash и записывает его в запись.
    pub fn finalize_hash(&mut self) {
        self.context_ledger_hash = self.compute_hash();
    }
}

fn push_field(input: &mut String, key: &str, value: &str) {
    input.push_str(key);
    input.push('=');
    input.push_str(value);
    input.push('\n');
}

/// Фактический usage провайдера. Пишется в отдельную append-only таблицу,
/// поэтому запись ledger остаётся immutable и hash-стабильной.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextLedgerUsage {
    /// Identifier of the immutable ledger entry this usage describes.
    pub ledger_id: String,
    /// Provider-reported prompt token count.
    pub actual_prompt_tokens: u32,
    /// Provider-reported completion token count.
    pub actual_completion_tokens: u32,
    /// Относительная погрешность оценки.
    pub estimator_drift: f64,
    /// unix ms.
    pub recorded_at: i64,
}

/// Политика ротации: запись хранится, пока выполняется хотя бы одно условие —
/// возраст менее 30 дней или запись относится к одной из последних 200 сессий.
/// Minimum number of days for which ledger records are retained.
pub const LEDGER_RETENTION_DAYS: i64 = 30;
/// Number of most recent sessions whose records are retained regardless of age.
pub const LEDGER_RETAINED_SESSIONS: usize = 200;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::BudgetUnavailableStage;

    fn entry() -> ContextLedgerEntry {
        ContextLedgerEntry {
            id: "ledger-1".to_string(),
            schema_version: CONTEXT_LEDGER_SCHEMA_VERSION,
            task_id: "task".to_string(),
            session_id: "session".to_string(),
            model_call_id: "call".to_string(),
            created_at: 1_700_000_000_000,
            provider: "literouter".to_string(),
            model: "gpt-4o-mini".to_string(),
            profile_version: "profile-1".to_string(),
            profile_snapshot: "{}".to_string(),
            tokenizer_version: "heuristic-1".to_string(),
            normalizer_version: crate::hash::NORMALIZER_VERSION.to_string(),
            strategy_version: crate::profile::STRATEGY_VERSION.to_string(),
            mandatory_tokens: 100,
            selected_optional_tokens: 200,
            reserves_tokens: 300,
            estimated_prompt_tokens: 300,
            selected_items: vec![
                SelectedItemRecord {
                    id: "a".to_string(),
                    estimated_tokens: 100,
                },
                SelectedItemRecord {
                    id: "b".to_string(),
                    estimated_tokens: 200,
                },
            ],
            dropped_items: vec![DroppedItemRecord {
                id: "c".to_string(),
                drop_reason: DropReason::LowPriority,
            }],
            mandatory_parts: vec![MandatoryPartRecord {
                part: MandatoryPart::SafetyPolicy,
                items: 1,
                tokens: 100,
            }],
            ladder_levels_applied: vec![LadderLevel::LowPriorityOptional],
            compression: Vec::new(),
            loadout: None,
            fallback_estimator: false,
            replan_of: None,
            outcome: LedgerOutcome::Sent,
            budget_unavailable: None,
            context_ledger_hash: String::new(),
        }
    }

    #[test]
    fn identical_input_produces_the_same_hash() {
        assert_eq!(entry().compute_hash(), entry().compute_hash());
    }

    #[test]
    fn item_order_changes_the_hash() {
        let mut reordered = entry();
        reordered.selected_items.reverse();
        assert_ne!(entry().compute_hash(), reordered.compute_hash());
    }

    #[test]
    fn profile_and_tokenizer_versions_change_the_hash() {
        let mut other_profile = entry();
        other_profile.profile_version = "profile-2".to_string();
        assert_ne!(entry().compute_hash(), other_profile.compute_hash());

        let mut other_tokenizer = entry();
        other_tokenizer.tokenizer_version = "heuristic-2".to_string();
        assert_ne!(entry().compute_hash(), other_tokenizer.compute_hash());

        let mut other_normalizer = entry();
        other_normalizer.normalizer_version = "norm-2".to_string();
        assert_ne!(entry().compute_hash(), other_normalizer.compute_hash());
    }

    #[test]
    fn compression_decisions_change_the_hash() {
        let mut compressed = entry();
        compressed.compression.push(CompressionRecord {
            summary_id: "s1".to_string(),
            source_ids: vec!["a".to_string(), "b".to_string()],
            compression_ratio: 0.1,
            summarizer_version: "sum-1".to_string(),
            summary_budget: 512,
            fallback: false,
            fallback_reason: None,
        });
        assert_ne!(entry().compute_hash(), compressed.compute_hash());
    }

    #[test]
    fn drop_reason_changes_the_hash_but_drop_order_does_not() {
        let mut reason_changed = entry();
        reason_changed.dropped_items[0].drop_reason = DropReason::Duplicate;
        assert_ne!(entry().compute_hash(), reason_changed.compute_hash());

        let mut two_drops = entry();
        two_drops.dropped_items = vec![
            DroppedItemRecord {
                id: "c".to_string(),
                drop_reason: DropReason::LowPriority,
            },
            DroppedItemRecord {
                id: "d".to_string(),
                drop_reason: DropReason::Expired,
            },
        ];
        let mut swapped = two_drops.clone();
        swapped.dropped_items.reverse();
        assert_eq!(two_drops.compute_hash(), swapped.compute_hash());
    }

    #[test]
    fn fallback_estimator_flag_changes_the_hash() {
        let mut fallback = entry();
        fallback.fallback_estimator = true;
        assert_ne!(entry().compute_hash(), fallback.compute_hash());
    }

    #[test]
    fn loadout_changes_the_hash() {
        let mut with_loadout = entry();
        with_loadout.loadout = Some(LoadoutRecord {
            loadout_id: "read-only".to_string(),
            intent: "inspect".to_string(),
            rules_version: "rules-1".to_string(),
            matched_rule: Some("inspect-keywords".to_string()),
            tool_ids: vec!["fs.read".to_string()],
            schema_tokens: 100,
            fallback: false,
        });
        assert_ne!(entry().compute_hash(), with_loadout.compute_hash());
    }

    #[test]
    fn budget_unavailable_entries_hash_their_stage_and_missing_part() {
        let mut refused = entry();
        refused.outcome = LedgerOutcome::BudgetUnavailable;
        refused.budget_unavailable = Some(
            BudgetUnavailable::new(
                BudgetUnavailableStage::MandatoryOverflow,
                1_000,
                500,
                "profile-1",
                "heuristic-1",
            )
            .with_missing_part(Some(MandatoryPart::UserPrompt)),
        );
        let first = refused.compute_hash();
        refused.budget_unavailable = Some(
            BudgetUnavailable::new(
                BudgetUnavailableStage::MandatoryOverflow,
                1_000,
                500,
                "profile-1",
                "heuristic-1",
            )
            .with_missing_part(Some(MandatoryPart::SafetyPolicy)),
        );
        assert_ne!(first, refused.compute_hash());
    }

    #[test]
    fn finalize_hash_stores_the_computed_value() {
        let mut record = entry();
        record.finalize_hash();
        assert_eq!(record.context_ledger_hash.len(), 64);
        assert_eq!(record.context_ledger_hash, record.compute_hash());
    }
}
