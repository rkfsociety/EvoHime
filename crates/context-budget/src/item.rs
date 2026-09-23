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
    /// Agent system policy and instructions.
    System,
    /// Current user request and constraints.
    User,
    /// Retrieved memory and evidence.
    Memory,
    /// Tool schemas and execution context.
    Tools,
    /// Conversation history and tool outputs.
    History,
    /// Task decisions and scratchpad state.
    Scratchpad,
    /// Model response tokens, which use a separate output reserve.
    Output,
}

impl BudgetCategory {
    /// Returns the stable category name used by metrics and ledgers.
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
    /// Returns the stable serialized trust label.
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
    /// Returns the stable serialized privacy label.
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
    /// Editable note that has not been confirmed by the user.
    Draft,
    /// User-confirmed note eligible for normal scratchpad use.
    Confirmed,
    /// Note recovered after restart and not yet re-confirmed.
    Recovered,
}

impl ScratchpadStatus {
    /// Returns the stable serialized status label.
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
    /// Item was excluded to satisfy a category or total token ceiling.
    OverBudget,
    /// Item fell below the active low-priority threshold.
    LowPriority,
    /// Equivalent content was already present in the selected context.
    Duplicate,
    /// A newer item superseded this revision.
    Superseded,
    /// Item exceeded its TTL or retention period.
    Expired,
    /// Item trust was insufficient for its intended context level.
    Unverified,
    /// Full content moved to artifact storage and was replaced by a reference.
    Offloaded,
    /// Tool result no longer belonged to a valid call/result pair.
    StaleToolOutput,
    /// Privacy policy prohibited including or offloading the content.
    PrivacyRestricted,
    /// Tool state was malformed or could not be paired safely.
    InvalidToolState,
    /// An active policy explicitly excluded the item.
    PolicyDenied,
    /// Значение, не входящее в известный справочник этой версии.
    Unknown,
}

impl DropReason {
    /// Returns the stable serialized reason code.
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
    /// Stable identifier of this context item.
    pub id: String,
    /// Owning task identifier.
    pub task_id: String,
    /// Session that produced or owns the item.
    pub session_id: String,
    /// Optional identifier of the parent item or source.
    pub parent_id: Option<String>,
    /// Semantic type that determines mandatory status and budget category.
    pub kind: ItemKind,
    /// Происхождение item (имя инструмента, `memory`, `user` и т.п.).
    pub source: String,
    /// Базовый приоритет 0..100, больше значит важнее.
    pub priority: u8,
    /// Trust classification used within an instruction hierarchy level.
    pub trust: Trust,
    /// Privacy classification controlling offload and diagnostics.
    pub privacy: Privacy,
    /// unix ms.
    pub created_at: i64,
    /// unix ms.
    pub last_used_at: i64,
    /// TTL в миллисекундах от `created_at`; `None` — бессрочно.
    pub ttl_ms: Option<i64>,
    /// Retention в миллисекундах от `created_at`; `None` — бессрочно.
    pub retention_ms: Option<i64>,
    /// Whether this item is protected from ordinary eviction.
    pub pinned: bool,
    /// Ревизия item в пределах одного `parent_id`/ключа.
    pub version: u32,
    /// Tokenizer version used to compute `estimated_tokens`.
    pub tokenizer_version: String,
    /// Normalized digest used for deduplication and ledger identity.
    pub content_hash: String,
    /// Original content size in bytes.
    pub bytes: u64,
    /// Estimated number of model tokens in this item.
    pub estimated_tokens: u32,
    /// Whether the planner selected this item for the outgoing context.
    pub selected: bool,
    /// Reason the item was excluded or replaced, if any.
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
/// Default threshold used to classify optional low-priority items.
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
    /// Starts an item with default metadata and the required identity fields.
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

    /// Assigns the owning task and session identifiers.
    pub fn task(mut self, task_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        self.item.task_id = task_id.into();
        self.item.session_id = session_id.into();
        self
    }

    /// Assigns the item origin, such as a tool name or `user`.
    pub fn source(mut self, source: impl Into<String>) -> Self {
        self.item.source = source.into();
        self
    }

    /// Assigns base priority, clamped to the supported `0..=100` range.
    pub fn priority(mut self, priority: u8) -> Self {
        self.item.priority = priority.min(100);
        self
    }

    /// Assigns the trust classification.
    pub fn trust(mut self, trust: Trust) -> Self {
        self.item.trust = trust;
        self
    }

    /// Assigns the privacy classification.
    pub fn privacy(mut self, privacy: Privacy) -> Self {
        self.item.privacy = privacy;
        self
    }

    /// Assigns creation time and initializes last-used time to match.
    pub fn created_at(mut self, created_at: i64) -> Self {
        self.item.created_at = created_at;
        self.item.last_used_at = created_at;
        self
    }

    /// Sets a TTL in milliseconds from the creation time.
    pub fn ttl_ms(mut self, ttl_ms: i64) -> Self {
        self.item.ttl_ms = Some(ttl_ms);
        self
    }

    /// Sets a retention period in milliseconds from the creation time.
    pub fn retention_ms(mut self, retention_ms: i64) -> Self {
        self.item.retention_ms = Some(retention_ms);
        self
    }

    /// Marks whether ordinary context reduction may remove this item.
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.item.pinned = pinned;
        self
    }

    /// Sets the revision number within this item's parent or conflict key.
    pub fn version(mut self, version: u32) -> Self {
        self.item.version = version;
        self
    }

    /// Associates this item with its source parent item.
    pub fn parent(mut self, parent_id: impl Into<String>) -> Self {
        self.item.parent_id = Some(parent_id.into());
        self
    }

    /// Marks the scratchpad lifecycle state when this item came from scratchpad.
    pub fn scratchpad_status(mut self, status: ScratchpadStatus) -> Self {
        self.item.scratchpad_status = Some(status);
        self
    }

    /// Assigns the deterministic key used to detect conflicting revisions.
    pub fn conflict_key(mut self, key: impl Into<String>) -> Self {
        self.item.conflict_key = Some(key.into());
        self
    }

    /// Records whether a tool result has a complete matching call/result pair.
    pub fn tool_pair_complete(mut self, complete: bool) -> Self {
        self.item.tool_pair_complete = complete;
        self
    }

    /// Sets raw byte size and estimated model-token count.
    pub fn sizes(mut self, bytes: u64, estimated_tokens: u32) -> Self {
        self.item.bytes = bytes;
        self.item.estimated_tokens = estimated_tokens;
        self
    }

    /// Returns the configured context item.
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
