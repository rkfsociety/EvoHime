//! Memory Extraction: bounded извлечение фактов из диалога с policy gate.
//!
//! Модуль детерминированный и не владеет ни persistence, ни сетью: он решает,
//! может ли кандидат стать памятью, каким state он получает и какой TTL/риск
//! ему соответствует. Единственный владелец extraction/validation/policy —
//! Core; всё, что приходит от модели, — это `candidate`, а не память.
//!
//! Ключевые инварианты (контракт описан в `docs/architecture.md`, раздел
//! «Memory Extraction»; исходный план удалён после реализации):
//!
//! * ни один model-generated candidate не становится активной памятью без
//!   strict trigger + policy или явного approval;
//! * `model_confidence` описывает уверенность извлекателя, а
//!   `verification_confidence` поднимает только версионируемая verification
//!   policy — повтор факта моделью не повышает уверенность;
//! * любой неясный kind/subject/scope/privacy/риск переводит запись в
//!   `pending_confirmation`;
//! * `secret` отвергается до persistence.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Версия policy: попадает в каждую запись и меняется при изменении порогов,
/// классов риска или правил gate.
pub const POLICY_VERSION: &str = "extraction-policy-v1";
/// Версия извлекателя (prompt + контракт structured output).
pub const EXTRACTOR_VERSION: &str = "extractor-v1";
/// Версия детерминированного нормализатора canonical subject.
pub const CANONICALIZER_VERSION: &str = "canonical-v1";
/// Версия verification policy: единственный механизм, повышающий
/// `verification_confidence`.
pub const VALIDATOR_VERSION: &str = "validator-v1";

pub const MAX_STATEMENT_CHARS: usize = 4_096;
pub const MAX_CANONICAL_SUBJECT_CHARS: usize = 512;
pub const MAX_REASON_CHARS: usize = 1_024;
pub const MAX_PROVENANCE_BYTES: usize = 8_192;
pub const MAX_STRUCTURED_OUTPUT_BYTES: usize = 16 * 1024;
pub const MAX_CANDIDATES_PER_TURN: usize = 5;
pub const MAX_CANDIDATES_PER_HOUR: usize = 30;
pub const MAX_EXTRACTION_TOKENS_PER_HOUR: u64 = 100_000;
pub const MAX_CONTEXT_MESSAGES: usize = 10;
pub const MAX_CONTEXT_TOKENS: usize = 2_048;

/// Ambient-бюджеты (этап 04.6). Они отдельные и заведомо меньше диалоговых:
/// речь идёт непрерывно, поэтому общий часовой лимит диалога ambient-путь
/// выбрал бы за несколько минут и вытеснил бы извлечение из самого диалога.
pub const MAX_AMBIENT_CANDIDATES_PER_HOUR: usize = 6;
pub const MAX_AMBIENT_EPISODES_PER_HOUR: usize = 12;
pub const MAX_AMBIENT_EXTRACTION_TOKENS_PER_HOUR: u64 = 20_000;

/// Задержки повторов на malformed output (мс) — максимум 2 повтора.
pub const RETRY_DELAYS_MS: [u64; 2] = [250, 1_000];
pub const MALFORMED_BREAKER_THRESHOLD: usize = 3;
pub const MALFORMED_BREAKER_WINDOW_MS: u64 = 10 * 60 * 1_000;
pub const MALFORMED_BREAKER_COOLDOWN_MS: u64 = 15 * 60 * 1_000;

/// Таймауты verification hook.
pub const FILESYSTEM_VALIDATION_TIMEOUT_MS: u64 = 2_000;
pub const TOOL_VALIDATION_TIMEOUT_MS: u64 = 5_000;

const DAY_MS: u64 = 24 * 60 * 60 * 1_000;
/// `session_summary` живёт до конца сессии и ещё сутки.
pub const SESSION_SUMMARY_GRACE_MS: u64 = DAY_MS;
/// Retention для старых encrypted backups после forget.
pub const FORGET_BACKUP_RETENTION_MS: u64 = 7 * DAY_MS;

// ---------------------------------------------------------------------------
// Bounded перечисления домена
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Preference,
    Constraint,
    Decision,
    Entity,
    Lesson,
    SessionSummary,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Preference => "preference",
            Self::Constraint => "constraint",
            Self::Decision => "decision",
            Self::Entity => "entity",
            Self::Lesson => "lesson",
            Self::SessionSummary => "session_summary",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "preference" => Some(Self::Preference),
            "constraint" => Some(Self::Constraint),
            "decision" => Some(Self::Decision),
            "entity" => Some(Self::Entity),
            "lesson" => Some(Self::Lesson),
            "session_summary" => Some(Self::SessionSummary),
            _ => None,
        }
    }

    /// Production defaults из плана. Конфигурируемы через `ExtractionPolicy`,
    /// но именно эти значения обязательны по умолчанию.
    pub fn default_ttl_ms(self) -> u64 {
        match self {
            Self::Preference => 180 * DAY_MS,
            Self::Constraint => 30 * DAY_MS,
            Self::Decision => 180 * DAY_MS,
            Self::Entity => 365 * DAY_MS,
            Self::Lesson => 365 * DAY_MS,
            // Живёт до конца сессии плюс сутки; точный конец сессии
            // добавляет вызывающий, здесь — верхняя граница.
            Self::SessionSummary => SESSION_SUMMARY_GRACE_MS,
        }
    }

    /// `constraint` и `decision` влияют на действия и всегда требуют approval.
    pub fn always_requires_approval(self) -> bool {
        matches!(self, Self::Constraint | Self::Decision)
    }

    /// `session_summary` не участвует в long-term retrieval и не может быть
    /// promoted в persistent memory без отдельного явного подтверждения.
    pub fn is_session_only(self) -> bool {
        matches!(self, Self::SessionSummary)
    }

    /// Default scope для kind, если модель scope не предложила.
    pub fn default_scope(self) -> MemoryScopeLevel {
        match self {
            Self::Preference => MemoryScopeLevel::Workspace,
            Self::Constraint | Self::Decision | Self::Lesson => MemoryScopeLevel::Project,
            Self::Entity => MemoryScopeLevel::Project,
            Self::SessionSummary => MemoryScopeLevel::Session,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeLevel {
    Task,
    Project,
    Workspace,
    Session,
}

impl MemoryScopeLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Project => "project",
            Self::Workspace => "workspace",
            Self::Session => "session",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "task" => Some(Self::Task),
            "project" => Some(Self::Project),
            "workspace" => Some(Self::Workspace),
            "session" => Some(Self::Session),
            _ => None,
        }
    }

    /// Приоритет для retrieval: `task` > `project` > `workspace` > `session`.
    /// Более узкая запись не уничтожает широкую: она лишь выигрывает в своём
    /// scope.
    pub fn precedence(self) -> u8 {
        match self {
            Self::Task => 3,
            Self::Project => 2,
            Self::Workspace => 1,
            Self::Session => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrust {
    User,
    ToolOutput,
    Document,
    ModelInference,
    /// Постоянное слушание (04.6): говорящий не подтверждён, диаризации нет,
    /// поэтому доверия к такому источнику меньше, чем к любому другому.
    Ambient,
}

impl SourceTrust {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::ToolOutput => "tool_output",
            Self::Document => "document",
            Self::ModelInference => "model_inference",
            Self::Ambient => "ambient",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user" => Some(Self::User),
            "tool_output" => Some(Self::ToolOutput),
            "document" => Some(Self::Document),
            "model_inference" => Some(Self::ModelInference),
            "ambient" => Some(Self::Ambient),
            _ => None,
        }
    }

    /// Пришёл ли факт из постоянного слушания.
    pub fn is_ambient(self) -> bool {
        matches!(self, Self::Ambient)
    }

    /// Только явное утверждение пользователя может быть основанием
    /// strict-mode сохранения без approval.
    pub fn can_ground_strict_save(self) -> bool {
        matches!(self, Self::User)
    }

    /// `document` факты требуют Local Agentic RAG validation, `tool_output` —
    /// verification hook, `ambient` — подтверждения человеком: говорящего
    /// никто не проверял, и считать такой факт проверенным нельзя.
    pub fn requires_validation(self) -> bool {
        matches!(self, Self::ToolOutput | Self::Document | Self::Ambient)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    Normal,
    Sensitive,
    Secret,
}

impl PrivacyLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Sensitive => "sensitive",
            Self::Secret => "secret",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(Self::Normal),
            "sensitive" => Some(Self::Sensitive),
            "secret" => Some(Self::Secret),
            _ => None,
        }
    }

    /// `sensitive` не попадает в обычный audit/body response и маскируется в
    /// renderer.
    pub fn redacts_body_by_default(self) -> bool {
        !matches!(self, Self::Normal)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationState {
    /// Результат model call до policy gate.
    Candidate,
    PendingConfirmation,
    Confirmed,
    Rejected,
    Superseded,
    Expired,
    Forgotten,
}

impl ConfirmationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::PendingConfirmation => "pending_confirmation",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
            Self::Forgotten => "forgotten",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "candidate" => Some(Self::Candidate),
            "pending_confirmation" => Some(Self::PendingConfirmation),
            "confirmed" => Some(Self::Confirmed),
            "rejected" => Some(Self::Rejected),
            "superseded" => Some(Self::Superseded),
            "expired" => Some(Self::Expired),
            "forgotten" => Some(Self::Forgotten),
            _ => None,
        }
    }

    /// Только `confirmed` участвует в retrieval (и только при валидной
    /// проверке, если она обязательна).
    pub fn is_retrievable(self) -> bool {
        matches!(self, Self::Confirmed)
    }

    /// Терминальные состояния не переходят дальше.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Rejected | Self::Forgotten)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    /// Проверка не требуется policy (например, явное предпочтение
    /// пользователя).
    NotRequired,
    /// Проверка требуется, но ещё не выполнена.
    Pending,
    Valid,
    Invalid,
    /// Валидатор не смог решить: запись остаётся pending.
    Unknown,
}

impl ValidationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Pending => "pending",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Unknown => "unknown",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "not_required" => Some(Self::NotRequired),
            "pending" => Some(Self::Pending),
            "valid" => Some(Self::Valid),
            "invalid" => Some(Self::Invalid),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    /// `invalid` исключает запись из retrieval и требует новой проверки.
    pub fn allows_retrieval(self) -> bool {
        matches!(self, Self::NotRequired | Self::Valid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionMode {
    /// Автоматическое извлечение полностью отключено пользователем; ручные
    /// команды «запомни» продолжают работать через explicit trigger.
    Disabled,
    /// Первый релиз: только явные пользовательские триггеры.
    Strict,
    /// Будущий режим: extraction после каждого turn, но результат всегда
    /// `pending_confirmation`.
    Open,
}

impl ExtractionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Strict => "strict",
            Self::Open => "open",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "disabled" => Some(Self::Disabled),
            "strict" => Some(Self::Strict),
            "open" => Some(Self::Open),
            _ => None,
        }
    }
}

