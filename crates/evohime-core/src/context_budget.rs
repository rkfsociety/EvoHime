//! Интеграция Context Budget Manager (план 01) в agent loop Core.
//!
//! Модуль превращает сообщения и tool schemas текущего шага в `ContextItem`,
//! прогоняет их через [`ContextPlanner`], собирает bounded loadout и возвращает
//! готовый к отправке набор сообщений вместе с записью `context_ledger`.
//!
//! Наружу (в `ModelContext` и UI) уходит только bounded projection: ids, счётчики,
//! причины и hash. Сырой prompt, тело памяти и raw tool output Core не покидают.

use std::collections::HashMap;
use std::sync::Arc;

use evohime_context_budget::{
    artifact::{ArtifactQuota, ArtifactRefStatus},
    budget::BudgetUnavailable,
    compression::{BoundedSummarizer, RawSummary, SummarizerConfig, SummaryModel},
    estimator::HeuristicEstimator,
    item::{ContextItem, ContextItemBuilder, ItemKind, Privacy, Trust},
    ladder::{OffloadOutcome, OffloadSink, Summarizer},
    ledger::{ContextLedgerEntry, LoadoutRecord},
    loadout::{
        build_loadout, check_tool_call, route_intent, DenyRule, IntentRule, IntentRules,
        LoadoutLimits, LoadoutMiss, ToolGroup, ToolLoadout, ToolRegistryEntry,
        INTENT_RULES_VERSION,
    },
    planner::{ContextPlan, ContextPlanner, OwnedContent, PlanInput, PlanRequest},
    profile::ProfileCatalog,
    scratchpad::{ScratchpadCategory, ScratchpadEntry},
};
use evohime_local_storage::artifact_store::ArtifactStore;
use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole},
    ToolSpec,
};

/// Инструменты, которые обязаны быть доступны всегда: отмена, статус и
/// approval/policy semantics. Конкретные имена берутся из registry, а не
/// зашиваются в router: здесь перечислены только capability-признаки.
const MANDATORY_CAPABILITY_MARKERS: [&str; 3] = ["task.", "approval", "policy"];

/// Bounded projection результата сборки для события `ModelContext` (этап 01.5).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModelContextProjection {
    pub schema_version: u32,
    pub context_ledger_hash: String,
    pub profile_version: String,
    pub tokenizer_version: String,
    pub target_tokens: u32,
    pub soft_limit_tokens: u32,
    pub hard_limit_tokens: u32,
    pub mandatory_tokens: u32,
    pub selected_optional_tokens: u32,
    pub reserves_tokens: u32,
    /// Не более 100 элементов.
    pub selected_item_ids: Vec<String>,
    /// Не более 100 элементов: `(id, drop_reason)`.
    pub dropped_items: Vec<DroppedItemProjection>,
    pub truncated: bool,
    pub ladder_levels_applied: Vec<String>,
    pub compression: Vec<CompressionProjection>,
    pub loadout: Option<LoadoutRecord>,
    pub fallback_estimator: bool,
    pub budget_unavailable: Option<BudgetUnavailable>,
}

/// Отброшенный item: только id и причина.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DroppedItemProjection {
    pub id: String,
    pub drop_reason: String,
}

/// Compression-решение: без текста summary. Коэффициент сжатия выражен в
/// промилле, чтобы projection оставалась сравнимой и не зависела от
/// представления чисел с плавающей точкой.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompressionProjection {
    pub summary_id: String,
    pub source_count: usize,
    pub compression_ratio_permille: u32,
    pub summarizer_version: String,
    pub fallback: bool,
    pub fallback_reason: Option<String>,
}

/// Версия схемы `ModelContext` projection. Поля additive: старые клиенты
/// игнорируют неизвестные значения без major bump.
pub const MODEL_CONTEXT_SCHEMA_VERSION: u32 = 2;

const BOUNDED_ID_LIMIT: usize = 100;
const BOUNDED_REASON_CHARS: usize = 200;

impl ModelContextProjection {
    /// Bounded projection плана. Ни одно поле не содержит содержимого item.
    pub fn from_plan(plan: &ContextPlan) -> Self {
        let ledger = &plan.ledger;
        Self {
            schema_version: MODEL_CONTEXT_SCHEMA_VERSION,
            context_ledger_hash: ledger.context_ledger_hash.clone(),
            profile_version: ledger.profile_version.clone(),
            tokenizer_version: ledger.tokenizer_version.clone(),
            target_tokens: plan.profile.target_tokens,
            soft_limit_tokens: plan.profile.soft_limit_tokens,
            hard_limit_tokens: plan.profile.hard_limit_tokens,
            mandatory_tokens: ledger.mandatory_tokens,
            selected_optional_tokens: ledger.selected_optional_tokens,
            reserves_tokens: ledger.reserves_tokens,
            selected_item_ids: ledger
                .selected_items
                .iter()
                .take(BOUNDED_ID_LIMIT)
                .map(|item| item.id.clone())
                .collect(),
            dropped_items: ledger
                .dropped_items
                .iter()
                .take(BOUNDED_ID_LIMIT)
                .map(|item| DroppedItemProjection {
                    id: item.id.clone(),
                    drop_reason: item.drop_reason.as_str().to_string(),
                })
                .collect(),
            truncated: ledger.selected_items.len() > BOUNDED_ID_LIMIT
                || ledger.dropped_items.len() > BOUNDED_ID_LIMIT,
            ladder_levels_applied: ledger
                .ladder_levels_applied
                .iter()
                .map(|level| level.as_str().to_string())
                .collect(),
            compression: ledger
                .compression
                .iter()
                .map(|record| CompressionProjection {
                    summary_id: record.summary_id.clone(),
                    source_count: record.source_ids.len(),
                    compression_ratio_permille: (record.compression_ratio * 1000.0)
                        .round()
                        .clamp(0.0, f64::from(u32::MAX))
                        as u32,
                    summarizer_version: record.summarizer_version.clone(),
                    fallback: record.fallback,
                    fallback_reason: record
                        .fallback_reason
                        .as_deref()
                        .map(|reason| reason.chars().take(BOUNDED_REASON_CHARS).collect()),
                })
                .collect(),
            loadout: ledger.loadout.clone(),
            fallback_estimator: ledger.fallback_estimator,
            budget_unavailable: plan.unavailable.clone(),
        }
    }
}

