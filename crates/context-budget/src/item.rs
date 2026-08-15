//! `ContextItem`, его атрибуты и справочник `drop_reason` (этап 01.1).

use serde::{Deserialize, Serialize};

/// `schema_version` контракта `ContextItem`. Версионируется независимо от
/// `ContextBudget`, `ModelContextProfile` и `context_ledger`.
pub const CONTEXT_ITEM_SCHEMA_VERSION: u32 = 1;

/// Категория элемента контекста. Категория одновременно определяет бюджетную
/// категорию и участвует в hash input `content_hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Safety/system policy — часть обязательного минимума.
    SafetyPolicy,
    /// Approval/permission semantics — часть обязательного минимума.
    ApprovalPolicy,
    /// Системные инструкции агента.
    SystemInstruction,
    /// Текущий пользовательский prompt.
    UserPrompt,
    /// Явное ограничение пользователя.
    UserConstraint,
    /// Состояние незавершённого tool-call.
    PendingToolCall,
    /// Контекст отмены.
    Cancellation,
    /// Запись долговременной памяти.
    Memory,
    /// Подтверждённое решение или факт задачи.
    Decision,
    /// Запись scratchpad.
    Scratchpad,
    /// Реплика истории диалога.
    History,
    /// Результат инструмента.
    ToolResult,
    /// Схема инструмента (loadout, 01.4).
    ToolSchema,
    /// Evidence block из плана 02.
    Evidence,
    /// Сжатая проекция нескольких item (01.3).
    Summary,
}

impl ItemKind {
    /// Строковое имя, используемое в hash input и в ledger.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SafetyPolicy => "safety_policy",
            Self::ApprovalPolicy => "approval_policy",
            Self::SystemInstruction => "system_instruction",
            Self::UserPrompt => "user_prompt",
            Self::UserConstraint => "user_constraint",
            Self::PendingToolCall => "pending_tool_call",
            Self::Cancellation => "cancellation",
            Self::Memory => "memory",
            Self::Decision => "decision",
            Self::Scratchpad => "scratchpad",
            Self::History => "history",
            Self::ToolResult => "tool_result",
            Self::ToolSchema => "tool_schema",
            Self::Evidence => "evidence",
            Self::Summary => "summary",
        }
    }

    /// Бюджетная категория элемента.
    pub fn category(self) -> BudgetCategory {
        match self {
            Self::SafetyPolicy
            | Self::ApprovalPolicy
            | Self::SystemInstruction
            | Self::Cancellation => BudgetCategory::System,
            Self::UserPrompt | Self::UserConstraint => BudgetCategory::User,
            Self::Memory | Self::Evidence => BudgetCategory::Memory,
            Self::ToolSchema => BudgetCategory::Tools,
            Self::History | Self::ToolResult | Self::PendingToolCall | Self::Summary => {
                BudgetCategory::History
            }
            Self::Scratchpad | Self::Decision => BudgetCategory::Scratchpad,
        }
    }
}

/// Категория бюджета: `ContextBudget` объявляет уровни для каждой из них.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetCategory {
    System,
    User,
    Memory,
    Tools,
    History,
    Scratchpad,
    Output,
}

impl BudgetCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Memory => "memory",
            Self::Tools => "tools",
            Self::History => "history",
            Self::Scratchpad => "scratchpad",
            Self::Output => "output",
        }
    }

    /// Все категории в детерминированном порядке — для гистограмм утилизации.
    pub fn all() -> [Self; 7] {
        [
            Self::System,
            Self::User,
            Self::Memory,
            Self::Tools,
            Self::History,
            Self::Scratchpad,
            Self::Output,
        ]
    }
}

/// Уровень доверия к содержимому. Работает только как тай-брейк внутри одного
/// уровня иерархии прав (01.3), а не как самостоятельное основание отбора.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    /// Непроверенное содержимое: `recovered` записи scratchpad, сырые данные.
    Unverified,
    /// Внешние данные, прошедшие envelope-проверку.
    External,
    /// Подтверждённая Core запись.
    Confirmed,
    /// Содержимое, порождённое самим Core (policy, system prompt).
    CoreOwned,
}