/// Режим ambient-извлечения (04.6). Аналога `open` здесь нет намеренно:
/// услышанное не может стать памятью без клика пользователя, поэтому
/// «включено» означает ровно `pending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbientMemoryMode {
    Off,
    Pending,
}

impl AmbientMemoryMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Pending => "pending",
        }
    }

    /// Разбор значения `EVOHIME_AMBIENT_MEMORY`. Отсутствие переменной — это
    /// дефолт `pending`, а вот мусор в ней — fail-safe `off`: неизвестное
    /// значение не должно молча включать извлечение из речи.
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(|value| value.trim().to_ascii_lowercase()) {
            None => Self::Pending,
            Some(value) if value.is_empty() => Self::Pending,
            Some(value) if value == "pending" => Self::Pending,
            Some(_) => Self::Off,
        }
    }

    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
}

impl RiskClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

// ---------------------------------------------------------------------------
// Ошибки
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractionError {
    /// Structured output больше лимита. Oversized output отклоняется целиком:
    /// без тримминга смысла и без сжатия.
    OversizedOutput {
        bytes: usize,
        max: usize,
    },
    /// JSON не разбирается или содержит неизвестные/недостающие поля.
    /// Содержимое никогда не логируется — только причина.
    MalformedOutput {
        reason: MalformedReason,
    },
    TooManyCandidates {
        count: usize,
        max: usize,
    },
    UnknownEnum {
        field: &'static str,
    },
    FieldTooLong {
        field: &'static str,
        max: usize,
    },
    EmptyField(&'static str),
    ConfidenceOutOfRange {
        field: &'static str,
    },
    /// Нормализация не дала устойчивого subject.
    UnresolvedSubject,
    /// Rate limit / circuit breaker / бюджет токенов.
    Throttled {
        reason: ThrottleReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MalformedReason {
    NotJson,
    NotAnObject,
    UnknownField,
    MissingField,
    WrongType,
}

impl MalformedReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotJson => "not_json",
            Self::NotAnObject => "not_an_object",
            Self::UnknownField => "unknown_field",
            Self::MissingField => "missing_field",
            Self::WrongType => "wrong_type",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrottleReason {
    TurnLimit,
    HourlyLimit,
    TokenBudget,
    CircuitOpen,
    ModeDisabled,
    NoExplicitTrigger,
    /// Ambient-бюджеты считаются отдельно от диалоговых: иначе два разных
    /// троттлинга стали бы неразличимы в трассах.
    AmbientCandidateLimit,
    AmbientEpisodeLimit,
}

impl ThrottleReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TurnLimit => "turn_limit",
            Self::HourlyLimit => "hourly_limit",
            Self::TokenBudget => "token_budget",
            Self::CircuitOpen => "circuit_open",
            Self::ModeDisabled => "mode_disabled",
            Self::NoExplicitTrigger => "no_explicit_trigger",
            Self::AmbientCandidateLimit => "ambient_candidate_limit",
            Self::AmbientEpisodeLimit => "ambient_episode_limit",
        }
    }
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OversizedOutput { bytes, max } => {
                write!(f, "extraction output is {bytes} bytes, limit is {max}")
            }
            Self::MalformedOutput { reason } => {
                write!(f, "extraction output is malformed: {}", reason.as_str())
            }
            Self::TooManyCandidates { count, max } => {
                write!(f, "extraction produced {count} candidates, limit is {max}")
            }
            Self::UnknownEnum { field } => write!(f, "unknown value for {field}"),
            Self::FieldTooLong { field, max } => write!(f, "{field} exceeds {max} characters"),
            Self::EmptyField(field) => write!(f, "{field} must not be empty"),
            Self::ConfidenceOutOfRange { field } => write!(f, "{field} must be within 0.0..=1.0"),
            Self::UnresolvedSubject => write!(f, "canonical subject could not be resolved"),
            Self::Throttled { reason } => {
                write!(f, "extraction is throttled: {}", reason.as_str())
            }
        }
    }
}

impl std::error::Error for ExtractionError {}

// ---------------------------------------------------------------------------
// Structured output контракт
// ---------------------------------------------------------------------------

/// Structured JSON извлекателя. `deny_unknown_fields` обязателен: неизвестные
/// поля отклоняются, а не игнорируются.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawCandidate {
    pub kind: String,
    pub statement: String,
    pub scope: String,
    pub canonical_subject: String,
    pub model_confidence: f64,
    pub verification_confidence: f64,
    pub reason: String,
    pub evidence_locator: RawEvidenceLocator,
    pub privacy: String,
    pub source_trust: String,
    pub suggested_ttl_ms: u64,
}

/// Устойчивый provenance locator. Для сообщений/инструментов — идентификаторы,
/// для файлов — логический path + content hash + line range. Полный body и
/// секреты сюда не копируются.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawEvidenceLocator {
    /// Эпизод постоянного слушания (04.6). Именно он попадает в
    /// `provenance_source_id`, поэтому удаление эпизода находит и отклоняет
    /// своих кандидатов по существующему индексу.
    #[serde(default)]
    pub episode_id: String,
    #[serde(default)]
    pub message_id: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub tool_call_id: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub content_hash: String,
    #[serde(default)]
    pub line_start: u32,
    #[serde(default)]
    pub line_end: u32,
}

impl RawEvidenceLocator {
    pub fn is_empty(&self) -> bool {
        self.episode_id.trim().is_empty()
            && self.message_id.trim().is_empty()
            && self.task_id.trim().is_empty()
            && self.tool_call_id.trim().is_empty()
            && self.file_path.trim().is_empty()
    }

    /// Компактное представление для хранения в `provenance`. Bounded: длиннее
    /// `MAX_PROVENANCE_BYTES` быть не может по построению, но проверяем.
    pub fn to_provenance_json(&self) -> Result<String, ExtractionError> {
        let json = serde_json::to_string(self).map_err(|_| ExtractionError::MalformedOutput {
            reason: MalformedReason::WrongType,
        })?;
        if json.len() > MAX_PROVENANCE_BYTES {
            return Err(ExtractionError::FieldTooLong {
                field: "evidence_locator",
                max: MAX_PROVENANCE_BYTES,
            });
        }
        Ok(json)
    }
}

/// Проверенный кандидат: enum-поля разобраны, длины ограничены, canonical
/// subject нормализован. Всё ещё не память — только вход для policy gate.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub kind: MemoryKind,
    pub statement: String,
    pub scope: MemoryScopeLevel,
    pub canonical_subject: String,
    pub raw_subject: String,
    pub model_confidence: f64,
    /// Значение из модели игнорируется policy gate: оно нормализуется в 0.0 и
    /// поднимается только verification policy.
    pub verification_confidence: f64,
    pub reason: String,
    pub evidence: RawEvidenceLocator,
    pub privacy: PrivacyLevel,
    pub source_trust: SourceTrust,
    pub suggested_ttl_ms: u64,
}

impl Candidate {
    /// Ключ конфликта: `kind + canonical_subject + scope`.
    pub fn conflict_key(&self) -> ConflictKey {
        ConflictKey {
            kind: self.kind,
            canonical_subject: self.canonical_subject.clone(),
            scope: self.scope,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConflictKey {
    pub kind: MemoryKind,
    pub canonical_subject: String,
    pub scope: MemoryScopeLevel,
}

impl fmt::Display for ConflictKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}|{}|{}",
            self.kind.as_str(),
            self.canonical_subject,
            self.scope.as_str()
        )
    }
}

// ---------------------------------------------------------------------------
// Canonical subject
// ---------------------------------------------------------------------------

/// Таблица зарегистрированных aliases. Model inference не может единолично
/// создать alias: записи сюда добавляет только Core по явному действию
/// пользователя или зарегистрированному entity id.
#[derive(Debug, Clone, Default)]
pub struct AliasTable {
    entries: BTreeMap<String, String>,
}

impl AliasTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Регистрирует alias -> canonical entity id. Обе стороны нормализуются
    /// одним и тем же детерминированным нормализатором.
    pub fn register(&mut self, alias: &str, entity_id: &str) -> Result<(), ExtractionError> {
        let alias = normalize_subject(alias)?;
        let entity = normalize_subject(entity_id)?;
        self.entries.insert(alias, entity);
        Ok(())
    }

    pub fn resolve(&self, normalized: &str) -> Option<&str> {
        self.entries.get(normalized).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Детерминированный нормализатор: Unicode NFKC, case-folding, схлопывание
/// пробелов и пунктуации. Версия — `CANONICALIZER_VERSION`.
pub fn normalize_subject(raw: &str) -> Result<String, ExtractionError> {
    use unicode_normalization::UnicodeNormalization;
    if raw.chars().count() > MAX_CANONICAL_SUBJECT_CHARS {
        return Err(ExtractionError::FieldTooLong {
            field: "canonical_subject",
            max: MAX_CANONICAL_SUBJECT_CHARS,
        });
    }
    let folded: String = raw
        .nfkc()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect();
    let normalized = folded.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Err(ExtractionError::UnresolvedSubject);
    }
    Ok(normalized)
}

/// Результат канонизации: если entity linking не подтверждён, конфликт не
/// разрешается автоматически — вызывающий обязан отправить кандидата в
/// `pending_confirmation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSubject {
    pub value: String,
    pub resolved_via_alias: bool,
    /// Нормализация дала слишком короткий/неоднозначный ключ.
    pub ambiguous: bool,
}

pub fn canonicalize_subject(
    raw: &str,
    aliases: &AliasTable,
) -> Result<CanonicalSubject, ExtractionError> {
    let normalized = normalize_subject(raw)?;
    if let Some(entity) = aliases.resolve(&normalized) {
        return Ok(CanonicalSubject {
            value: entity.to_owned(),
            resolved_via_alias: true,
            ambiguous: false,
        });
    }
    // Односимвольный или чисто числовой subject не считается устойчивым
    // entity linking: такой кандидат не может разрешать конфликты сам.
    let ambiguous = normalized.chars().count() < 2
        || normalized
            .chars()
            .all(|character| character.is_numeric() || character == ' ');
    Ok(CanonicalSubject {
        value: normalized,
        resolved_via_alias: false,
        ambiguous,
    })
}

// ---------------------------------------------------------------------------
// Explicit trigger (strict mode)
// ---------------------------------------------------------------------------