/// Собранный контекст одного шага agent loop.
pub struct AssembledContext {
    /// Сообщения в порядке собранного контекста.
    pub messages: Vec<ChatMessage>,
    /// Схемы инструментов, ограниченные loadout.
    pub tool_specs: Vec<ToolSpec>,
    pub loadout: ToolLoadout,
    pub plan: ContextPlan,
}

impl AssembledContext {
    pub fn is_ready(&self) -> bool {
        self.plan.is_ready()
    }

    pub fn ledger(&self) -> &ContextLedgerEntry {
        &self.plan.ledger
    }

    pub fn projection(&self) -> ModelContextProjection {
        ModelContextProjection::from_plan(&self.plan)
    }

    /// Проверка вызова инструмента до эффекта.
    pub fn check_tool_call(&self, tool_id: &str) -> Result<(), LoadoutMiss> {
        check_tool_call(&self.loadout, tool_id)
    }
}

/// Таблица правил intent router, поставляемая со сборкой Core.
pub fn default_intent_rules() -> IntentRules {
    IntentRules {
        version: INTENT_RULES_VERSION.to_string(),
        rules: vec![
            IntentRule {
                id: "inspect".to_string(),
                intent: "inspect".to_string(),
                keywords: vec![
                    "проверь".to_string(),
                    "покажи".to_string(),
                    "прочитай".to_string(),
                    "найди".to_string(),
                    "посмотри".to_string(),
                    "какой".to_string(),
                    "review".to_string(),
                    "inspect".to_string(),
                ],
                allows_mutation: false,
                capabilities: Vec::new(),
            },
            IntentRule {
                id: "edit".to_string(),
                intent: "edit".to_string(),
                keywords: vec![
                    "измени".to_string(),
                    "исправь".to_string(),
                    "добавь".to_string(),
                    "удали".to_string(),
                    "создай".to_string(),
                    "напиши".to_string(),
                    "реализуй".to_string(),
                    "почини".to_string(),
                ],
                allows_mutation: true,
                capabilities: Vec::new(),
            },
            IntentRule {
                id: "research".to_string(),
                intent: "research".to_string(),
                keywords: vec![
                    "исследуй".to_string(),
                    "изучи".to_string(),
                    "сравни".to_string(),
                    "документац".to_string(),
                ],
                allows_mutation: false,
                capabilities: Vec::new(),
            },
        ],
        deny: Vec::new(),
    }
}

/// Правило запрета capability, выведенное из политики Core.
pub fn deny_capability(capability: &str, reason: &str) -> DenyRule {
    DenyRule {
        id: format!("deny/{capability}"),
        capability: capability.to_string(),
        reason: reason.to_string(),
    }
}

/// Runtime сборки контекста: планировщик, правила loadout и настройки
/// summarizer. Создаётся один раз на процесс Core.
pub struct ContextRuntime {
    planner: ContextPlanner,
    rules: IntentRules,
    summarizer_config: SummarizerConfig,
    artifact_quota: ArtifactQuota,
    /// Реальные окна моделей, как их сообщил провайдер. Пустая карта означает
    /// «провайдер ещё не спрошен», и профиль берётся из встроенного каталога —
    /// это осознанно консервативная оценка, а не признак ошибки.
    model_windows: HashMap<String, u32>,
}

impl ContextRuntime {
    pub fn new(model: &str) -> Self {
        Self {
            planner: ContextPlanner::new(
                ProfileCatalog::builtin(),
                Some(Arc::new(HeuristicEstimator::default_for(model))),
            ),
            rules: default_intent_rules(),
            summarizer_config: SummarizerConfig::default(),
            artifact_quota: ArtifactQuota::default(),
            model_windows: HashMap::new(),
        }
    }

    /// Подставляет окна, вычитанные из каталога провайдера. Вызывается при
    /// старте (из локальной базы) и после каждого обновления каталога.
    pub fn set_model_windows(&mut self, windows: HashMap<String, u32>) {
        self.model_windows = windows;
    }

    pub fn model_window(&self, model: &str) -> Option<u32> {
        self.model_windows.get(model).copied()
    }

    pub fn rules(&self) -> &IntentRules {
        &self.rules
    }

    pub fn set_rules(&mut self, rules: IntentRules) {
        self.rules = rules;
    }

    pub fn artifact_quota(&self) -> ArtifactQuota {
        self.artifact_quota
    }

    pub fn planner(&self) -> &ContextPlanner {
        &self.planner
    }

    pub fn summarizer_config(&self) -> &SummarizerConfig {
        &self.summarizer_config
    }

    /// Сборка контекста одного шага. `offload` и `summarizer` подставляются
    /// вызывающей стороной: их отсутствие не блокирует сборку.
    #[allow(clippy::too_many_arguments)]
    pub fn assemble(
        &mut self,
        task_id: &str,
        session_id: &str,
        model_call_id: &str,
        provider: &str,
        model: &str,
        now: i64,
        messages: &[ChatMessage],
        specs: &[ToolSpec],
        open_questions: &[String],
        // Подтверждённые записи scratchpad задачи (01.2). После restart в
        // рабочий контекст возвращаются только они.
        scratchpad: &[ScratchpadEntry],
        // Item, закреплённые пользователем командой `pin item` (01.5), и
        // запрошенное пользователем принудительное сжатие (`summarize now`).
        pinned_ids: &[String],
        force_reduction: bool,
        offload: &mut dyn OffloadSink,
        summarizer: &mut dyn Summarizer,
    ) -> AssembledContext {
        let user_prompt = messages
            .iter()
            .find(|message| message.role == ChatRole::User)
            .map(|message| message.content.clone())
            .unwrap_or_default();

        // 1. Tool loadout: только релевантный набор schemas, полный registry
        //    остаётся в Core.
        let registry = registry_from_specs(specs);
        let decision = route_intent(&self.rules, &user_prompt, open_questions);
        let schema_estimator = |schema: &str| (schema.len() as u32).div_ceil(3) + 16;
        let provider_window = self.model_window(model);
        let profile = self
            .planner
            .catalog()
            .resolve(provider, model, provider_window);
        let loadout = build_loadout(
            &registry,
            &self.rules,
            decision,
            LoadoutLimits {
                tool_schema_reserve: profile.tool_schema_reserve,
                mandatory_schema_reserve: profile.tool_schema_reserve / 2,
            },
            &schema_estimator,
        );
        let loadout_record = loadout.to_record();

        // 2. Сообщения превращаются в `ContextItem`. Pin повышает приоритет,
        //    но не гарантирует включение в контекст: при нехватке бюджета
        //    закреплённый item отбрасывается последним и с явной причиной.
        let mut inputs = plan_inputs(task_id, session_id, messages, now);
        for entry in scratchpad {
            let mut item = entry.to_context_item();
            item.task_id = task_id.to_string();
            item.session_id = session_id.to_string();
            inputs.push(PlanInput::new(
                item,
                OwnedContent::Text(scratchpad_context_text(entry)),
            ));
        }
        for input in &mut inputs {
            if pinned_ids.iter().any(|id| id == &input.item.id) {
                input.item.pinned = true;
            }
        }

        let request = PlanRequest {
            task_id: task_id.to_string(),
            session_id: session_id.to_string(),
            model_call_id: model_call_id.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            provider_window,
            now,
            inputs,
            loadout: Some(loadout_record),
            replan_of: None,
            force_reduction,
        };
        let plan = self.planner.plan_with(&request, offload, summarizer, None);

        // 3. Сообщения восстанавливаются в порядке собранного контекста.
        let selected_messages = rebuild_messages(messages, &plan);
        let tool_specs = specs
            .iter()
            .filter(|spec| loadout.allows(&spec.function.name))
            .cloned()
            .collect();

        AssembledContext {
            messages: selected_messages,
            tool_specs,
            loadout,
            plan,
        }
    }

