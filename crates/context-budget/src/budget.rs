//! `ContextBudget`, обязательный минимум и контракт отказа сборки (этап 01.1).

use serde::{Deserialize, Serialize};

use crate::item::{BudgetCategory, ContextItem, ItemKind};
use crate::profile::ModelContextProfile;

/// `schema_version` контракта `ContextBudget`.
pub const CONTEXT_BUDGET_SCHEMA_VERSION: u32 = 1;

/// Уровни бюджета одной категории.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryBudget {
    pub target_tokens: u32,
    pub soft_limit_tokens: u32,
    pub hard_limit_tokens: u32,
}

/// Core-owned бюджет контекста: общие уровни профиля плюс уровни по категориям.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBudget {
    pub schema_version: u32,
    pub profile_version: String,
    pub target_tokens: u32,
    pub soft_limit_tokens: u32,
    pub hard_limit_tokens: u32,
    pub reserves_total: u32,
    pub system: CategoryBudget,
    pub user: CategoryBudget,
    pub memory: CategoryBudget,
    pub tools: CategoryBudget,
    pub history: CategoryBudget,
    pub scratchpad: CategoryBudget,
    pub output: CategoryBudget,
}

/// Детерминированная доля категории в `target_tokens`, в процентах.
/// Сумма долей — 100. Категория `output` покрывается резервом ответа и не
/// расходует контекстную часть бюджета.
fn category_share(category: BudgetCategory) -> u32 {
    match category {
        BudgetCategory::System => 10,
        BudgetCategory::User => 10,
        BudgetCategory::Memory => 20,
        BudgetCategory::Tools => 10,
        BudgetCategory::History => 35,
        BudgetCategory::Scratchpad => 15,
        BudgetCategory::Output => 0,
    }
}

fn share_of(value: u32, percent: u32) -> u32 {
    ((u64::from(value) * u64::from(percent)) / 100) as u32
}

impl ContextBudget {
    /// Бюджет выводится из профиля детерминированно: доли категорий фиксированы,
    /// поэтому одинаковый профиль всегда даёт одинаковый бюджет.
    pub fn from_profile(profile: &ModelContextProfile) -> Self {
        let category = |kind: BudgetCategory| CategoryBudget {
            target_tokens: share_of(profile.target_tokens, category_share(kind)),
            soft_limit_tokens: share_of(profile.soft_limit_tokens, category_share(kind)),
            hard_limit_tokens: share_of(profile.hard_limit_tokens, category_share(kind)),
        };
        Self {
            schema_version: CONTEXT_BUDGET_SCHEMA_VERSION,
            profile_version: profile.profile_version.clone(),
            target_tokens: profile.target_tokens,
            soft_limit_tokens: profile.soft_limit_tokens,
            hard_limit_tokens: profile.hard_limit_tokens,
            reserves_total: profile.reserves_total(),
            system: category(BudgetCategory::System),
            user: category(BudgetCategory::User),
            memory: category(BudgetCategory::Memory),
            tools: category(BudgetCategory::Tools),
            history: category(BudgetCategory::History),
            scratchpad: category(BudgetCategory::Scratchpad),
            output: CategoryBudget {
                target_tokens: profile.final_answer_reserve,
                soft_limit_tokens: profile.final_answer_reserve,
                hard_limit_tokens: profile.final_answer_reserve,
            },
        }
    }

    pub fn category(&self, category: BudgetCategory) -> CategoryBudget {
        match category {
            BudgetCategory::System => self.system,
            BudgetCategory::User => self.user,
            BudgetCategory::Memory => self.memory,
            BudgetCategory::Tools => self.tools,
            BudgetCategory::History => self.history,
            BudgetCategory::Scratchpad => self.scratchpad,
            BudgetCategory::Output => self.output,
        }
    }

    /// Целевое состояние: `context_tokens + reserves_total <= target_tokens + reserves_total`.
    pub fn within_target(&self, context_tokens: u32) -> bool {
        context_tokens <= self.target_tokens
    }

    /// Порог запуска лестницы сокращения.
    pub fn within_soft_limit(&self, context_tokens: u32) -> bool {
        context_tokens.saturating_add(self.reserves_total) <= self.soft_limit_tokens
    }

    /// Жёсткий инвариант: `context_tokens + reserves_total <= hard_limit_tokens`.
    pub fn within_hard_limit(&self, context_tokens: u32) -> bool {
        context_tokens.saturating_add(self.reserves_total) <= self.hard_limit_tokens
    }
}