/// Явные пользовательские триггеры strict-режима. Только они, произнесённые
/// пользователем, дают основание для strict-mode сохранения.
const EXPLICIT_TRIGGERS: &[&str] = &[
    "запомни",
    "запомнить",
    "запомните",
    "не забудь",
    "не забывай",
    "важно",
    "ограничение",
    "правило",
    "учти",
    "учитывай",
    "remember",
    "important",
    "constraint",
    "keep in mind",
    "note that",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerMatch {
    pub keyword: &'static str,
}

/// Ищет явный триггер в сообщении пользователя. Текст нормализуется тем же
/// детерминированным нормализатором, что и subject, чтобы регистр и
/// пунктуация не влияли на результат.
pub fn detect_explicit_trigger(user_message: &str) -> Option<TriggerMatch> {
    let normalized = normalize_subject(user_message).ok()?;
    let padded = format!(" {normalized} ");
    EXPLICIT_TRIGGERS
        .iter()
        .find(|keyword| {
            normalize_subject(keyword)
                .map(|needle| padded.contains(&format!(" {needle} ")))
                .unwrap_or(false)
        })
        .map(|keyword| TriggerMatch { keyword })
}

// ---------------------------------------------------------------------------
// Секреты
// ---------------------------------------------------------------------------

/// Грубый, но детерминированный детектор секретов. Секреты не сохраняются
/// вообще: такой кандидат отвергается до persistence, независимо от того,
/// какой `privacy` предложила модель.
pub fn looks_like_secret(statement: &str) -> bool {
    statement.split_whitespace().any(|token| {
        let lower = token.to_ascii_lowercase();
        lower.starts_with("sk-")
            || lower.starts_with("ghp_")
            || lower.starts_with("github_pat_")
            || lower.starts_with("bearer")
            || lower.starts_with("api_key=")
            || lower.starts_with("apikey=")
            || lower.starts_with("token=")
            || lower.starts_with("password=")
            || lower.starts_with("secret=")
            || lower.starts_with("aiza")
            || lower.starts_with("-----begin")
    })
}

/// Категории, которые план относит к high-risk независимо от kind.
const HIGH_RISK_MARKERS: &[&str] = &[
    "пароль",
    "пароли",
    "токен",
    "ключ",
    "диагноз",
    "болезнь",
    "здоровь",
    "медицин",
    "юридическ",
    "контракт",
    "договор",
    "счет",
    "счёт",
    "карта",
    "платеж",
    "платёж",
    "банк",
    "налог",
    "зарплат",
    "password",
    "token",
    "credential",
    "secret",
    "diagnosis",
    "medical",
    "health",
    "legal",
    "contract",
    "invoice",
    "payment",
    "bank",
    "salary",
    "tax",
    "security",
];

fn has_high_risk_marker(statement: &str) -> bool {
    let lower = statement.to_lowercase();
    HIGH_RISK_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

/// Версионируемые пороги. Конфигурируются только Core.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractionPolicy {
    pub version: &'static str,
    pub low_risk_min_confidence: f64,
    pub medium_risk_min_confidence: f64,
    pub min_verification_confidence: f64,
    pub max_candidates_per_turn: usize,
    pub max_candidates_per_hour: usize,
    pub max_statement_chars: usize,
    pub max_output_bytes: usize,
    pub max_tokens_per_hour: u64,
    /// Ambient-бюджеты живут в той же policy, но своими полями: общий
    /// часовой лимит диалога речь выбрала бы за минуты.
    pub max_ambient_candidates_per_hour: usize,
    pub max_ambient_episodes_per_hour: usize,
    pub max_ambient_tokens_per_hour: u64,
}

impl Default for ExtractionPolicy {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION,
            low_risk_min_confidence: 0.85,
            medium_risk_min_confidence: 0.95,
            min_verification_confidence: 0.80,
            max_candidates_per_turn: MAX_CANDIDATES_PER_TURN,
            max_candidates_per_hour: MAX_CANDIDATES_PER_HOUR,
            max_statement_chars: MAX_STATEMENT_CHARS,
            max_output_bytes: MAX_STRUCTURED_OUTPUT_BYTES,
            max_tokens_per_hour: MAX_EXTRACTION_TOKENS_PER_HOUR,
            max_ambient_candidates_per_hour: MAX_AMBIENT_CANDIDATES_PER_HOUR,
            max_ambient_episodes_per_hour: MAX_AMBIENT_EPISODES_PER_HOUR,
            max_ambient_tokens_per_hour: MAX_AMBIENT_EXTRACTION_TOKENS_PER_HOUR,
        }
    }
}

/// Классификация риска по плану:
///
/// * low — явное неперсональное предпочтение формата или рабочего процесса
///   (`normal` privacy);
/// * medium — preference/entity, влияющие на проект;
/// * high — constraint, decision, действие с внешним эффектом, а также
///   security/health/legal/financial данные.
pub fn classify_risk(candidate: &Candidate) -> RiskClass {
    if candidate.privacy != PrivacyLevel::Normal || has_high_risk_marker(&candidate.statement) {
        return RiskClass::High;
    }
    match candidate.kind {
        MemoryKind::Constraint | MemoryKind::Decision => RiskClass::High,
        MemoryKind::Preference => {
            // Low только для workspace/session-уровня: предпочтение,
            // привязанное к проекту или задаче, уже влияет на проект.
            if matches!(
                candidate.scope,
                MemoryScopeLevel::Workspace | MemoryScopeLevel::Session
            ) {
                RiskClass::Low
            } else {
                RiskClass::Medium
            }
        }
        MemoryKind::Entity | MemoryKind::Lesson => RiskClass::Medium,
        MemoryKind::SessionSummary => RiskClass::Low,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyOutcome {
    /// Запись может стать активной памятью сразу.
    AutoConfirm,
    /// Требуется approval пользователя.
    Pending,
    /// Кандидат отвергается до persistence.
    Reject,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyReason {
    ExplicitUserTriggerLowRisk,
    OpenModeAlwaysPending,
    KindRequiresApproval,
    HighRiskRequiresApproval,
    ConfidenceBelowThreshold,
    UntrustedSource,
    AmbiguousSubject,
    SensitivePrivacy,
    ValidationRequired,
    SecretNeverStored,
    ExtractionDisabled,
    NoExplicitTrigger,
    SessionOnly,
    /// Услышанное никогда не подтверждается само.
    AmbientNeverAutoConfirms,
}

impl PolicyReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitUserTriggerLowRisk => "explicit_user_trigger_low_risk",
            Self::OpenModeAlwaysPending => "open_mode_always_pending",
            Self::KindRequiresApproval => "kind_requires_approval",
            Self::HighRiskRequiresApproval => "high_risk_requires_approval",
            Self::ConfidenceBelowThreshold => "confidence_below_threshold",
            Self::UntrustedSource => "untrusted_source",
            Self::AmbiguousSubject => "ambiguous_subject",
            Self::SensitivePrivacy => "sensitive_privacy",
            Self::ValidationRequired => "validation_required",
            Self::SecretNeverStored => "secret_never_stored",
            Self::ExtractionDisabled => "extraction_disabled",
            Self::NoExplicitTrigger => "no_explicit_trigger",
            Self::SessionOnly => "session_only",
            Self::AmbientNeverAutoConfirms => "ambient_never_auto_confirms",
        }
    }
}

/// Решение policy gate. Именно оно, а не модель, определяет state записи.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyDecision {
    pub outcome: PolicyOutcome,
    pub state: ConfirmationState,
    pub risk: RiskClass,
    pub reason: PolicyReason,
    pub validation_status: ValidationStatus,
    pub ttl_ms: u64,
    pub policy_version: &'static str,
    pub extractor_version: &'static str,
    /// Session-scoped запись не создаёт persistent row.
    pub session_only: bool,
}

/// Контекст turn'а: режим, явный триггер и источник.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnContext {
    pub mode: ExtractionMode,
    pub trigger: Option<TriggerMatch>,
    /// Явное утверждение пользователя в этом turn'е (а не пересказ моделью).
    pub user_asserted: bool,
}

impl TurnContext {
    pub fn strict_with_trigger(keyword: &'static str) -> Self {
        Self {
            mode: ExtractionMode::Strict,
            trigger: Some(TriggerMatch { keyword }),
            user_asserted: true,
        }
    }

    pub fn open() -> Self {
        Self {
            mode: ExtractionMode::Open,
            trigger: None,
            user_asserted: false,
        }
    }

    /// Контекст ambient-извлечения: триггером служит закрытие эпизода, а не
    /// фраза пользователя, поэтому `user_asserted` здесь ложно по
    /// построению. Подделывать «ход» с ambient-текстом вместо реплики
    /// пользователя нельзя: это сломало бы смысл `user_asserted`.
    pub fn ambient(mode: ExtractionMode) -> Self {
        Self {
            mode,
            trigger: None,
            user_asserted: false,
        }
    }
}

/// Единственный policy gate. Любая неясность даёт `pending_confirmation`.
pub fn evaluate(
    candidate: &Candidate,
    context: &TurnContext,
    subject: &CanonicalSubject,
    policy: &ExtractionPolicy,
) -> PolicyDecision {
    let risk = classify_risk(candidate);
    let ttl_ms = bounded_ttl(candidate);
    let validation_status = if candidate.source_trust.requires_validation() {
        ValidationStatus::Pending
    } else {
        ValidationStatus::NotRequired
    };
    let reject = |reason| PolicyDecision {
        outcome: PolicyOutcome::Reject,
        state: ConfirmationState::Rejected,
        risk,
        reason,
        validation_status,
        ttl_ms,
        policy_version: policy.version,
        extractor_version: EXTRACTOR_VERSION,
        session_only: candidate.kind.is_session_only(),
    };
    let pending = |reason| PolicyDecision {
        outcome: PolicyOutcome::Pending,
        state: ConfirmationState::PendingConfirmation,
        risk,
        reason,
        validation_status,
        ttl_ms,
        policy_version: policy.version,
        extractor_version: EXTRACTOR_VERSION,
        session_only: candidate.kind.is_session_only(),
    };

    // Секреты не сохраняются вообще — ни pending, ни confirmed.
    if candidate.privacy == PrivacyLevel::Secret || looks_like_secret(&candidate.statement) {
        return reject(PolicyReason::SecretNeverStored);
    }
    // Услышанное не подтверждается автоматически ни при каких значениях
    // kind/scope/privacy/confidence. Гейт стоит здесь явно, а не выводится
    // из того, что кандидат «и так упадёт ниже»: единственный путь до
    // `AutoConfirm` не должен зависеть от порядка не связанных с ambient
    // проверок. Общий выключатель извлечения при этом старше частного и
    // проверяется до вызова `evaluate`, в самой ambient-точке входа.
    if candidate.source_trust.is_ambient() {
        return pending(PolicyReason::AmbientNeverAutoConfirms);
    }
    if context.mode == ExtractionMode::Disabled && context.trigger.is_none() {
        return reject(PolicyReason::ExtractionDisabled);
    }
    // В open-режиме результат всегда проходит pending_confirmation.
    if context.mode == ExtractionMode::Open {
        return pending(PolicyReason::OpenModeAlwaysPending);
    }
    // Strict-режим без явного триггера ничего не сохраняет.
    let Some(_trigger) = context.trigger.as_ref() else {
        return reject(PolicyReason::NoExplicitTrigger);
    };
    // session_summary никогда не становится persistent памятью автоматически.
    if candidate.kind.is_session_only() {
        return pending(PolicyReason::SessionOnly);
    }
    if candidate.kind.always_requires_approval() {
        return pending(PolicyReason::KindRequiresApproval);
    }
    if risk == RiskClass::High {
        return pending(PolicyReason::HighRiskRequiresApproval);
    }
    if candidate.privacy != PrivacyLevel::Normal {
        return pending(PolicyReason::SensitivePrivacy);
    }
    // Только явное утверждение пользователя может быть основанием
    // strict-mode сохранения.
    if !context.user_asserted || !candidate.source_trust.can_ground_strict_save() {
        return pending(PolicyReason::UntrustedSource);
    }
    if subject.ambiguous {
        return pending(PolicyReason::AmbiguousSubject);
    }
    if validation_status == ValidationStatus::Pending {
        return pending(PolicyReason::ValidationRequired);
    }
    let threshold = match risk {
        RiskClass::Low => policy.low_risk_min_confidence,
        RiskClass::Medium => policy.medium_risk_min_confidence,
        // High сюда не доходит, но порог должен быть недостижимым.
        RiskClass::High => f64::INFINITY,
    };
    if candidate.model_confidence < threshold {
        return pending(PolicyReason::ConfidenceBelowThreshold);
    }
    if risk != RiskClass::Low {
        // В strict-режиме автосохранение допустимо только для low-risk.
        return pending(PolicyReason::HighRiskRequiresApproval);
    }
    PolicyDecision {
        outcome: PolicyOutcome::AutoConfirm,
        state: ConfirmationState::Confirmed,
        risk,
        reason: PolicyReason::ExplicitUserTriggerLowRisk,
        validation_status,
        ttl_ms,
        policy_version: policy.version,
        extractor_version: EXTRACTOR_VERSION,
        session_only: false,
    }
}