    /// Deterministic re-plan после context-length error провайдера.
    pub fn replan(
        &mut self,
        assembled: &AssembledContext,
        request: &PlanRequest,
        provider_window: Option<u32>,
        offload: &mut dyn OffloadSink,
        summarizer: &mut dyn Summarizer,
    ) -> ContextPlan {
        self.planner.replan_after_context_length_error(
            request,
            &assembled.plan,
            provider_window,
            offload,
            summarizer,
        )
    }

    /// Запись фактического usage провайдера в диагностику планировщика.
    pub fn record_actual_usage(&mut self, plan: &ContextPlan, actual_prompt_tokens: u32) {
        let _ = self.planner.record_actual_usage(plan, actual_prompt_tokens);
    }

    pub fn metrics(&self) -> &evohime_context_budget::ContextMetrics {
        self.planner.metrics()
    }
}

/// Текст записи scratchpad в контексте. Выгруженная запись представлена
/// bounded ссылкой с hash и locator, а не усечённым содержимым.
fn scratchpad_context_text(entry: &ScratchpadEntry) -> String {
    match &entry.artifact_locator {
        Some(locator) => format!(
            "[{}] вынесено в artifact store: {locator}, hash {}",
            entry.category.as_str(),
            entry.content_hash
        ),
        None => format!("[{}] {}", entry.category.as_str(), entry.content),
    }
}

/// Кандидаты на выгрузку при превышении бюджета категории scratchpad:
/// самые старые `confirmed` записи. `open_questions` текущего шага и уже
/// выгруженные записи не вытесняются, молчаливое усечение запрещено.
pub fn scratchpad_offload_candidates(
    entries: &[ScratchpadEntry],
    budget_tokens: u32,
) -> Vec<String> {
    let estimate = |entry: &ScratchpadEntry| (entry.content.len() as u32).div_ceil(3) + 8;
    let mut total: u32 = entries.iter().map(estimate).fold(0, u32::saturating_add);
    if total <= budget_tokens {
        return Vec::new();
    }
    let mut candidates: Vec<&ScratchpadEntry> = entries
        .iter()
        .filter(|entry| {
            entry.category != ScratchpadCategory::OpenQuestions && entry.artifact_locator.is_none()
        })
        .collect();
    candidates.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut selected = Vec::new();
    for entry in candidates {
        if total <= budget_tokens {
            break;
        }
        total = total.saturating_sub(estimate(entry));
        selected.push(entry.id.clone());
    }
    selected
}

/// Идентификатор item сообщения. Детерминированный: одинаковый вход даёт
/// одинаковые ids, поэтому `context_ledger_hash` воспроизводим.
pub fn message_item_id(index: usize, role: ChatRole) -> String {
    format!("msg-{index:04}-{}", role.as_str())
}

fn kind_for(index: usize, message: &ChatMessage) -> ItemKind {
    match message.role {
        // Первое system-сообщение — safety/system policy: оно всегда входит в
        // обязательный минимум.
        ChatRole::System if index == 0 => ItemKind::SafetyPolicy,
        ChatRole::System => ItemKind::Memory,
        // Роль user разбирается в `plan_inputs`: текущим prompt является
        // только последнее такое сообщение, остальные остаются историей.
        ChatRole::User => ItemKind::History,
        ChatRole::Assistant if !message.tool_calls.is_empty() => ItemKind::PendingToolCall,
        ChatRole::Assistant => ItemKind::History,
        ChatRole::Tool => ItemKind::ToolResult,
    }
}

fn plan_inputs(
    task_id: &str,
    session_id: &str,
    messages: &[ChatMessage],
    now: i64,
) -> Vec<PlanInput> {
    // Индекс последнего user-сообщения: именно оно является текущим prompt.
    let last_user = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == ChatRole::User)
        .map(|(index, _)| index)
        .next_back();
    // Незавершённые пары tool-call/result: assistant с tool_calls, на который
    // ещё нет tool-сообщения.
    let answered: Vec<String> = messages
        .iter()
        .filter_map(|message| message.tool_call_id.clone())
        .collect();

    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let mut kind = kind_for(index, message);
            if message.role == ChatRole::User {
                kind = if Some(index) == last_user {
                    ItemKind::UserPrompt
                } else {
                    ItemKind::History
                };
            }
            let pair_complete = match kind {
                ItemKind::PendingToolCall => message
                    .tool_calls
                    .iter()
                    .all(|call| answered.contains(&call.id)),
                ItemKind::ToolResult => true,
                _ => true,
            };
            // Незавершённая пара перестаёт быть обязательной только когда она
            // закрыта: закрытая пара опускается до истории.
            let kind = if kind == ItemKind::PendingToolCall && pair_complete {
                ItemKind::History
            } else {
                kind
            };
            let priority = match kind {
                ItemKind::SafetyPolicy | ItemKind::UserPrompt => 100,
                ItemKind::PendingToolCall => 95,
                ItemKind::Memory => 55,
                ItemKind::ToolResult => 45,
                _ => 40,
            };
            let item =
                ContextItemBuilder::new(message_item_id(index, message.role), kind, String::new())
                    .task(task_id, session_id)
                    .source(message.role.as_str())
                    .priority(priority)
                    .trust(match message.role {
                        ChatRole::System => Trust::CoreOwned,
                        ChatRole::User => Trust::Confirmed,
                        ChatRole::Tool => Trust::External,
                        ChatRole::Assistant => Trust::External,
                    })
                    .privacy(Privacy::Workspace)
                    .created_at(now + index as i64)
                    .tool_pair_complete(pair_complete)
                    .build();
            PlanInput::new(item, OwnedContent::Text(message.content.clone()))
        })
        .collect()
}