impl Trust {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::External => "external",
            Self::Confirmed => "confirmed",
            Self::CoreOwned => "core_owned",
        }
    }
}

/// Privacy label. Ограничивает offload и передачу содержимого наружу.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    /// Обычные рабочие данные: offload разрешён.
    Workspace,
    /// Чувствительные данные: offload на диск запрещён.
    Sensitive,
    /// Секреты: не попадают ни в offload, ни в diagnostics.
    Secret,
}

impl Privacy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workspace => "workspace",
            Self::Sensitive => "sensitive",
            Self::Secret => "secret",
        }
    }

    /// Допускает ли label выгрузку содержимого в artifact store.
    pub fn allows_offload(self) -> bool {
        matches!(self, Self::Workspace)
    }
}

/// Статус записи scratchpad (01.2). Не является `drop_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScratchpadStatus {
    Draft,
    Confirmed,
    Recovered,
}

impl ScratchpadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Confirmed => "confirmed",
            Self::Recovered => "recovered",
        }
    }
}

/// Справочник причин отбрасывания. Потребители обязаны трактовать неизвестное
/// значение как [`DropReason::Unknown`] без ошибки: расширение справочника —
/// minor-изменение контракта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    OverBudget,
    LowPriority,
    Duplicate,
    Superseded,
    Expired,
    Unverified,
    Offloaded,
    StaleToolOutput,
    PrivacyRestricted,
    InvalidToolState,
    PolicyDenied,
    /// Значение, не входящее в известный справочник этой версии.
    Unknown,
}

impl DropReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OverBudget => "over_budget",
            Self::LowPriority => "low_priority",
            Self::Duplicate => "duplicate",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
            Self::Unverified => "unverified",
            Self::Offloaded => "offloaded",
            Self::StaleToolOutput => "stale_tool_output",
            Self::PrivacyRestricted => "privacy_restricted",
            Self::InvalidToolState => "invalid_tool_state",
            Self::PolicyDenied => "policy_denied",
            Self::Unknown => "unknown",
        }
    }

    /// Разбор значения из ledger. Неизвестная строка не является ошибкой.
    pub fn parse(value: &str) -> Self {
        match value {
            "over_budget" => Self::OverBudget,
            "low_priority" => Self::LowPriority,
            "duplicate" => Self::Duplicate,
            "superseded" => Self::Superseded,
            "expired" => Self::Expired,
            "unverified" => Self::Unverified,
            "offloaded" => Self::Offloaded,
            "stale_tool_output" => Self::StaleToolOutput,
            "privacy_restricted" => Self::PrivacyRestricted,
            "invalid_tool_state" => Self::InvalidToolState,
            "policy_denied" => Self::PolicyDenied,
            _ => Self::Unknown,
        }
    }
}