/// TTL кандидата: предложение модели ограничено сверху default'ом kind'а и
/// никогда не равно нулю.
pub fn bounded_ttl(candidate: &Candidate) -> u64 {
    let default = candidate.kind.default_ttl_ms();
    if candidate.suggested_ttl_ms == 0 {
        default
    } else {
        candidate.suggested_ttl_ms.min(default)
    }
}

// ---------------------------------------------------------------------------
// Ambient (04.6)
// ---------------------------------------------------------------------------

/// Какие kind вообще принимаются из речи.
///
/// `constraint` и `decision` отбрасываются до persistence: они влияют на
/// действия агента, а услышанная в комнате фраза — слишком дешёвое
/// основание, чтобы ошибиться в них. `session_summary` не принимается по
/// другой причине: у ambient нет диалоговой сессии, а session-note не
/// проходит через очередь подтверждения и потому не был бы виден
/// пользователю до того, как начнёт работать.
pub fn ambient_kind_allowed(kind: MemoryKind) -> bool {
    matches!(
        kind,
        MemoryKind::Preference | MemoryKind::Entity | MemoryKind::Lesson
    )
}

/// Местоимения и обороты, которыми высказывание говорит не о говорящем.
const THIRD_PARTY_MARKERS: &[&str] = &[
    "он",
    "она",
    "они",
    "его",
    "ее",
    "её",
    "их",
    "him",
    "her",
    "them",
    "they",
    "he",
    "she",
    "ему",
    "ей",
    "им",
    "него",
    "нее",
    "неё",
    "них",
    "сказал",
    "сказала",
    "сказали",
    "говорит",
    "говорил",
    "говорила",
    "просил",
    "просила",
    "просят",
    "обещал",
    "обещала",
    "считает",
    "считают",
    "said",
    "says",
    "told",
    "asked",
    "promised",
    "thinks",
];

/// Распознан ли в высказывании субъект не в первом лице.
///
/// Диаризации и голосового профиля в v1 нет намеренно: голосовой шаблон —
/// это биометрия, а ошибка диаризации приписала бы пользователю чужое
/// утверждение. Поэтому вместо «кто это сказал» проверяется «о ком это
/// сказано», и проверка нарочно перестраховывается: заглавное слово в
/// середине фразы считается именем. Ложное срабатывание здесь стоит скрытого
/// тела записи, ложный пропуск — чужого утверждения в памяти пользователя.
pub fn mentions_third_party(statement: &str) -> bool {
    let Ok(normalized) = normalize_subject(statement) else {
        // Нормализовать нечего: решать не по чему, и это не повод считать
        // высказывание безопасным.
        return true;
    };
    let padded = format!(" {normalized} ");
    if THIRD_PARTY_MARKERS
        .iter()
        .any(|marker| padded.contains(&format!(" {marker} ")))
    {
        return true;
    }
    // Имя собственное: слово с заглавной буквы не в начале фразы и не после
    // точки.
    let mut sentence_start = true;
    for word in statement.split_whitespace() {
        let first = word.chars().next();
        let starts_upper = first.is_some_and(|character| character.is_uppercase());
        if starts_upper && !sentence_start {
            return true;
        }
        sentence_start = word.ends_with('.') || word.ends_with('!') || word.ends_with('?');
    }
    false
}

/// Поднимает `privacy_class` ambient-кандидата минимум до `sensitive`, если
/// в высказывании распознан субъект не в первом лице. По существующему коду
/// это даёт `SensitivePrivacy`, pending и скрытое тело записи. Возвращает
/// `true`, если класс был поднят.
pub fn apply_ambient_privacy_floor(candidate: &mut Candidate) -> bool {
    if candidate.privacy != PrivacyLevel::Normal {
        return false;
    }
    if !mentions_third_party(&candidate.statement) {
        return false;
    }
    candidate.privacy = PrivacyLevel::Sensitive;
    true
}

// ---------------------------------------------------------------------------
// Парсинг structured output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawExtraction {
    pub candidates: Vec<RawCandidate>,
}

/// Разбирает structured output извлекателя. Ничего из содержимого наружу не
/// уходит: при ошибке возвращается только её причина.
pub fn parse_extraction(
    raw: &str,
    policy: &ExtractionPolicy,
) -> Result<Vec<RawCandidate>, ExtractionError> {
    if raw.len() > policy.max_output_bytes {
        return Err(ExtractionError::OversizedOutput {
            bytes: raw.len(),
            max: policy.max_output_bytes,
        });
    }
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| ExtractionError::MalformedOutput {
            reason: MalformedReason::NotJson,
        })?;
    if !value.is_object() {
        return Err(ExtractionError::MalformedOutput {
            reason: MalformedReason::NotAnObject,
        });
    }
    let extraction: RawExtraction =
        serde_json::from_value(value).map_err(|error| ExtractionError::MalformedOutput {
            reason: classify_serde_error(&error),
        })?;
    if extraction.candidates.len() > policy.max_candidates_per_turn {
        return Err(ExtractionError::TooManyCandidates {
            count: extraction.candidates.len(),
            max: policy.max_candidates_per_turn,
        });
    }
    Ok(extraction.candidates)
}

fn classify_serde_error(error: &serde_json::Error) -> MalformedReason {
    // Сообщение serde не содержит пользовательского контента: только имена
    // полей и типы, поэтому классификация безопасна.
    let text = error.to_string();
    if text.contains("unknown field") {
        MalformedReason::UnknownField
    } else if text.contains("missing field") {
        MalformedReason::MissingField
    } else {
        MalformedReason::WrongType
    }
}