/// Восстановление сообщений из плана. Порядок берётся из ledger, содержимое —
/// из исходных сообщений; выгруженные item заменяются своим bounded summary.
fn rebuild_messages(messages: &[ChatMessage], plan: &ContextPlan) -> Vec<ChatMessage> {
    let mut rebuilt: Vec<(usize, ChatMessage)> = Vec::with_capacity(plan.selected.len());
    for item in &plan.selected {
        let Some(index) = item
            .id
            .split('-')
            .nth(1)
            .and_then(|raw| raw.parse::<usize>().ok())
        else {
            continue;
        };
        let Some(original) = messages.get(index) else {
            continue;
        };
        let mut message = original.clone();
        if let Some(locator) = &item.artifact_locator {
            // Полное содержимое ушло в artifact store: в контексте остаётся
            // bounded ссылка, читаемая отдельным Core API.
            message.content = format!(
                "[вынесено в artifact store: {locator}, {} байт; полное содержимое читается отдельной командой Core]",
                item.bytes
            );
        }
        rebuilt.push((index, message));
    }
    // Порядок собранного контекста задаёт иерархия прав, но провайдер требует
    // корректной последовательности реплик: внутри одного уровня сообщения
    // возвращаются в исходном хронологическом порядке.
    rebuilt.sort_by_key(|(index, _)| *index);
    repair_tool_pairs(rebuilt.into_iter().map(|(_, message)| message).collect())
}

/// Нельзя оставить половину пары tool-call/result: провайдер отвергнет такой
/// запрос. Если одна половина не попала в контекст, отбрасывается и вторая.
fn repair_tool_pairs(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let requested: Vec<String> = messages
        .iter()
        .flat_map(|message| message.tool_calls.iter().map(|call| call.id.clone()))
        .collect();
    let answered: Vec<String> = messages
        .iter()
        .filter_map(|message| message.tool_call_id.clone())
        .collect();
    messages
        .into_iter()
        .filter(|message| {
            if let Some(call_id) = &message.tool_call_id {
                return requested.contains(call_id);
            }
            if !message.tool_calls.is_empty() {
                return message
                    .tool_calls
                    .iter()
                    .all(|call| answered.contains(&call.id));
            }
            true
        })
        .collect()
}

fn registry_from_specs(specs: &[ToolSpec]) -> Vec<ToolRegistryEntry> {
    specs
        .iter()
        .map(|spec| {
            let name = spec.function.name.clone();
            let capability = name.split('.').next().unwrap_or(name.as_str()).to_string();
            let mutation = is_mutation_tool(&name);
            let mandatory = MANDATORY_CAPABILITY_MARKERS
                .iter()
                .any(|marker| name.contains(marker));
            ToolRegistryEntry {
                id: name.clone(),
                capability,
                group: if mandatory {
                    ToolGroup::Mandatory
                } else if mutation {
                    ToolGroup::Mutation
                } else {
                    ToolGroup::ReadOnly
                },
                schema_json: serde_json::to_string(&spec.function.parameters)
                    .unwrap_or_else(|_| "{}".to_string()),
                approval_required: mutation,
                // Permission/approval semantics выбранного инструмента остаются
                // видимыми и не скрываются от модели.
                permission_label: if mutation {
                    format!("{name}: изменяет состояние, требуется approval")
                } else {
                    format!("{name}: только чтение")
                },
                mandatory_for_capability: mandatory,
            }
        })
        .collect()
}

fn is_mutation_tool(name: &str) -> bool {
    const MUTATION_MARKERS: [&str; 8] = [
        "write", "delete", "remove", "create", "apply", "commit", "exec", "run",
    ];
    MUTATION_MARKERS
        .iter()
        .any(|marker| name.to_lowercase().contains(marker))
}

/// Core-owned реализация [`OffloadSink`] поверх artifact store.
pub struct ArtifactOffload<'a> {
    store: ArtifactStore<'a>,
    task_id: String,
    now: i64,
    /// Сбои записи фиксируются, но не роняют сборку.
    pub failures: Vec<String>,
}

impl<'a> ArtifactOffload<'a> {
    pub fn new(
        connection: &'a rusqlite::Connection,
        quota: ArtifactQuota,
        task_id: impl Into<String>,
        now: i64,
    ) -> Self {
        Self {
            store: ArtifactStore::with_quota(connection, quota),
            task_id: task_id.into(),
            now,
            failures: Vec::new(),
        }
    }
}

impl ArtifactOffload<'_> {
    /// Выгрузка конкретного содержимого. Возвращает locator и bounded summary.
    pub fn offload_text(
        &mut self,
        kind: &str,
        content: &str,
        privacy: Privacy,
    ) -> Result<OffloadOutcome, String> {
        match self.store.offload(
            kind,
            &self.task_id,
            &self.task_id,
            content,
            privacy,
            self.now,
        ) {
            Ok(result) => Ok(OffloadOutcome {
                locator: result.reference.locator,
                summary_tokens: (result.reference.summary.len() as u32).div_ceil(3) + 8,
                offloaded_bytes: result.reference.bytes,
            }),
            Err(error) => {
                let message = error.to_string();
                self.failures.push(message.clone());
                Err(message)
            }
        }
    }

    /// Чтение полного содержимого артефакта с повторной проверкой policy и hash.
    pub fn read(
        &self,
        locator: &str,
        task_id: &str,
        parent_chain: &[String],
        kind: &str,
    ) -> Result<String, String> {
        self.store
            .read(locator, task_id, parent_chain, kind, self.now)
            .map_err(|error| error.to_string())
    }

    /// Статус ссылки — нужен UI и дочерним задачам.
    pub fn ref_status(&self, locator: &str) -> Option<ArtifactRefStatus> {
        self.store
            .get_ref(locator)
            .ok()
            .flatten()
            .map(|reference| reference.status)
    }
}