/// Часть обязательного минимума. Порядок вариантов задаёт фиксированный порядок
/// частей в собранном контексте и детерминированный выбор `missing_part`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MandatoryPart {
    SafetyPolicy,
    ApprovalPolicy,
    UserPrompt,
    PendingToolCall,
    Cancellation,
}

impl MandatoryPart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SafetyPolicy => "safety_policy",
            Self::ApprovalPolicy => "approval_policy",
            Self::UserPrompt => "user_prompt",
            Self::PendingToolCall => "pending_tool_call",
            Self::Cancellation => "cancellation",
        }
    }

    /// Фиксированный порядок частей обязательного минимума.
    pub fn order() -> [Self; 5] {
        [
            Self::SafetyPolicy,
            Self::ApprovalPolicy,
            Self::UserPrompt,
            Self::PendingToolCall,
            Self::Cancellation,
        ]
    }

    /// Часть, к которой относится kind, если item обязателен.
    pub fn of_kind(kind: ItemKind) -> Option<Self> {
        match kind {
            ItemKind::SafetyPolicy => Some(Self::SafetyPolicy),
            ItemKind::ApprovalPolicy => Some(Self::ApprovalPolicy),
            ItemKind::UserPrompt => Some(Self::UserPrompt),
            ItemKind::PendingToolCall => Some(Self::PendingToolCall),
            ItemKind::Cancellation => Some(Self::Cancellation),
            _ => None,
        }
    }

    /// Safety- и approval-часть не сокращается никогда.
    pub fn is_never_reducible(self) -> bool {
        matches!(self, Self::SafetyPolicy | Self::ApprovalPolicy)
    }
}

/// Минимальный обязательный набор, вычисленный детерминированно из состояния Core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MinimumViableContext {
    /// Item в фиксированном порядке частей.
    pub items: Vec<ContextItem>,
    /// Причина включения каждой части: `(часть, число item, токены)`.
    pub parts: Vec<(MandatoryPart, u32, u32)>,
    pub tokens: u32,
}

impl MinimumViableContext {
    /// Собирает обязательный минимум из полного набора item. Всегда включает
    /// safety/system policy и текущий user prompt; approval semantics,
    /// незавершённый tool-call и cancellation context — при наличии таких
    /// состояний. Порядок частей фиксирован.
    pub fn select(items: &[ContextItem]) -> Self {
        let mut selected = Vec::new();
        let mut parts = Vec::new();
        let mut tokens = 0_u32;
        for part in MandatoryPart::order() {
            let mut part_items: Vec<&ContextItem> = items
                .iter()
                .filter(|item| MandatoryPart::of_kind(item.kind) == Some(part))
                .collect();
            if part_items.is_empty() {
                continue;
            }
            // Внутри части порядок детерминированный: created_at, затем id.
            part_items.sort_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            let part_tokens: u32 = part_items
                .iter()
                .map(|item| item.estimated_tokens)
                .fold(0, u32::saturating_add);
            parts.push((part, part_items.len() as u32, part_tokens));
            tokens = tokens.saturating_add(part_tokens);
            selected.extend(part_items.into_iter().cloned());
        }
        Self {
            items: selected,
            parts,
            tokens,
        }
    }

    /// Первая по фиксированному порядку часть, на которой накопленная сумма
    /// превысила лимит. Порядок не даёт права сокращать младшие части, он лишь
    /// делает выбор `missing_part` детерминированным.
    pub fn missing_part(&self, limit: u32) -> Option<MandatoryPart> {
        let mut running = 0_u32;
        for (part, _, part_tokens) in &self.parts {
            running = running.saturating_add(*part_tokens);
            if running > limit {
                return Some(*part);
            }
        }
        self.parts.last().map(|(part, _, _)| *part)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.items.iter().any(|item| item.id == id)
    }
}

/// Стадия отказа сборки контекста.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetUnavailableStage {
    MandatoryOverflow,
    DropsExhausted,
    ProviderReplanFailed,
    EstimatorUnavailable,
}

impl BudgetUnavailableStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MandatoryOverflow => "mandatory_overflow",
            Self::DropsExhausted => "drops_exhausted",
            Self::ProviderReplanFailed => "provider_replan_failed",
            Self::EstimatorUnavailable => "estimator_unavailable",
        }
    }
}

/// Код отказа. Терминальный результат сборки, а не исключение внутри неё.
pub const BUDGET_UNAVAILABLE_CODE: &str = "budget_unavailable";

/// Терминальный результат сборки контекста. Model call при этом не выполняется,
/// автоматический retry запрещён на всех уровнях.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetUnavailable {
    pub code: String,
    pub stage: BudgetUnavailableStage,
    pub required_tokens: u32,
    pub available_tokens: u32,
    pub profile_version: String,
    pub tokenizer_version: String,
    /// Hash частичной сборки.
    pub context_ledger_hash: String,
    /// Какая именно категория обязательного набора не поместилась.
    pub missing_part: Option<MandatoryPart>,
}