/// Валидирует один raw candidate и нормализует его subject.
pub fn validate_candidate(
    raw: &RawCandidate,
    aliases: &AliasTable,
    policy: &ExtractionPolicy,
) -> Result<(Candidate, CanonicalSubject), ExtractionError> {
    let kind =
        MemoryKind::parse(&raw.kind).ok_or(ExtractionError::UnknownEnum { field: "kind" })?;
    let scope = MemoryScopeLevel::parse(&raw.scope)
        .ok_or(ExtractionError::UnknownEnum { field: "scope" })?;
    let privacy = PrivacyLevel::parse(&raw.privacy)
        .ok_or(ExtractionError::UnknownEnum { field: "privacy" })?;
    let source_trust =
        SourceTrust::parse(&raw.source_trust).ok_or(ExtractionError::UnknownEnum {
            field: "source_trust",
        })?;
    if raw.statement.trim().is_empty() {
        return Err(ExtractionError::EmptyField("statement"));
    }
    if raw.statement.chars().count() > policy.max_statement_chars {
        return Err(ExtractionError::FieldTooLong {
            field: "statement",
            max: policy.max_statement_chars,
        });
    }
    if raw.reason.chars().count() > MAX_REASON_CHARS {
        return Err(ExtractionError::FieldTooLong {
            field: "reason",
            max: MAX_REASON_CHARS,
        });
    }
    if !(0.0..=1.0).contains(&raw.model_confidence) {
        return Err(ExtractionError::ConfidenceOutOfRange {
            field: "model_confidence",
        });
    }
    if !(0.0..=1.0).contains(&raw.verification_confidence) {
        return Err(ExtractionError::ConfidenceOutOfRange {
            field: "verification_confidence",
        });
    }
    if raw.evidence_locator.is_empty() {
        return Err(ExtractionError::EmptyField("evidence_locator"));
    }
    raw.evidence_locator.to_provenance_json()?;
    let subject = canonicalize_subject(&raw.canonical_subject, aliases)?;
    let candidate = Candidate {
        kind,
        statement: raw.statement.trim().to_owned(),
        scope,
        canonical_subject: subject.value.clone(),
        raw_subject: raw.canonical_subject.clone(),
        model_confidence: raw.model_confidence,
        // Повторение моделью факта confidence не повышает: пока не отработала
        // verification policy, verification_confidence равен нулю.
        verification_confidence: 0.0,
        reason: raw.reason.trim().to_owned(),
        evidence: raw.evidence_locator.clone(),
        privacy,
        source_trust,
        suggested_ttl_ms: raw.suggested_ttl_ms,
    };
    Ok((candidate, subject))
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Результат verification hook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationOutcome {
    /// `None` — валидатор не смог решить (`unknown`).
    pub valid: Option<bool>,
    pub confidence: f64,
    pub checked_at_ms: u64,
    pub validator_version: String,
    pub evidence_digest: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationVerdict {
    pub status: ValidationStatus,
    pub verification_confidence: f64,
    pub validated_at_ms: u64,
    pub validator_version: String,
    pub evidence_digest: String,
}

/// Применяет verification policy. Это единственный механизм, повышающий
/// `verification_confidence`.
pub fn apply_verification(
    outcome: &VerificationOutcome,
    policy: &ExtractionPolicy,
) -> VerificationVerdict {
    let confidence = outcome.confidence.clamp(0.0, 1.0);
    let status = match outcome.valid {
        Some(true) if confidence >= policy.min_verification_confidence => ValidationStatus::Valid,
        // Валидный по мнению hook'а, но с недостаточной уверенностью, — это
        // не подтверждение: запись остаётся неразрешённой.
        Some(true) => ValidationStatus::Unknown,
        Some(false) => ValidationStatus::Invalid,
        None => ValidationStatus::Unknown,
    };
    VerificationVerdict {
        status,
        verification_confidence: if status == ValidationStatus::Valid {
            confidence
        } else {
            0.0
        },
        validated_at_ms: outcome.checked_at_ms,
        validator_version: outcome.validator_version.clone(),
        evidence_digest: outcome.evidence_digest.clone(),
    }
}

/// Класс проверки, определяющий таймаут: filesystem/git — 2 с, tool/API — 5 с.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationTarget {
    Filesystem,
    Tool,
}

impl ValidationTarget {
    pub fn timeout_ms(self) -> u64 {
        match self {
            Self::Filesystem => FILESYSTEM_VALIDATION_TIMEOUT_MS,
            Self::Tool => TOOL_VALIDATION_TIMEOUT_MS,
        }
    }
}

/// Какой валидатор нужен кандидату. `None` — проверка не требуется policy.
pub fn validation_target(candidate: &Candidate) -> Option<ValidationTarget> {
    if !candidate.source_trust.requires_validation() {
        return None;
    }
    if candidate.evidence.file_path.trim().is_empty() {
        Some(ValidationTarget::Tool)
    } else {
        Some(ValidationTarget::Filesystem)
    }
}

/// Решение по файловой evidence: содержимое файла всё ещё то, на которое
/// ссылался кандидат. Сравниваются только digest'ы — тело не покидает
/// валидатор.
pub fn file_evidence_outcome(
    expected_hash: &str,
    actual_hash: Option<&str>,
    checked_at_ms: u64,
) -> VerificationOutcome {
    match actual_hash {
        // Кандидат не указал hash: проверить нечего, но и подтвердить нельзя.
        _ if expected_hash.trim().is_empty() => VerificationOutcome {
            valid: None,
            confidence: 0.0,
            checked_at_ms,
            validator_version: VALIDATOR_VERSION.to_owned(),
            evidence_digest: actual_hash.unwrap_or_default().to_owned(),
            reason: "missing_expected_hash".to_owned(),
        },
        Some(actual) if actual == expected_hash => VerificationOutcome {
            valid: Some(true),
            confidence: 1.0,
            checked_at_ms,
            validator_version: VALIDATOR_VERSION.to_owned(),
            evidence_digest: actual.to_owned(),
            reason: "content_hash_matches".to_owned(),
        },
        Some(actual) => VerificationOutcome {
            valid: Some(false),
            confidence: 1.0,
            checked_at_ms,
            validator_version: VALIDATOR_VERSION.to_owned(),
            evidence_digest: actual.to_owned(),
            reason: "content_hash_changed".to_owned(),
        },
        // Файл недоступен или проверка не уложилась в таймаут: это `unknown`,
        // а не отрицательный результат.
        None => VerificationOutcome {
            valid: None,
            confidence: 0.0,
            checked_at_ms,
            validator_version: VALIDATOR_VERSION.to_owned(),
            evidence_digest: String::new(),
            reason: "evidence_unreadable".to_owned(),
        },
    }
}

/// Изменение file hash, git revision или tool version инвалидирует прошлую
/// проверку.
pub fn verification_is_stale(
    stored_digest: &str,
    stored_validator_version: &str,
    current_digest: &str,
    current_validator_version: &str,
) -> bool {
    stored_digest != current_digest || stored_validator_version != current_validator_version
}

// ---------------------------------------------------------------------------
// Rate limit, budget и circuit breaker
// ---------------------------------------------------------------------------

/// Детерминированный guard: время всегда передаётся снаружи, чтобы поведение
/// было воспроизводимо в тестах.
#[derive(Debug, Clone, Default)]
pub struct ExtractionGuard {
    candidates_this_turn: usize,
    candidate_timestamps_ms: Vec<u64>,
    malformed_timestamps_ms: Vec<u64>,
    breaker_open_until_ms: Option<u64>,
    token_events: Vec<(u64, u64)>,
    /// Ambient-счётчики отдельные: circuit breaker общий, а бюджеты — нет.
    ambient_candidate_timestamps_ms: Vec<u64>,
    ambient_episode_timestamps_ms: Vec<u64>,
    ambient_token_events: Vec<(u64, u64)>,
}

impl ExtractionGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Начало нового turn'а: per-turn счётчик обнуляется.
    pub fn begin_turn(&mut self) {
        self.candidates_this_turn = 0;
    }

    /// Можно ли вообще запускать extraction сейчас.
    pub fn check_can_extract(
        &mut self,
        mode: ExtractionMode,
        trigger: Option<&TriggerMatch>,
        now_ms: u64,
        policy: &ExtractionPolicy,
    ) -> Result<(), ExtractionError> {
        if mode == ExtractionMode::Disabled && trigger.is_none() {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled,
            });
        }
        if mode == ExtractionMode::Strict && trigger.is_none() {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::NoExplicitTrigger,
            });
        }
        if self.breaker_is_open(now_ms) {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::CircuitOpen,
            });
        }
        if self.tokens_in_last_hour(now_ms) >= policy.max_tokens_per_hour {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::TokenBudget,
            });
        }
        Ok(())
    }

    /// Можно ли запускать ambient-извлечение сейчас.
    ///
    /// Это отдельный гейт, а не ветка `check_can_extract`: та отвергает всё
    /// без явного триггера в strict-режиме — то есть заблокировала бы
    /// ambient полностью, потому что триггером ambient служит закрытие
    /// эпизода, а не фраза пользователя. Circuit breaker переиспользуется
    /// как есть: malformed output извлекателя одинаково плох в обоих путях.
    /// Общий выключатель старше частного, поэтому `ExtractionMode::Disabled`
    /// закрывает и ambient, каким бы ни было `EVOHIME_AMBIENT_MEMORY`.
    pub fn check_can_extract_ambient(
        &mut self,
        ambient: AmbientMemoryMode,
        mode: ExtractionMode,
        now_ms: u64,
        policy: &ExtractionPolicy,
    ) -> Result<(), ExtractionError> {
        if mode == ExtractionMode::Disabled || !ambient.is_enabled() {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled,
            });
        }
        if self.breaker_is_open(now_ms) {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::CircuitOpen,
            });
        }
        if self.ambient_tokens_in_last_hour(now_ms) >= policy.max_ambient_tokens_per_hour {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::TokenBudget,
            });
        }
        Ok(())
    }

    /// Регистрирует эпизод, отданный в извлечение. Переполнение — отказ без
    /// очереди: отложенный эпизод к моменту разбора уже мог быть удалён.
    pub fn register_ambient_episode(
        &mut self,
        now_ms: u64,
        policy: &ExtractionPolicy,
    ) -> Result<(), ExtractionError> {
        self.prune(now_ms);
        if self.ambient_episode_timestamps_ms.len() >= policy.max_ambient_episodes_per_hour {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::AmbientEpisodeLimit,
            });
        }
        self.ambient_episode_timestamps_ms.push(now_ms);
        Ok(())
    }

    /// Регистрирует ambient-кандидата в его собственном часовом лимите.
    pub fn register_ambient_candidate(
        &mut self,
        now_ms: u64,
        policy: &ExtractionPolicy,
    ) -> Result<(), ExtractionError> {
        self.prune(now_ms);
        if self.ambient_candidate_timestamps_ms.len() >= policy.max_ambient_candidates_per_hour {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::AmbientCandidateLimit,
            });
        }
        self.ambient_candidate_timestamps_ms.push(now_ms);
        Ok(())
    }

    /// Учитывает ambient-токены в их собственном бюджете.
    pub fn register_ambient_tokens(&mut self, now_ms: u64, tokens: u64) {
        self.prune(now_ms);
        self.ambient_token_events.push((now_ms, tokens));
    }

    pub fn ambient_tokens_in_last_hour(&self, now_ms: u64) -> u64 {
        let window_start = now_ms.saturating_sub(60 * 60 * 1_000);
        self.ambient_token_events
            .iter()
            .filter(|(at, _)| *at >= window_start)
            .map(|(_, tokens)| *tokens)
            .sum()
    }

    /// Регистрирует появление кандидата, соблюдая per-turn и hourly лимиты.
    pub fn register_candidate(
        &mut self,
        now_ms: u64,
        policy: &ExtractionPolicy,
    ) -> Result<(), ExtractionError> {
        if self.candidates_this_turn >= policy.max_candidates_per_turn {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::TurnLimit,
            });
        }
        self.prune(now_ms);
        if self.candidate_timestamps_ms.len() >= policy.max_candidates_per_hour {
            return Err(ExtractionError::Throttled {
                reason: ThrottleReason::HourlyLimit,
            });
        }
        self.candidates_this_turn += 1;
        self.candidate_timestamps_ms.push(now_ms);
        Ok(())
    }

    /// Учитывает потраченные extraction-токены (input + output).
    pub fn register_tokens(&mut self, now_ms: u64, tokens: u64) {
        self.prune(now_ms);
        self.token_events.push((now_ms, tokens));
    }

    pub fn tokens_in_last_hour(&self, now_ms: u64) -> u64 {
        let window_start = now_ms.saturating_sub(60 * 60 * 1_000);
        self.token_events
            .iter()
            .filter(|(at, _)| *at >= window_start)
            .map(|(_, tokens)| *tokens)
            .sum()
    }

    /// Три malformed output за 10 минут включают circuit breaker на 15 минут.
    pub fn register_malformed(&mut self, now_ms: u64) {
        self.malformed_timestamps_ms.push(now_ms);
        let window_start = now_ms.saturating_sub(MALFORMED_BREAKER_WINDOW_MS);
        self.malformed_timestamps_ms
            .retain(|at| *at >= window_start);
        if self.malformed_timestamps_ms.len() >= MALFORMED_BREAKER_THRESHOLD {
            self.breaker_open_until_ms = Some(now_ms + MALFORMED_BREAKER_COOLDOWN_MS);
            self.malformed_timestamps_ms.clear();
        }
    }

    pub fn breaker_is_open(&self, now_ms: u64) -> bool {
        self.breaker_open_until_ms
            .is_some_and(|until| now_ms < until)
    }

    pub fn breaker_open_until_ms(&self) -> Option<u64> {
        self.breaker_open_until_ms
    }

    /// Задержка перед повтором. `attempt` считается с нуля; после двух
    /// повторов возвращается `None`.
    pub fn retry_delay_ms(attempt: usize) -> Option<u64> {
        RETRY_DELAYS_MS.get(attempt).copied()
    }

    fn prune(&mut self, now_ms: u64) {
        let window_start = now_ms.saturating_sub(60 * 60 * 1_000);
        self.candidate_timestamps_ms
            .retain(|at| *at >= window_start);
        self.token_events.retain(|(at, _)| *at >= window_start);
        self.ambient_candidate_timestamps_ms
            .retain(|at| *at >= window_start);
        self.ambient_episode_timestamps_ms
            .retain(|at| *at >= window_start);
        self.ambient_token_events
            .retain(|(at, _)| *at >= window_start);
    }
}

// ---------------------------------------------------------------------------
// Конфликты
// ---------------------------------------------------------------------------

/// Минимальное представление активной записи для сравнения с кандидатом.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveMemorySummary {
    pub id: String,
    pub kind: MemoryKind,
    pub canonical_subject: String,
    pub scope: MemoryScopeLevel,
    pub statement: String,
    pub state: ConfirmationState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConflictVerdict {
    /// Конфликта нет.
    None,
    /// Тот же смысл: дубликат, новую запись создавать не нужно.
    Duplicate { existing_id: String },
    /// Несовместимые statements при равном ключе: старая запись остаётся
    /// активной, новая уходит в pending до явного выбора пользователя.
    Conflict { existing_id: String },
}