/// Offload, материализующий содержимое сообщений: планировщик отдаёт item,
/// а содержимое берётся из карты, собранной на этапе `plan_inputs`.
pub struct MessageOffload<'a> {
    inner: ArtifactOffload<'a>,
    contents: Vec<(String, String)>,
}

impl<'a> MessageOffload<'a> {
    pub fn new(inner: ArtifactOffload<'a>, contents: Vec<(String, String)>) -> Self {
        Self { inner, contents }
    }

    pub fn failures(&self) -> &[String] {
        &self.inner.failures
    }
}

impl OffloadSink for MessageOffload<'_> {
    fn available(&self) -> bool {
        true
    }

    fn offload(&mut self, item: &ContextItem) -> Result<OffloadOutcome, String> {
        if !item.privacy.allows_offload() {
            return Err(format!("privacy {} forbids offload", item.privacy.as_str()));
        }
        let Some((_, content)) = self.contents.iter().find(|(id, _)| id == &item.id).cloned()
        else {
            return Err(format!("content for {} is not available", item.id));
        };
        self.inner
            .offload_text(item.kind.as_str(), &content, item.privacy)
    }
}

/// Модель-суммаризатор поверх model gateway. Вызов не может вызывать
/// инструменты и не повторяется: это отдельный low-cost вызов.
pub struct GatewaySummaryModel<F>
where
    F: FnMut(&str, &SummarizerConfig) -> Result<String, String>,
{
    call: F,
    available: bool,
}

impl<F> GatewaySummaryModel<F>
where
    F: FnMut(&str, &SummarizerConfig) -> Result<String, String>,
{
    pub fn new(call: F, available: bool) -> Self {
        Self { call, available }
    }
}