/// Элемент контекста. Полный набор атрибутов из 01.1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextItem {
    pub id: String,
    pub task_id: String,
    pub session_id: String,
    pub parent_id: Option<String>,
    pub kind: ItemKind,
    /// Происхождение item (имя инструмента, `memory`, `user` и т.п.).
    pub source: String,
    /// Базовый приоритет 0..100, больше значит важнее.
    pub priority: u8,
    pub trust: Trust,
    pub privacy: Privacy,
    /// unix ms.
    pub created_at: i64,
    /// unix ms.
    pub last_used_at: i64,
    /// TTL в миллисекундах от `created_at`; `None` — бессрочно.
    pub ttl_ms: Option<i64>,
    /// Retention в миллисекундах от `created_at`; `None` — бессрочно.
    pub retention_ms: Option<i64>,
    pub pinned: bool,
    /// Ревизия item в пределах одного `parent_id`/ключа.
    pub version: u32,
    pub tokenizer_version: String,
    pub content_hash: String,
    pub bytes: u64,
    pub estimated_tokens: u32,
    pub selected: bool,
    pub drop_reason: Option<DropReason>,
    /// Статус scratchpad, если item пришёл из scratchpad (01.2).
    #[serde(default)]
    pub scratchpad_status: Option<ScratchpadStatus>,
    /// Ключ конфликта (01.3): `entity_id`+атрибут, `tool_call_id`+поле,
    /// `decision_key`. Пусто для item без определённого ключа.
    #[serde(default)]
    pub conflict_key: Option<String>,
    /// Locator артефакта, если содержимое выгружено в artifact store (01.2).
    #[serde(default)]
    pub artifact_locator: Option<String>,
    /// Завершена ли пара tool-call/result. Незавершённые пары не отбрасываются
    /// уровнем L3 лестницы.
    #[serde(default = "default_true")]
    pub tool_pair_complete: bool,
}

fn default_true() -> bool {
    true
}

/// Порог, ниже которого item считается низкоприоритетным по умолчанию.
pub const DEFAULT_LOW_PRIORITY_CUTOFF: u8 = 30;

impl ContextItem {
    /// `effective_priority` вычисляется детерминированно: базовый `priority`;
    /// `pinned=true` даёт `max(priority, 90)`; scratchpad-статус `recovered`
    /// даёт `min(priority, 20)`. Правила применяются именно в этом порядке,
    /// поэтому pinned recovered-запись получает 90 и всё равно остаётся
    /// необязательной.
    pub fn effective_priority(&self) -> u8 {
        let mut priority = self.priority;
        if self.pinned {
            priority = priority.max(90);
        }
        if self.scratchpad_status == Some(ScratchpadStatus::Recovered) {
            priority = priority.min(20);
        }
        priority
    }

    /// Истёк ли TTL на момент `now` (unix ms).
    pub fn ttl_expired(&self, now: i64) -> bool {
        self.ttl_ms
            .is_some_and(|ttl| now > self.created_at.saturating_add(ttl))
    }

    /// Истёк ли retention на момент `now` (unix ms).
    pub fn retention_expired(&self, now: i64) -> bool {
        self.retention_ms
            .is_some_and(|retention| now > self.created_at.saturating_add(retention))
    }

    /// Входит ли kind в обязательный минимум по определению.
    pub fn is_mandatory_kind(&self) -> bool {
        matches!(
            self.kind,
            ItemKind::SafetyPolicy
                | ItemKind::ApprovalPolicy
                | ItemKind::UserPrompt
                | ItemKind::PendingToolCall
                | ItemKind::Cancellation
        )
    }

    /// Детерминированный ключ порядка отбрасывания внутри уровня лестницы:
    /// pinned последним, затем по возрастанию `effective_priority`, `created_at`,
    /// `content_hash` и `id`.
    pub fn drop_order_key(&self) -> (bool, u8, i64, &str, &str) {
        (
            self.pinned,
            self.effective_priority(),
            self.created_at,
            self.content_hash.as_str(),
            self.id.as_str(),
        )
    }
}

/// Конструктор с разумными значениями по умолчанию для тестов и вызывающего кода.
#[derive(Debug, Clone)]
pub struct ContextItemBuilder {
    item: ContextItem,
}

impl ContextItemBuilder {
    pub fn new(id: impl Into<String>, kind: ItemKind, content_hash: impl Into<String>) -> Self {
        Self {
            item: ContextItem {
                id: id.into(),
                task_id: String::new(),
                session_id: String::new(),
                parent_id: None,
                kind,
                source: String::new(),
                priority: 50,
                trust: Trust::External,
                privacy: Privacy::Workspace,
                created_at: 0,
                last_used_at: 0,
                ttl_ms: None,
                retention_ms: None,
                pinned: false,
                version: 1,
                tokenizer_version: String::new(),
                content_hash: content_hash.into(),
                bytes: 0,
                estimated_tokens: 0,
                selected: false,
                drop_reason: None,
                scratchpad_status: None,
                conflict_key: None,
                artifact_locator: None,
                tool_pair_complete: true,
            },
        }
    }