/// Конфликт определяется по `kind + canonical_subject + scope` и
/// несовместимым statements. Равные scopes с несовместимыми statements
/// образуют conflict; разные scopes конфликта не образуют.
pub fn detect_conflict(candidate: &Candidate, active: &[ActiveMemorySummary]) -> ConflictVerdict {
    let key = candidate.conflict_key();
    for existing in active
        .iter()
        .filter(|existing| existing.state.is_retrievable())
    {
        let existing_key = ConflictKey {
            kind: existing.kind,
            canonical_subject: existing.canonical_subject.clone(),
            scope: existing.scope,
        };
        if existing_key != key {
            continue;
        }
        if statements_are_equivalent(&existing.statement, &candidate.statement) {
            return ConflictVerdict::Duplicate {
                existing_id: existing.id.clone(),
            };
        }
        return ConflictVerdict::Conflict {
            existing_id: existing.id.clone(),
        };
    }
    ConflictVerdict::None
}

fn statements_are_equivalent(left: &str, right: &str) -> bool {
    match (normalize_subject(left), normalize_subject(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Причина supersede: обязательна, чтобы цепочка `A -> B -> C` объясняла себя.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupersessionReason {
    UserChoice,
    Revalidated,
    Expired,
    Corrected,
}

impl SupersessionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserChoice => "user_choice",
            Self::Revalidated => "revalidated",
            Self::Expired => "expired",
            Self::Corrected => "corrected",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "user_choice" => Some(Self::UserChoice),
            "revalidated" => Some(Self::Revalidated),
            "expired" => Some(Self::Expired),
            "corrected" => Some(Self::Corrected),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Retrieval
// ---------------------------------------------------------------------------

/// Может ли запись участвовать в retrieval прямо сейчас. Истёкшие, forgotten,
/// invalid и session-only записи исключаются.
pub fn is_retrievable(
    state: ConfirmationState,
    validation: ValidationStatus,
    kind: MemoryKind,
    expires_at_ms: Option<u64>,
    now_ms: u64,
) -> bool {
    if !state.is_retrievable() || !validation.allows_retrieval() {
        return false;
    }
    if kind.is_session_only() {
        return false;
    }
    expires_at_ms.is_none_or(|expires| now_ms < expires)
}

/// Приоритет scope при retrieval: более узкая запись выигрывает в своём
/// scope, но не уничтожает более широкую.
pub fn sort_by_scope_precedence(records: &mut [ActiveMemorySummary]) {
    records.sort_by(|left, right| {
        right
            .scope
            .precedence()
            .cmp(&left.scope.precedence())
            .then_with(|| left.id.cmp(&right.id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(kind: &str, scope: &str, trust: &str, confidence: f64) -> RawCandidate {
        RawCandidate {
            kind: kind.to_owned(),
            statement: "Использовать русский язык в UI".to_owned(),
            scope: scope.to_owned(),
            canonical_subject: "Язык интерфейса".to_owned(),
            model_confidence: confidence,
            verification_confidence: 0.99,
            reason: "пользователь сказал явно".to_owned(),
            evidence_locator: RawEvidenceLocator {
                message_id: "msg-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: "normal".to_owned(),
            source_trust: trust.to_owned(),
            suggested_ttl_ms: 0,
        }
    }

    fn candidate(raw: &RawCandidate) -> (Candidate, CanonicalSubject) {
        validate_candidate(raw, &AliasTable::new(), &ExtractionPolicy::default())
            .expect("candidate validates")
    }

    #[test]
    fn model_verification_confidence_is_never_trusted() {
        let (candidate, _) = candidate(&raw("preference", "workspace", "user", 0.9));
        assert_eq!(candidate.verification_confidence, 0.0);
    }

    #[test]
    fn strict_low_risk_user_preference_can_auto_confirm() {
        let source = raw("preference", "workspace", "user", 0.9);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::AutoConfirm);
        assert_eq!(decision.state, ConfirmationState::Confirmed);
        assert_eq!(decision.risk, RiskClass::Low);
        assert_eq!(decision.ttl_ms, 180 * DAY_MS);
    }

    #[test]
    fn strict_low_risk_below_threshold_falls_back_to_pending() {
        let source = raw("preference", "workspace", "user", 0.84);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::ConfidenceBelowThreshold);
    }

    #[test]
    fn model_inference_never_grounds_strict_save() {
        let source = raw("preference", "workspace", "model_inference", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::UntrustedSource);
    }

    #[test]
    fn constraint_and_decision_always_require_approval() {
        for kind in ["constraint", "decision"] {
            let source = raw(kind, "project", "user", 1.0);
            let (candidate, subject) = candidate(&source);
            let decision = evaluate(
                &candidate,
                &TurnContext::strict_with_trigger("запомни"),
                &subject,
                &ExtractionPolicy::default(),
            );
            assert_eq!(decision.outcome, PolicyOutcome::Pending, "{kind}");
            assert_eq!(decision.reason, PolicyReason::KindRequiresApproval);
            assert_eq!(decision.risk, RiskClass::High);
        }
    }

    #[test]
    fn open_mode_always_produces_pending() {
        let source = raw("preference", "workspace", "user", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::open(),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.state, ConfirmationState::PendingConfirmation);
        assert_eq!(decision.reason, PolicyReason::OpenModeAlwaysPending);
    }

    #[test]
    fn strict_mode_without_trigger_rejects() {
        let source = raw("preference", "workspace", "user", 1.0);
        let (candidate, subject) = candidate(&source);
        let context = TurnContext {
            mode: ExtractionMode::Strict,
            trigger: None,
            user_asserted: true,
        };
        let decision = evaluate(&candidate, &context, &subject, &ExtractionPolicy::default());
        assert_eq!(decision.outcome, PolicyOutcome::Reject);
        assert_eq!(decision.reason, PolicyReason::NoExplicitTrigger);
    }

    #[test]
    fn secrets_are_rejected_before_persistence() {
        let mut source = raw("preference", "workspace", "user", 1.0);
        source.statement = "ключ доступа sk-live-1234567890".to_owned();
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Reject);
        assert_eq!(decision.reason, PolicyReason::SecretNeverStored);
    }

    #[test]
    fn session_summary_is_never_auto_promoted() {
        let source = raw("session_summary", "session", "user", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::SessionOnly);
        assert!(decision.session_only);
    }

    #[test]
    fn tool_output_requires_validation_before_confirmation() {
        let source = raw("preference", "workspace", "tool_output", 1.0);
        let (candidate, subject) = candidate(&source);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.validation_status, ValidationStatus::Pending);
    }

    #[test]
    fn high_risk_markers_force_pending_for_any_kind() {
        let mut source = raw("entity", "project", "user", 1.0);
        source.statement = "Медицинский диагноз клиента известен команде".to_owned();
        let (candidate, subject) = candidate(&source);
        assert_eq!(classify_risk(&candidate), RiskClass::High);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.reason, PolicyReason::HighRiskRequiresApproval);
    }

    #[test]
    fn ambiguous_subject_falls_back_to_pending() {
        let mut source = raw("preference", "workspace", "user", 1.0);
        source.canonical_subject = "42".to_owned();
        let (candidate, subject) = candidate(&source);
        assert!(subject.ambiguous);
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.reason, PolicyReason::AmbiguousSubject);
    }

    #[test]
    fn parse_rejects_oversized_malformed_and_unknown_fields() {
        let policy = ExtractionPolicy::default();
        let oversized = "x".repeat(policy.max_output_bytes + 1);
        assert!(matches!(
            parse_extraction(&oversized, &policy),
            Err(ExtractionError::OversizedOutput { .. })
        ));
        assert!(matches!(
            parse_extraction("not json", &policy),
            Err(ExtractionError::MalformedOutput {
                reason: MalformedReason::NotJson
            })
        ));
        assert!(matches!(
            parse_extraction("[]", &policy),
            Err(ExtractionError::MalformedOutput {
                reason: MalformedReason::NotAnObject
            })
        ));
        let unknown = serde_json::json!({
            "candidates": [],
            "extra": 1
        })
        .to_string();
        assert!(matches!(
            parse_extraction(&unknown, &policy),
            Err(ExtractionError::MalformedOutput {
                reason: MalformedReason::UnknownField
            })
        ));
    }

    #[test]
    fn parse_bounds_candidate_count_per_turn() {
        let policy = ExtractionPolicy::default();
        let candidates = (0..policy.max_candidates_per_turn + 1)
            .map(|_| serde_json::to_value(raw("preference", "workspace", "user", 0.9)).unwrap())
            .collect::<Vec<_>>();
        let payload = serde_json::json!({ "candidates": candidates }).to_string();
        assert!(matches!(
            parse_extraction(&payload, &policy),
            Err(ExtractionError::TooManyCandidates { .. })
        ));
    }

    #[test]
    fn unknown_enum_values_are_rejected() {
        let policy = ExtractionPolicy::default();
        let aliases = AliasTable::new();
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.kind = "belief".to_owned();
        assert_eq!(
            validate_candidate(&source, &aliases, &policy),
            Err(ExtractionError::UnknownEnum { field: "kind" })
        );
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.privacy = "top_secret".to_owned();
        assert_eq!(
            validate_candidate(&source, &aliases, &policy),
            Err(ExtractionError::UnknownEnum { field: "privacy" })
        );
    }

    #[test]
    fn evidence_locator_is_required() {
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.evidence_locator = RawEvidenceLocator::default();
        assert_eq!(
            validate_candidate(&source, &AliasTable::new(), &ExtractionPolicy::default()),
            Err(ExtractionError::EmptyField("evidence_locator"))
        );
    }

    #[test]
    fn canonicalization_is_unicode_and_alias_aware() {
        assert_eq!(
            normalize_subject("  Язык   ИНТЕРФЕЙСА! ").unwrap(),
            "язык интерфейса"
        );
        // NFKC складывает полноширинные формы к обычным.
        assert_eq!(normalize_subject("ＵＩ").unwrap(), "ui");
        let mut aliases = AliasTable::new();
        aliases.register("UI язык", "entity:ui-language").unwrap();
        let resolved = canonicalize_subject("ui  ЯЗЫК", &aliases).unwrap();
        assert!(resolved.resolved_via_alias);
        assert_eq!(resolved.value, "entity ui language");
        assert!(!resolved.ambiguous);
    }

    #[test]
    fn triggers_are_detected_in_both_languages() {
        assert!(detect_explicit_trigger("Запомни: сборка идёт через cargo").is_some());
        assert!(detect_explicit_trigger("Please remember the build order").is_some());
        assert!(detect_explicit_trigger("расскажи про сборку").is_none());
    }

    #[test]
    fn scope_precedence_orders_task_over_session() {
        let mut records = vec![
            ActiveMemorySummary {
                id: "s".into(),
                kind: MemoryKind::Entity,
                canonical_subject: "x".into(),
                scope: MemoryScopeLevel::Session,
                statement: "a".into(),
                state: ConfirmationState::Confirmed,
            },
            ActiveMemorySummary {
                id: "t".into(),
                kind: MemoryKind::Entity,
                canonical_subject: "x".into(),
                scope: MemoryScopeLevel::Task,
                statement: "b".into(),
                state: ConfirmationState::Confirmed,
            },
        ];
        sort_by_scope_precedence(&mut records);
        assert_eq!(records[0].id, "t");
    }

    #[test]
    fn conflicts_need_same_key_and_incompatible_statement() {
        let (candidate, _) = candidate(&raw("preference", "workspace", "user", 0.9));
        let same_scope = ActiveMemorySummary {
            id: "m-1".into(),
            kind: MemoryKind::Preference,
            canonical_subject: candidate.canonical_subject.clone(),
            scope: MemoryScopeLevel::Workspace,
            statement: "Использовать английский язык в UI".into(),
            state: ConfirmationState::Confirmed,
        };
        assert_eq!(
            detect_conflict(&candidate, std::slice::from_ref(&same_scope)),
            ConflictVerdict::Conflict {
                existing_id: "m-1".into()
            }
        );
        let duplicate = ActiveMemorySummary {
            statement: candidate.statement.clone(),
            ..same_scope.clone()
        };
        assert_eq!(
            detect_conflict(&candidate, &[duplicate]),
            ConflictVerdict::Duplicate {
                existing_id: "m-1".into()
            }
        );
        let narrower = ActiveMemorySummary {
            scope: MemoryScopeLevel::Task,
            ..same_scope.clone()
        };
        assert_eq!(
            detect_conflict(&candidate, &[narrower]),
            ConflictVerdict::None
        );
        let pending = ActiveMemorySummary {
            state: ConfirmationState::PendingConfirmation,
            ..same_scope
        };
        assert_eq!(
            detect_conflict(&candidate, &[pending]),
            ConflictVerdict::None
        );
    }

    #[test]
    fn verification_only_source_of_confidence() {
        let policy = ExtractionPolicy::default();
        let valid = apply_verification(
            &VerificationOutcome {
                valid: Some(true),
                confidence: 0.9,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "file hash matches".into(),
            },
            &policy,
        );
        assert_eq!(valid.status, ValidationStatus::Valid);
        assert_eq!(valid.verification_confidence, 0.9);

        let weak = apply_verification(
            &VerificationOutcome {
                valid: Some(true),
                confidence: 0.5,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "weak".into(),
            },
            &policy,
        );
        assert_eq!(weak.status, ValidationStatus::Unknown);
        assert_eq!(weak.verification_confidence, 0.0);

        let invalid = apply_verification(
            &VerificationOutcome {
                valid: Some(false),
                confidence: 1.0,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "file changed".into(),
            },
            &policy,
        );
        assert_eq!(invalid.status, ValidationStatus::Invalid);
        assert!(!invalid.status.allows_retrieval());

        let unknown = apply_verification(
            &VerificationOutcome {
                valid: None,
                confidence: 1.0,
                checked_at_ms: 10,
                validator_version: VALIDATOR_VERSION.into(),
                evidence_digest: "digest-1".into(),
                reason: "timeout".into(),
            },
            &policy,
        );
        assert_eq!(unknown.status, ValidationStatus::Unknown);
    }

    #[test]
    fn file_evidence_distinguishes_changed_from_unreadable() {
        let policy = ExtractionPolicy::default();
        let matched = file_evidence_outcome("abc", Some("abc"), 5);
        assert_eq!(
            apply_verification(&matched, &policy).status,
            ValidationStatus::Valid
        );

        let changed = file_evidence_outcome("abc", Some("def"), 5);
        assert_eq!(
            apply_verification(&changed, &policy).status,
            ValidationStatus::Invalid
        );

        // Недоступная evidence и таймаут — это `unknown`: запись остаётся
        // pending, а не отвергается как ложная.
        let unreadable = file_evidence_outcome("abc", None, 5);
        assert_eq!(
            apply_verification(&unreadable, &policy).status,
            ValidationStatus::Unknown
        );

        let no_expectation = file_evidence_outcome("", Some("abc"), 5);
        assert_eq!(
            apply_verification(&no_expectation, &policy).status,
            ValidationStatus::Unknown
        );
    }

    #[test]
    fn validation_target_follows_source_trust_and_evidence() {
        let (user_candidate, _) = candidate(&raw("preference", "workspace", "user", 0.9));
        assert_eq!(validation_target(&user_candidate), None);

        let (tool_candidate, _) = candidate(&raw("entity", "project", "tool_output", 0.9));
        assert_eq!(
            validation_target(&tool_candidate),
            Some(ValidationTarget::Tool)
        );
        assert_eq!(ValidationTarget::Tool.timeout_ms(), 5_000);

        let mut source = raw("entity", "project", "document", 0.9);
        source.evidence_locator.file_path = "docs/plan.md".to_owned();
        let (document_candidate, _) = candidate(&source);
        assert_eq!(
            validation_target(&document_candidate),
            Some(ValidationTarget::Filesystem)
        );
        assert_eq!(ValidationTarget::Filesystem.timeout_ms(), 2_000);
    }

    #[test]
    fn hash_or_validator_change_invalidates_previous_check() {
        assert!(verification_is_stale("a", "v1", "b", "v1"));
        assert!(verification_is_stale("a", "v1", "a", "v2"));
        assert!(!verification_is_stale("a", "v1", "a", "v1"));
    }

    #[test]
    fn guard_enforces_turn_hour_and_breaker_limits() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        guard.begin_turn();
        for index in 0..policy.max_candidates_per_turn {
            guard
                .register_candidate(1_000 + index as u64, &policy)
                .expect("within turn limit");
        }
        assert!(matches!(
            guard.register_candidate(2_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::TurnLimit
            })
        ));

        // Часовой лимит держится поверх per-turn счётчика.
        let mut hourly = ExtractionGuard::new();
        for index in 0..policy.max_candidates_per_hour {
            hourly.begin_turn();
            hourly
                .register_candidate(1_000 + index as u64, &policy)
                .expect("within hourly limit");
        }
        hourly.begin_turn();
        assert!(matches!(
            hourly.register_candidate(2_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::HourlyLimit
            })
        ));

        let mut breaker = ExtractionGuard::new();
        breaker.register_malformed(0);
        breaker.register_malformed(1_000);
        assert!(!breaker.breaker_is_open(1_000));
        breaker.register_malformed(2_000);
        assert!(breaker.breaker_is_open(2_000));
        assert!(!breaker.breaker_is_open(2_000 + MALFORMED_BREAKER_COOLDOWN_MS));
        assert!(matches!(
            breaker.check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                2_500,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::CircuitOpen
            })
        ));
    }

    #[test]
    fn malformed_outside_window_does_not_open_breaker() {
        let mut guard = ExtractionGuard::new();
        guard.register_malformed(0);
        guard.register_malformed(1_000);
        guard.register_malformed(MALFORMED_BREAKER_WINDOW_MS + 2_000);
        assert!(!guard.breaker_is_open(MALFORMED_BREAKER_WINDOW_MS + 2_000));
    }

    #[test]
    fn token_budget_stops_extraction_for_the_hour() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        guard.register_tokens(1_000, policy.max_tokens_per_hour);
        assert!(matches!(
            guard.check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                2_000,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::TokenBudget
            })
        ));
        // Через час бюджет освобождается.
        assert!(guard
            .check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                1_000 + 60 * 60 * 1_000 + 1,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn disabled_mode_still_allows_manual_trigger() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        assert!(matches!(
            guard.check_can_extract(ExtractionMode::Disabled, None, 1, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled
            })
        ));
        assert!(guard
            .check_can_extract(
                ExtractionMode::Disabled,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                1,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn retry_delays_are_bounded_to_two_attempts() {
        assert_eq!(ExtractionGuard::retry_delay_ms(0), Some(250));
        assert_eq!(ExtractionGuard::retry_delay_ms(1), Some(1_000));
        assert_eq!(ExtractionGuard::retry_delay_ms(2), None);
    }

    #[test]
    fn retrieval_excludes_expired_invalid_and_session_records() {
        assert!(is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Valid,
            MemoryKind::Entity,
            Some(100),
            50
        ));
        assert!(!is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Valid,
            MemoryKind::Entity,
            Some(100),
            100
        ));
        assert!(!is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Invalid,
            MemoryKind::Entity,
            None,
            0
        ));
        assert!(!is_retrievable(
            ConfirmationState::PendingConfirmation,
            ValidationStatus::Valid,
            MemoryKind::Entity,
            None,
            0
        ));
        assert!(!is_retrievable(
            ConfirmationState::Confirmed,
            ValidationStatus::Valid,
            MemoryKind::SessionSummary,
            None,
            0
        ));
    }

    #[test]
    fn policy_path_costs_far_less_than_the_latency_budget() {
        // The plan budgets <= 200 ms p95 of added turn latency. Everything in
        // this module is deterministic and I/O-free; the only remaining cost
        // is the model call, which runs after the answer has been sent.
        let policy = ExtractionPolicy::default();
        let aliases = AliasTable::new();
        let payload = serde_json::json!({
            "candidates": (0..policy.max_candidates_per_turn)
                .map(|_| serde_json::to_value(raw("preference", "workspace", "user", 0.9)).unwrap())
                .collect::<Vec<_>>()
        })
        .to_string();
        let started = std::time::Instant::now();
        for _ in 0..100 {
            let parsed = parse_extraction(&payload, &policy).expect("parses");
            for candidate in &parsed {
                let (candidate, subject) =
                    validate_candidate(candidate, &aliases, &policy).expect("validates");
                let _ = evaluate(
                    &candidate,
                    &TurnContext::strict_with_trigger("запомни"),
                    &subject,
                    &policy,
                );
            }
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(200),
            "100 full policy passes took {elapsed:?}, budget is 200 ms for one turn"
        );
    }

    // ---------------------------------------------------------------------
    // Ambient (04.6)
    // ---------------------------------------------------------------------

    #[test]
    fn ambient_never_auto_confirms_in_any_combination() {
        let kinds = [
            MemoryKind::Preference,
            MemoryKind::Constraint,
            MemoryKind::Decision,
            MemoryKind::Entity,
            MemoryKind::Lesson,
            MemoryKind::SessionSummary,
        ];
        let scopes = [
            MemoryScopeLevel::Task,
            MemoryScopeLevel::Project,
            MemoryScopeLevel::Workspace,
            MemoryScopeLevel::Session,
        ];
        let privacies = [PrivacyLevel::Normal, PrivacyLevel::Sensitive];
        let confidences = [0.0, 0.5, 0.85, 0.95, 1.0];
        let subjects = [
            CanonicalSubject {
                value: "язык интерфейса".to_owned(),
                resolved_via_alias: true,
                ambiguous: false,
            },
            CanonicalSubject {
                value: "x".to_owned(),
                resolved_via_alias: false,
                ambiguous: true,
            },
        ];
        let modes = [
            ExtractionMode::Strict,
            ExtractionMode::Open,
            ExtractionMode::Disabled,
        ];
        let policy = ExtractionPolicy::default();
        let mut checked = 0usize;
        for kind in kinds {
            for scope in scopes {
                for privacy in privacies {
                    for confidence in confidences {
                        for subject in &subjects {
                            for mode in modes {
                                let candidate = Candidate {
                                    kind,
                                    statement: "использовать русский язык в интерфейсе".to_owned(),
                                    scope,
                                    canonical_subject: subject.value.clone(),
                                    raw_subject: subject.value.clone(),
                                    model_confidence: confidence,
                                    verification_confidence: 0.0,
                                    reason: "услышано".to_owned(),
                                    evidence: RawEvidenceLocator {
                                        episode_id: "episode-1".to_owned(),
                                        ..RawEvidenceLocator::default()
                                    },
                                    privacy,
                                    source_trust: SourceTrust::Ambient,
                                    suggested_ttl_ms: 0,
                                };
                                let decision = evaluate(
                                    &candidate,
                                    &TurnContext::ambient(mode),
                                    subject,
                                    &policy,
                                );
                                assert_ne!(
                                    decision.outcome,
                                    PolicyOutcome::AutoConfirm,
                                    "ambient auto-confirmed {kind:?}/{scope:?}/{privacy:?}"
                                );
                                assert_ne!(decision.state, ConfirmationState::Confirmed);
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 6 * 4 * 2 * 5 * 2 * 3);
    }

    #[test]
    fn ambient_candidate_stays_pending_even_with_a_user_trigger() {
        // Явный триггер в диалоге не делает услышанное утверждением
        // пользователя: источник остаётся ambient.
        let candidate = Candidate {
            kind: MemoryKind::Preference,
            statement: "использовать тёмную тему".to_owned(),
            scope: MemoryScopeLevel::Workspace,
            canonical_subject: "тема оформления".to_owned(),
            raw_subject: "тема оформления".to_owned(),
            model_confidence: 1.0,
            verification_confidence: 0.0,
            reason: "услышано".to_owned(),
            evidence: RawEvidenceLocator {
                episode_id: "episode-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: PrivacyLevel::Normal,
            source_trust: SourceTrust::Ambient,
            suggested_ttl_ms: 0,
        };
        let subject = CanonicalSubject {
            value: "тема оформления".to_owned(),
            resolved_via_alias: true,
            ambiguous: false,
        };
        let decision = evaluate(
            &candidate,
            &TurnContext::strict_with_trigger("запомни"),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.reason, PolicyReason::AmbientNeverAutoConfirms);
    }

    #[test]
    fn ambient_secret_is_still_rejected_before_the_ambient_gate() {
        let candidate = Candidate {
            kind: MemoryKind::Entity,
            statement: "ключ sk-abcdef".to_owned(),
            scope: MemoryScopeLevel::Workspace,
            canonical_subject: "ключ".to_owned(),
            raw_subject: "ключ".to_owned(),
            model_confidence: 0.9,
            verification_confidence: 0.0,
            reason: "услышано".to_owned(),
            evidence: RawEvidenceLocator {
                episode_id: "episode-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: PrivacyLevel::Normal,
            source_trust: SourceTrust::Ambient,
            suggested_ttl_ms: 0,
        };
        let subject = CanonicalSubject {
            value: "ключ".to_owned(),
            resolved_via_alias: false,
            ambiguous: false,
        };
        let decision = evaluate(
            &candidate,
            &TurnContext::ambient(ExtractionMode::Strict),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Reject);
        assert_eq!(decision.reason, PolicyReason::SecretNeverStored);
    }

    #[test]
    fn ambient_mode_defaults_to_pending_and_fails_safe_on_garbage() {
        assert_eq!(AmbientMemoryMode::parse(None), AmbientMemoryMode::Pending);
        assert_eq!(
            AmbientMemoryMode::parse(Some(" PENDING ")),
            AmbientMemoryMode::Pending
        );
        assert_eq!(
            AmbientMemoryMode::parse(Some("off")),
            AmbientMemoryMode::Off
        );
        // Аналога `open` у ambient нет: он не «почти pending», а мусор.
        assert_eq!(
            AmbientMemoryMode::parse(Some("open")),
            AmbientMemoryMode::Off
        );
        assert_eq!(AmbientMemoryMode::parse(Some("да")), AmbientMemoryMode::Off);
    }

    #[test]
    fn ambient_runs_in_strict_mode_although_the_dialog_gate_would_refuse_it() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        // Диалоговый гейт отвергает всё без триггера в strict-режиме...
        assert_eq!(
            guard.check_can_extract(ExtractionMode::Strict, None, 0, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::NoExplicitTrigger
            })
        );
        // ...а ambient-гейт триггера не ждёт: им служит закрытие эпизода.
        assert!(guard
            .check_can_extract_ambient(
                AmbientMemoryMode::Pending,
                ExtractionMode::Strict,
                0,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn general_switch_outranks_the_ambient_one() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        assert_eq!(
            guard.check_can_extract_ambient(
                AmbientMemoryMode::Pending,
                ExtractionMode::Disabled,
                0,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled
            })
        );
        assert_eq!(
            guard.check_can_extract_ambient(
                AmbientMemoryMode::Off,
                ExtractionMode::Strict,
                0,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::ModeDisabled
            })
        );
    }

    #[test]
    fn ambient_budgets_are_counted_apart_from_the_dialog_ones() {
        let policy = ExtractionPolicy::default();
        let mut guard = ExtractionGuard::new();
        for _ in 0..policy.max_ambient_candidates_per_hour {
            assert!(guard.register_ambient_candidate(1_000, &policy).is_ok());
        }
        assert_eq!(
            guard.register_ambient_candidate(1_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::AmbientCandidateLimit
            })
        );
        for _ in 0..policy.max_ambient_episodes_per_hour {
            assert!(guard.register_ambient_episode(1_000, &policy).is_ok());
        }
        assert_eq!(
            guard.register_ambient_episode(1_000, &policy),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::AmbientEpisodeLimit
            })
        );
        // Диалоговый путь этих трат не заметил: у него свой счётчик и своя
        // причина отказа.
        guard.begin_turn();
        assert!(guard.register_candidate(1_000, &policy).is_ok());
        // Свой бюджет токенов: ambient не тратит часовой бюджет диалога.
        guard.register_ambient_tokens(1_000, policy.max_ambient_tokens_per_hour);
        assert_eq!(
            guard.check_can_extract_ambient(
                AmbientMemoryMode::Pending,
                ExtractionMode::Strict,
                1_000,
                &policy
            ),
            Err(ExtractionError::Throttled {
                reason: ThrottleReason::TokenBudget
            })
        );
        assert!(guard
            .check_can_extract(
                ExtractionMode::Strict,
                Some(&TriggerMatch {
                    keyword: "запомни"
                }),
                1_000,
                &policy
            )
            .is_ok());
    }

    #[test]
    fn constraint_and_decision_never_come_from_ambient() {
        assert!(!ambient_kind_allowed(MemoryKind::Constraint));
        assert!(!ambient_kind_allowed(MemoryKind::Decision));
        // session_summary тоже не принимается: у речи нет диалоговой сессии,
        // а session-note не проходит через очередь подтверждения.
        assert!(!ambient_kind_allowed(MemoryKind::SessionSummary));
        assert!(ambient_kind_allowed(MemoryKind::Preference));
        assert!(ambient_kind_allowed(MemoryKind::Entity));
        assert!(ambient_kind_allowed(MemoryKind::Lesson));
    }

    #[test]
    fn a_third_person_statement_is_raised_to_sensitive_and_stays_pending() {
        let mut candidate = Candidate {
            kind: MemoryKind::Entity,
            statement: "она просила присылать отчёты по понедельникам".to_owned(),
            scope: MemoryScopeLevel::Workspace,
            canonical_subject: "отчёты".to_owned(),
            raw_subject: "отчёты".to_owned(),
            model_confidence: 1.0,
            verification_confidence: 0.0,
            reason: "услышано".to_owned(),
            evidence: RawEvidenceLocator {
                episode_id: "episode-1".to_owned(),
                ..RawEvidenceLocator::default()
            },
            privacy: PrivacyLevel::Normal,
            source_trust: SourceTrust::Ambient,
            suggested_ttl_ms: 0,
        };
        assert!(apply_ambient_privacy_floor(&mut candidate));
        assert_eq!(candidate.privacy, PrivacyLevel::Sensitive);
        assert!(candidate.privacy.redacts_body_by_default());
        let subject = CanonicalSubject {
            value: "отчёты".to_owned(),
            resolved_via_alias: true,
            ambiguous: false,
        };
        let decision = evaluate(
            &candidate,
            &TurnContext::ambient(ExtractionMode::Strict),
            &subject,
            &ExtractionPolicy::default(),
        );
        assert_eq!(decision.outcome, PolicyOutcome::Pending);
        assert_eq!(decision.risk, RiskClass::High);

        // Утверждение от первого лица класс не поднимает.
        let mut plain = candidate.clone();
        plain.statement = "предпочитаю тёмную тему".to_owned();
        plain.privacy = PrivacyLevel::Normal;
        assert!(!apply_ambient_privacy_floor(&mut plain));
        assert_eq!(plain.privacy, PrivacyLevel::Normal);
    }

    #[test]
    fn third_party_detection_covers_pronouns_and_names() {
        assert!(mentions_third_party("он сказал, что сборка сломана"));
        assert!(mentions_third_party("Это просил передать Андрей"));
        assert!(!mentions_third_party("Хочу отчёты по понедельникам"));
    }

    #[test]
    fn ambient_locator_carries_the_episode_and_reaches_provenance() {
        let mut raw = raw("preference", "workspace", "ambient", 0.9);
        raw.evidence_locator = RawEvidenceLocator {
            episode_id: "episode-42".to_owned(),
            ..RawEvidenceLocator::default()
        };
        // Локатор с одним эпизодом — не пустой: иначе ambient-кандидат не
        // прошёл бы валидацию вовсе.
        assert!(!raw.evidence_locator.is_empty());
        let (candidate, _) = candidate(&raw);
        assert_eq!(candidate.source_trust, SourceTrust::Ambient);
        assert!(candidate.source_trust.requires_validation());
        assert!(!candidate.source_trust.can_ground_strict_save());
        assert_eq!(candidate.evidence.episode_id, "episode-42");
        // Хеш текста для ambient пуст: по правилу 04.1 хеш короткой фразы
        // приравнивается к её содержимому.
        assert!(candidate.evidence.content_hash.is_empty());
        let json = candidate
            .evidence
            .to_provenance_json()
            .expect("locator serialises");
        assert!(json.contains("episode-42"));
    }

    #[test]
    fn ttl_is_bounded_by_kind_default() {
        let mut source = raw("preference", "workspace", "user", 0.9);
        source.suggested_ttl_ms = 10 * 365 * DAY_MS;
        let (capped, _) = candidate(&source);
        assert_eq!(bounded_ttl(&capped), 180 * DAY_MS);
        source.suggested_ttl_ms = DAY_MS;
        let (shorter, _) = candidate(&source);
        assert_eq!(bounded_ttl(&shorter), DAY_MS);
    }
}