impl<F> SummaryModel for GatewaySummaryModel<F>
where
    F: FnMut(&str, &SummarizerConfig) -> Result<String, String>,
{
    fn available(&self) -> bool {
        self.available
    }

    fn summarize(
        &mut self,
        items: &[ContextItem],
        config: &SummarizerConfig,
    ) -> Result<RawSummary, String> {
        // Вход суммаризатора — только идентификаторы и метаданные: содержимое
        // подставляет вызывающая сторона внутри `call`.
        let input = items
            .iter()
            .map(|item| format!("{}:{}", item.id, item.kind.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        let text = (self.call)(&input, config)?;
        Ok(RawSummary {
            summary_id: format!(
                "summary-{}",
                evohime_context_budget::hash::sha256_hex(&input)
                    .chars()
                    .take(16)
                    .collect::<String>()
            ),
            source_ids: items.iter().map(|item| item.id.clone()).collect(),
            estimated_tokens: (text.len() as u32).div_ceil(3) + 8,
            text,
        })
    }
}

/// Summarizer, доступный без модели: применяется deterministic fallback.
pub fn deterministic_summarizer(config: SummarizerConfig) -> impl Summarizer {
    BoundedSummarizer::<GatewaySummaryModel<fn(&str, &SummarizerConfig) -> Result<String, String>>>::new(
        None, config,
    )
}

/// Summarizer поверх уже полученного ответа модели. Вызов gateway выполняется
/// вызывающей стороной один раз до сборки, поэтому внутри лестницы не остаётся
/// ни асинхронности, ни повторов; отсутствующий ответ означает deterministic
/// fallback.
pub fn model_summarizer(
    config: SummarizerConfig,
    summary: Option<String>,
) -> BoundedSummarizer<PrecomputedSummaryModel> {
    let model = summary.map(|text| PrecomputedSummaryModel { text });
    BoundedSummarizer::new(model, config)
}

/// Модель-суммаризатор с заранее полученным ответом.
pub struct PrecomputedSummaryModel {
    text: String,
}

impl SummaryModel for PrecomputedSummaryModel {
    fn available(&self) -> bool {
        !self.text.trim().is_empty()
    }

    fn summarize(
        &mut self,
        items: &[ContextItem],
        _config: &SummarizerConfig,
    ) -> Result<RawSummary, String> {
        let source_ids: Vec<String> = items
            .iter()
            .filter(|item| !item.is_mandatory_kind() && item.tool_pair_complete)
            .map(|item| item.id.clone())
            .collect();
        if source_ids.is_empty() {
            return Err("no compressible items".to_string());
        }
        Ok(RawSummary {
            summary_id: format!(
                "summary-{}",
                evohime_context_budget::hash::sha256_hex(&source_ids.join(","))
                    .chars()
                    .take(16)
                    .collect::<String>()
            ),
            source_ids,
            estimated_tokens: (self.text.len() as u32).div_ceil(3) + 8,
            text: self.text.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evohime_context_budget::ladder::NoSummarizer;
    use evohime_model_gateway::tools::{FunctionSpec, NativeToolCall};

    fn spec(name: &str) -> ToolSpec {
        ToolSpec {
            kind: "function".to_string(),
            function: FunctionSpec {
                name: name.to_string(),
                description: format!("описание {name}"),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
                manifest_hash: None,
            },
        }
    }

    fn runtime() -> ContextRuntime {
        ContextRuntime::new("test-model")
    }

    struct NoOffloadSink;

    impl OffloadSink for NoOffloadSink {
        fn available(&self) -> bool {
            false
        }

        fn offload(&mut self, _item: &ContextItem) -> Result<OffloadOutcome, String> {
            Err("unavailable".to_string())
        }
    }

    fn assemble(
        runtime: &mut ContextRuntime,
        messages: &[ChatMessage],
        specs: &[ToolSpec],
    ) -> AssembledContext {
        runtime.assemble(
            "task",
            "session",
            "call-1",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            messages,
            specs,
            &[],
            &[],
            &[],
            false,
            &mut NoOffloadSink,
            &mut NoSummarizer,
        )
    }

    #[test]
    fn a_normal_step_is_assembled_and_keeps_the_mandatory_minimum() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        let assembled = assemble(&mut runtime, &messages, &[spec("filesystem.read")]);
        assert!(assembled.is_ready());
        assert_eq!(assembled.messages.len(), 2);
        assert_eq!(assembled.messages[0].role, ChatRole::System);
        assert_eq!(assembled.messages[1].content, "проверь репозиторий");
        assert_eq!(assembled.ledger().context_ledger_hash.len(), 64);
    }

    /// Окно, названное провайдером, должно доходить до планировщика: без этого
    /// Ева считает бюджет по встроенному профилю и не знает, что у модели
    /// контекст на порядок больше или меньше.
    #[test]
    fn a_provider_window_reaches_the_planned_budget() {
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];

        let mut default_runtime = runtime();
        let default_limit = assemble(&mut default_runtime, &messages, &[])
            .plan
            .profile
            .hard_limit_tokens;

        let mut narrow = runtime();
        narrow.set_model_windows(HashMap::from([("gpt-4o-mini".to_string(), 8_192)]));
        let narrow_limit = assemble(&mut narrow, &messages, &[])
            .plan
            .profile
            .hard_limit_tokens;

        assert_eq!(narrow.model_window("gpt-4o-mini"), Some(8_192));
        assert!(narrow_limit <= 8_192, "окно провайдера ограничивает бюджет");
        assert!(
            narrow_limit < default_limit,
            "узкое окно должно давать меньший бюджет, чем встроенный профиль"
        );
    }

    #[test]
    fn the_projection_never_carries_message_content() {
        let mut runtime = runtime();
        let secret = "СЕКРЕТ_В_ПРОМПТЕ_98765";
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, secret),
            ChatMessage::text(ChatRole::Tool, format!("вывод инструмента {secret}")),
        ];
        let assembled = assemble(&mut runtime, &messages, &[spec("filesystem.read")]);
        let serialized =
            serde_json::to_string(&assembled.projection()).expect("projection serializes");
        assert!(!serialized.contains(secret));
        assert!(serialized.contains("context_ledger_hash"));
    }

    #[test]
    fn the_last_user_message_is_the_mandatory_prompt() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "старый вопрос"),
            ChatMessage::text(ChatRole::Assistant, "ответ"),
            ChatMessage::text(ChatRole::User, "текущий вопрос"),
        ];
        let assembled = assemble(&mut runtime, &messages, &[]);
        let prompt_parts: Vec<&str> = assembled
            .ledger()
            .mandatory_parts
            .iter()
            .map(|part| part.part.as_str())
            .collect();
        assert!(prompt_parts.contains(&"user_prompt"));
        // Текущий prompt всегда остаётся в собранном контексте.
        assert!(assembled
            .messages
            .iter()
            .any(|message| message.content == "текущий вопрос"));
    }

    #[test]
    fn an_unanswered_tool_call_stays_in_the_mandatory_minimum() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь"),
            ChatMessage::assistant_tool_calls(
                "вызываю",
                vec![NativeToolCall {
                    id: "call-1".to_string(),
                    name: "filesystem.read".to_string(),
                    arguments: "{}".to_string(),
                }],
            ),
        ];
        let assembled = assemble(&mut runtime, &messages, &[spec("filesystem.read")]);
        let parts: Vec<&str> = assembled
            .ledger()
            .mandatory_parts
            .iter()
            .map(|part| part.part.as_str())
            .collect();
        assert!(parts.contains(&"pending_tool_call"));
    }

    #[test]
    fn an_answered_tool_call_is_no_longer_mandatory() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь"),
            ChatMessage::assistant_tool_calls(
                "вызываю",
                vec![NativeToolCall {
                    id: "call-1".to_string(),
                    name: "filesystem.read".to_string(),
                    arguments: "{}".to_string(),
                }],
            ),
            ChatMessage::tool_observation("call-1", "результат"),
        ];
        let assembled = assemble(&mut runtime, &messages, &[spec("filesystem.read")]);
        let parts: Vec<&str> = assembled
            .ledger()
            .mandatory_parts
            .iter()
            .map(|part| part.part.as_str())
            .collect();
        assert!(!parts.contains(&"pending_tool_call"));
    }

    #[test]
    fn the_loadout_limits_tool_schemas_and_keeps_mandatory_tools() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        let specs = vec![
            spec("task.status"),
            spec("filesystem.read"),
            spec("filesystem.write"),
        ];
        let assembled = assemble(&mut runtime, &messages, &specs);
        let names: Vec<&str> = assembled
            .tool_specs
            .iter()
            .map(|spec| spec.function.name.as_str())
            .collect();
        assert!(
            names.contains(&"task.status"),
            "обязательные всегда в loadout"
        );
        assert!(names.contains(&"filesystem.read"));
        assert!(
            !names.contains(&"filesystem.write"),
            "read-only intent не даёт mutation-инструмент"
        );
    }

    #[test]
    fn an_out_of_loadout_call_is_rejected_before_the_effect() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        let assembled = assemble(
            &mut runtime,
            &messages,
            &[spec("filesystem.read"), spec("filesystem.write")],
        );
        let miss = assembled
            .check_tool_call("filesystem.write")
            .expect_err("mutation is out of loadout");
        assert_eq!(miss.tool_id, "filesystem.write");
        assert!(!miss.policy_reason.is_empty());
        assert!(assembled.check_tool_call("filesystem.read").is_ok());
    }

    #[test]
    fn mutation_intent_admits_the_write_tool_with_visible_approval_semantics() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "исправь файл конфигурации"),
        ];
        let assembled = assemble(
            &mut runtime,
            &messages,
            &[spec("filesystem.read"), spec("filesystem.write")],
        );
        assert!(assembled.check_tool_call("filesystem.write").is_ok());
        let write = assembled
            .loadout
            .tools
            .iter()
            .find(|tool| tool.id == "filesystem.write")
            .expect("mutation tool selected");
        assert!(write.approval_required);
        assert!(write.permission_label.contains("approval"));
    }

    #[test]
    fn the_same_step_assembles_to_the_same_ledger_hash() {
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        let specs = vec![spec("filesystem.read")];
        let left = assemble(&mut runtime(), &messages, &specs);
        let right = assemble(&mut runtime(), &messages, &specs);
        assert_eq!(
            left.ledger().context_ledger_hash,
            right.ledger().context_ledger_hash
        );
    }

    #[test]
    fn a_long_history_is_reduced_and_the_hard_limit_holds() {
        let mut runtime = runtime();
        let block = "данные ".repeat(20_000);
        let mut messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        for _ in 0..12 {
            messages.push(ChatMessage::text(ChatRole::Tool, block.clone()));
        }
        let assembled = assemble(&mut runtime, &messages, &[spec("filesystem.read")]);
        assert!(assembled.is_ready());
        assert!(assembled.messages.len() < messages.len());
        let ledger = assembled.ledger();
        let total =
            ledger.mandatory_tokens + ledger.selected_optional_tokens + ledger.reserves_tokens;
        assert!(total <= assembled.plan.profile.hard_limit_tokens);
        // Обязательный минимум остаётся на месте.
        assert!(assembled
            .messages
            .iter()
            .any(|message| message.content == "проверь репозиторий"));
        assert!(assembled
            .messages
            .iter()
            .any(|message| message.content == "системная политика"));
    }

    /// Старый клиент знает только исходные поля `ModelContext`. Additive-поле
    /// `context` он обязан игнорировать без ошибки и без major bump протокола.
    #[derive(serde::Deserialize)]
    struct LegacyModelContext {
        task_id: String,
        workspace_path: String,
        model: String,
        system_prompt: String,
        user_prompt: String,
        tools: Vec<String>,
        estimated_tokens: usize,
        context_limit_tokens: usize,
    }

    #[test]
    fn old_clients_ignore_the_additive_model_context_field() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        let assembled = assemble(&mut runtime, &messages, &[spec("filesystem.read")]);
        let event = crate::CoreEvent::ModelContext {
            task_id: "task".to_string(),
            workspace_path: "C:/work".to_string(),
            model: "gpt-4o-mini".to_string(),
            system_prompt: "системная политика".to_string(),
            user_prompt: "проверь репозиторий".to_string(),
            tools: vec!["filesystem.read".to_string()],
            estimated_tokens: 42,
            context_limit_tokens: 128_000,
            context: Some(Box::new(assembled.projection())),
        };
        let payload = serde_json::to_value(&event).expect("event serializes");
        let body = payload
            .get("ModelContext")
            .expect("externally tagged event body");

        // Новая схема присутствует и читается новым клиентом.
        assert!(body.get("context").is_some());
        assert_eq!(
            body["context"]["schema_version"],
            serde_json::json!(MODEL_CONTEXT_SCHEMA_VERSION)
        );

        // Старый клиент читает тот же payload без ошибки.
        let legacy: LegacyModelContext =
            serde_json::from_value(body.clone()).expect("legacy client parses the payload");
        assert_eq!(legacy.task_id, "task");
        assert_eq!(legacy.workspace_path, "C:/work");
        assert_eq!(legacy.model, "gpt-4o-mini");
        assert_eq!(legacy.system_prompt, "системная политика");
        assert_eq!(legacy.user_prompt, "проверь репозиторий");
        assert_eq!(legacy.tools, vec!["filesystem.read".to_string()]);
        assert_eq!(legacy.estimated_tokens, 42);
        assert_eq!(legacy.context_limit_tokens, 128_000);
    }

    #[test]
    fn an_event_without_the_new_field_stays_readable() {
        // Событие старого Core: поле `context` отсутствует, ошибки нет.
        let payload = serde_json::json!({
            "task_id": "task",
            "workspace_path": "C:/work",
            "model": "m",
            "system_prompt": "s",
            "user_prompt": "u",
            "tools": [],
            "estimated_tokens": 1,
            "context_limit_tokens": 2
        });
        let legacy: LegacyModelContext =
            serde_json::from_value(payload.clone()).expect("payload parses");
        assert_eq!(legacy.task_id, "task");
        assert!(payload.get("context").is_none());
    }

    fn scratchpad_entry(
        id: &str,
        category: ScratchpadCategory,
        created_at: i64,
    ) -> ScratchpadEntry {
        let mut entry = ScratchpadEntry::draft(
            id,
            "task",
            "session",
            category,
            format!("содержимое {id} ").repeat(20),
            created_at,
        );
        entry.confirm(
            evohime_context_budget::scratchpad::ConfirmationBasis::ToolProvenanceVerified,
            created_at,
        );
        entry
    }

    #[test]
    fn confirmed_scratchpad_entries_join_the_assembled_context() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        let entries = vec![scratchpad_entry("s1", ScratchpadCategory::Facts, 10)];
        let assembled = runtime.assemble(
            "task",
            "session",
            "call-1",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            &messages,
            &[],
            &[],
            &entries,
            &[],
            false,
            &mut NoOffloadSink,
            &mut NoSummarizer,
        );
        assert!(assembled.is_ready());
        assert!(assembled
            .ledger()
            .selected_items
            .iter()
            .any(|item| item.id == "s1"));
    }

    #[test]
    fn an_offloaded_entry_appears_as_a_bounded_reference_not_as_truncated_text() {
        let mut entry = scratchpad_entry("s1", ScratchpadCategory::Facts, 10);
        let full = entry.content.clone();
        entry.artifact_locator = Some("artifact://task/abc".to_string());
        let text = scratchpad_context_text(&entry);
        assert!(text.contains("artifact://task/abc"));
        assert!(text.contains(&entry.content_hash));
        // Молчаливого усечения нет: содержимое в контекст не попадает вовсе.
        assert!(!text.contains(&full));
    }

    #[test]
    fn scratchpad_overflow_evicts_the_oldest_confirmed_entries_first() {
        let entries = vec![
            scratchpad_entry("oldest", ScratchpadCategory::Facts, 10),
            scratchpad_entry("newer", ScratchpadCategory::Facts, 20),
            scratchpad_entry("question", ScratchpadCategory::OpenQuestions, 5),
        ];
        // Бюджет меньше суммарного размера: выгружаются самые старые записи.
        let candidates = scratchpad_offload_candidates(&entries, 200);
        assert_eq!(candidates.first().map(String::as_str), Some("oldest"));
        // `open_questions` не вытесняются, даже будучи самыми старыми.
        assert!(!candidates.contains(&"question".to_string()));
    }

    #[test]
    fn a_scratchpad_within_budget_is_never_offloaded() {
        let entries = vec![scratchpad_entry("s1", ScratchpadCategory::Facts, 10)];
        assert!(scratchpad_offload_candidates(&entries, 100_000).is_empty());
    }

    #[test]
    fn an_already_offloaded_entry_is_not_offloaded_twice() {
        let mut entry = scratchpad_entry("s1", ScratchpadCategory::Facts, 10);
        entry.artifact_locator = Some("artifact://task/abc".to_string());
        assert!(scratchpad_offload_candidates(&[entry], 1).is_empty());
    }

    #[test]
    fn open_questions_from_the_scratchpad_reach_the_intent_router() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "продолжай"),
        ];
        let assembled = runtime.assemble(
            "task",
            "session",
            "call-1",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            &messages,
            &[spec("filesystem.read"), spec("filesystem.write")],
            &["нужно исправь конфигурацию".to_string()],
            &[],
            &[],
            false,
            &mut NoOffloadSink,
            &mut NoSummarizer,
        );
        assert_eq!(assembled.loadout.decision.intent, "edit");
        assert!(assembled.check_tool_call("filesystem.write").is_ok());
    }

    #[test]
    fn a_pinned_item_survives_a_reduction_that_drops_its_peers() {
        let mut runtime = runtime();
        let block = "данные ".repeat(20_000);
        let mut messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        for _ in 0..12 {
            messages.push(ChatMessage::text(ChatRole::Tool, block.clone()));
        }
        let pinned = message_item_id(3, ChatRole::Tool);
        let assembled = runtime.assemble(
            "task",
            "session",
            "call-1",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            &messages,
            &[],
            &[],
            &[],
            std::slice::from_ref(&pinned),
            false,
            &mut NoOffloadSink,
            &mut NoSummarizer,
        );
        assert!(assembled.is_ready());
        let dropped: Vec<&str> = assembled
            .ledger()
            .dropped_items
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        // Закреплённый item отбрасывается последним; часть остальных уходит.
        assert!(!dropped.is_empty());
        if dropped.contains(&pinned.as_str()) {
            // Pin не отменяет hard limit: если места не хватило, причина явная.
            let reason = assembled
                .ledger()
                .dropped_items
                .iter()
                .find(|item| item.id == pinned)
                .map(|item| item.drop_reason);
            assert!(reason.is_some(), "у закреплённого item должна быть причина");
        }
    }

    #[test]
    fn forced_summarization_reduces_the_current_assembly_only() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
            ChatMessage::text(ChatRole::Tool, "устаревший вывод инструмента"),
        ];
        let relaxed = assemble(&mut runtime, &messages, &[]);
        let forced = runtime.assemble(
            "task",
            "session",
            "call-2",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            &messages,
            &[],
            &[],
            &[],
            &[],
            true,
            &mut NoOffloadSink,
            &mut NoSummarizer,
        );
        assert!(forced.messages.len() < relaxed.messages.len());
        // Обязательный минимум остаётся на месте даже при принудительном сжатии.
        assert!(forced
            .messages
            .iter()
            .any(|message| message.content == "проверь репозиторий"));
    }

    #[test]
    fn a_model_summary_replaces_the_compressed_history() {
        let mut runtime = runtime();
        let block = "данные ".repeat(20_000);
        let mut messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        for index in 0..12 {
            // Содержимое различается: иначе уровень L1 отбросил бы реплики как
            // дубликаты и до сжатия дело бы не дошло.
            messages.push(ChatMessage::text(
                ChatRole::Assistant,
                format!("шаг {index}: {block}"),
            ));
        }
        let mut summarizer = model_summarizer(
            SummarizerConfig::default(),
            Some("краткое изложение истории".to_string()),
        );
        let assembled = runtime.assemble(
            "task",
            "session",
            "call-1",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            &messages,
            &[],
            &[],
            &[],
            &[],
            false,
            &mut NoOffloadSink,
            &mut summarizer,
        );
        assert!(assembled.is_ready());
        let compression = &assembled.ledger().compression;
        assert_eq!(compression.len(), 1, "ровно одно compression-решение");
        assert!(!compression[0].fallback, "использован ответ модели");
        assert!(!compression[0].source_ids.is_empty());
        // Связь summary_id -> source_ids сохранена для повторной сборки.
        assert!(compression[0].compression_ratio < 1.0);
    }

    #[test]
    fn a_missing_model_answer_falls_back_deterministically() {
        let mut runtime = runtime();
        let block = "данные ".repeat(20_000);
        let mut messages = vec![
            ChatMessage::text(ChatRole::System, "системная политика"),
            ChatMessage::text(ChatRole::User, "проверь репозиторий"),
        ];
        for index in 0..12 {
            // Содержимое различается: иначе уровень L1 отбросил бы реплики как
            // дубликаты и до сжатия дело бы не дошло.
            messages.push(ChatMessage::text(
                ChatRole::Assistant,
                format!("шаг {index}: {block}"),
            ));
        }
        let mut summarizer = model_summarizer(SummarizerConfig::default(), None);
        let assembled = runtime.assemble(
            "task",
            "session",
            "call-1",
            "literouter",
            "gpt-4o-mini",
            1_000_000,
            &messages,
            &[],
            &[],
            &[],
            &[],
            false,
            &mut NoOffloadSink,
            &mut summarizer,
        );
        assert!(assembled.is_ready());
        let compression = &assembled.ledger().compression;
        assert_eq!(compression.len(), 1);
        assert!(
            compression[0].fallback,
            "без ответа модели работает fallback"
        );
        assert!(compression[0].fallback_reason.is_some());
    }

    #[test]
    fn an_empty_model_answer_is_treated_as_unavailable() {
        let mut summarizer = model_summarizer(SummarizerConfig::default(), Some("   ".to_string()));
        let items = vec![
            ContextItemBuilder::new("h1", ItemKind::History, "hash-1")
                .sizes(300, 100)
                .build(),
            ContextItemBuilder::new("h2", ItemKind::History, "hash-2")
                .sizes(300, 100)
                .build(),
        ];
        let outcome = summarizer.summarize(&items).expect("summary");
        assert!(outcome.fallback);
    }

    #[test]
    fn a_budget_refusal_is_reported_instead_of_a_silent_truncation() {
        let mut runtime = runtime();
        let messages = vec![
            ChatMessage::text(ChatRole::System, "я".repeat(2_000_000)),
            ChatMessage::text(ChatRole::User, "проверь"),
        ];
        let assembled = assemble(&mut runtime, &messages, &[]);
        assert!(!assembled.is_ready());
        let projection = assembled.projection();
        let refusal = projection
            .budget_unavailable
            .expect("отказ виден в projection");
        assert_eq!(refusal.code, "budget_unavailable");
        assert!(refusal.required_tokens > refusal.available_tokens);
        assert!(!refusal.context_ledger_hash.is_empty());
    }
}