impl BudgetUnavailable {
    pub fn new(
        stage: BudgetUnavailableStage,
        required_tokens: u32,
        available_tokens: u32,
        profile_version: impl Into<String>,
        tokenizer_version: impl Into<String>,
    ) -> Self {
        Self {
            code: BUDGET_UNAVAILABLE_CODE.to_string(),
            stage,
            required_tokens,
            available_tokens,
            profile_version: profile_version.into(),
            tokenizer_version: tokenizer_version.into(),
            context_ledger_hash: String::new(),
            missing_part: None,
        }
    }

    pub fn with_missing_part(mut self, part: Option<MandatoryPart>) -> Self {
        self.missing_part = part;
        self
    }

    pub fn with_ledger_hash(mut self, hash: impl Into<String>) -> Self {
        self.context_ledger_hash = hash.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{ContextItemBuilder, ItemKind};

    fn mandatory_item(id: &str, kind: ItemKind, tokens: u32) -> ContextItem {
        ContextItemBuilder::new(id, kind, format!("hash-{id}"))
            .sizes(u64::from(tokens) * 3, tokens)
            .build()
    }

    #[test]
    fn category_shares_sum_to_the_whole_context_budget() {
        let total: u32 = BudgetCategory::all().into_iter().map(category_share).sum();
        assert_eq!(total, 100);
    }

    #[test]
    fn budget_levels_follow_the_profile() {
        let profile = ModelContextProfile::fallback("p", "m", 128_000);
        let budget = ContextBudget::from_profile(&profile);
        assert_eq!(budget.target_tokens, profile.target_tokens);
        assert_eq!(budget.reserves_total, profile.reserves_total());
        assert_eq!(budget.output.target_tokens, profile.final_answer_reserve);
        assert_eq!(
            budget.history.target_tokens,
            share_of(profile.target_tokens, 35)
        );
    }

    #[test]
    fn hard_limit_check_accounts_for_reserves() {
        let profile = ModelContextProfile::fallback("p", "m", 128_000);
        let budget = ContextBudget::from_profile(&profile);
        let ceiling = profile.hard_limit_tokens - profile.reserves_total();
        assert!(budget.within_hard_limit(ceiling));
        assert!(!budget.within_hard_limit(ceiling + 1));
    }

    #[test]
    fn minimum_viable_context_keeps_the_fixed_part_order() {
        let items = vec![
            mandatory_item("cancel", ItemKind::Cancellation, 5),
            mandatory_item("prompt", ItemKind::UserPrompt, 20),
            mandatory_item("safety", ItemKind::SafetyPolicy, 100),
            mandatory_item("approval", ItemKind::ApprovalPolicy, 30),
        ];
        let mvc = MinimumViableContext::select(&items);
        let order: Vec<&str> = mvc.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(order, vec!["safety", "approval", "prompt", "cancel"]);
        assert_eq!(mvc.tokens, 155);
    }

    #[test]
    fn optional_kinds_never_enter_the_minimum_viable_context() {
        let items = vec![
            mandatory_item("safety", ItemKind::SafetyPolicy, 10),
            mandatory_item("history", ItemKind::History, 500),
            mandatory_item("memory", ItemKind::Memory, 500),
        ];
        let mvc = MinimumViableContext::select(&items);
        assert_eq!(mvc.tokens, 10);
        assert!(!mvc.contains("history"));
    }

    #[test]
    fn missing_part_is_the_first_part_that_overflows() {
        let items = vec![
            mandatory_item("safety", ItemKind::SafetyPolicy, 100),
            mandatory_item("approval", ItemKind::ApprovalPolicy, 100),
            mandatory_item("prompt", ItemKind::UserPrompt, 100),
        ];
        let mvc = MinimumViableContext::select(&items);
        assert_eq!(mvc.missing_part(50), Some(MandatoryPart::SafetyPolicy));
        assert_eq!(mvc.missing_part(150), Some(MandatoryPart::ApprovalPolicy));
        assert_eq!(mvc.missing_part(250), Some(MandatoryPart::UserPrompt));
    }

    #[test]
    fn safety_and_approval_are_never_reducible() {
        assert!(MandatoryPart::SafetyPolicy.is_never_reducible());
        assert!(MandatoryPart::ApprovalPolicy.is_never_reducible());
        assert!(!MandatoryPart::PendingToolCall.is_never_reducible());
    }
}