    pub fn task(mut self, task_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        self.item.task_id = task_id.into();
        self.item.session_id = session_id.into();
        self
    }

    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.item.source = source.into();
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.item.priority = priority.min(100);
        self
    }

    pub fn trust(mut self, trust: Trust) -> Self {
        self.item.trust = trust;
        self
    }

    pub fn privacy(mut self, privacy: Privacy) -> Self {
        self.item.privacy = privacy;
        self
    }

    pub fn created_at(mut self, created_at: i64) -> Self {
        self.item.created_at = created_at;
        self.item.last_used_at = created_at;
        self
    }

    pub fn ttl_ms(mut self, ttl_ms: i64) -> Self {
        self.item.ttl_ms = Some(ttl_ms);
        self
    }

    pub fn retention_ms(mut self, retention_ms: i64) -> Self {
        self.item.retention_ms = Some(retention_ms);
        self
    }

    pub fn pinned(mut self, pinned: bool) -> Self {
        self.item.pinned = pinned;
        self
    }

    pub fn version(mut self, version: u32) -> Self {
        self.item.version = version;
        self
    }

    pub fn parent(mut self, parent_id: impl Into<String>) -> Self {
        self.item.parent_id = Some(parent_id.into());
        self
    }

    pub fn scratchpad_status(mut self, status: ScratchpadStatus) -> Self {
        self.item.scratchpad_status = Some(status);
        self
    }

    pub fn conflict_key(mut self, key: impl Into<String>) -> Self {
        self.item.conflict_key = Some(key.into());
        self
    }

    pub fn tool_pair_complete(mut self, complete: bool) -> Self {
        self.item.tool_pair_complete = complete;
        self
    }

    pub fn sizes(mut self, bytes: u64, estimated_tokens: u32) -> Self {
        self.item.bytes = bytes;
        self.item.estimated_tokens = estimated_tokens;
        self
    }

    pub fn build(self) -> ContextItem {
        self.item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item() -> ContextItemBuilder {
        ContextItemBuilder::new("i1", ItemKind::History, "hash")
    }

    #[test]
    fn pin_raises_priority_to_at_least_ninety() {
        let pinned = item().priority(10).pinned(true).build();
        assert_eq!(pinned.effective_priority(), 90);
    }

    #[test]
    fn pin_does_not_lower_an_already_higher_priority() {
        let pinned = item().priority(95).pinned(true).build();
        assert_eq!(pinned.effective_priority(), 95);
    }

    #[test]
    fn recovered_status_caps_priority_after_pin() {
        let recovered = item()
            .priority(80)
            .pinned(true)
            .scratchpad_status(ScratchpadStatus::Recovered)
            .build();
        // Порядок правил: pin поднимает до 90, затем recovered опускает до 20.
        assert_eq!(recovered.effective_priority(), 20);
    }

    #[test]
    fn ttl_expiry_uses_strict_comparison() {
        let entry = item().created_at(1_000).ttl_ms(100).build();
        assert!(!entry.ttl_expired(1_100));
        assert!(entry.ttl_expired(1_101));
    }

    #[test]
    fn unknown_drop_reason_parses_without_error() {
        assert_eq!(DropReason::parse("some_future_reason"), DropReason::Unknown);
        assert_eq!(DropReason::parse("duplicate"), DropReason::Duplicate);
    }

    #[test]
    fn secret_and_sensitive_items_are_not_offloadable() {
        assert!(Privacy::Workspace.allows_offload());
        assert!(!Privacy::Sensitive.allows_offload());
        assert!(!Privacy::Secret.allows_offload());
    }
}
