//! Tool loadout и deterministic intent router (этап 01.4).
//!
//! Registry остаётся в Core: наружу уходит сам loadout в model call, а в ledger —
//! loadout id, intent, версия таблицы правил, matched rule и bounded diagnostic
//! `loadout_miss`.

use serde::{Deserialize, Serialize};

use crate::ledger::LoadoutRecord;

/// Версия таблицы правил intent router.
pub const INTENT_RULES_VERSION: &str = "intent-rules-1";

/// Группа инструмента.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolGroup {
    /// Всегда входит в loadout и имеет отдельный `mandatory_schema_reserve`.
    Mandatory,
    /// Не меняет состояние.
    ReadOnly,
    /// Меняет состояние: требует approval по policy.
    Mutation,
}

impl ToolGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mandatory => "mandatory",
            Self::ReadOnly => "read_only",
            Self::Mutation => "mutation",
        }
    }
}

/// Запись registry. Permission/approval semantics выбранного инструмента
/// остаются видимыми и никогда не скрываются.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRegistryEntry {
    pub id: String,
    /// Capability, к которой относится инструмент.
    pub capability: String,
    pub group: ToolGroup,
    /// JSON-схема инструмента.
    pub schema_json: String,
    /// Требуется ли approval перед эффектом.
    pub approval_required: bool,
    /// Ярлык прав, показываемый вместе со схемой.
    pub permission_label: String,
    /// Обязателен ли инструмент для cancellation/status и policy/approval
    /// semantics своей capability. Конкретные имена не зашиваются в router.
    #[serde(default)]
    pub mandatory_for_capability: bool,
}

/// Правило intent router.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentRule {
    pub id: String,
    pub intent: String,
    /// Ключевые слова capability в нижнем регистре.
    pub keywords: Vec<String>,
    /// Разрешает ли intent mutation-инструменты.
    pub allows_mutation: bool,
    /// Capabilities, инструменты которых входят в loadout этого intent.
    pub capabilities: Vec<String>,
}

/// Deny/approval-правило: запрещает конкретные capability независимо от intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DenyRule {
    pub id: String,
    pub capability: String,
    pub reason: String,
}

/// Versioned таблица правил.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentRules {
    pub version: String,
    pub rules: Vec<IntentRule>,
    #[serde(default)]
    pub deny: Vec<DenyRule>,
}

impl Default for IntentRules {
    fn default() -> Self {
        Self {
            version: INTENT_RULES_VERSION.to_string(),
            rules: Vec::new(),
            deny: Vec::new(),
        }
    }
}

/// Результат работы router.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentDecision {
    pub intent: String,
    /// 0.0..1.0. Ноль означает неопределённый intent.
    pub confidence: f64,
    pub matched_rules: Vec<String>,
    pub rules_version: String,
    pub allows_mutation: bool,
    /// Использован ли безопасный read-only fallback.
    pub fallback: bool,
}

/// Intent, используемый при неопределённом результате.
pub const FALLBACK_INTENT: &str = "read_only_fallback";

/// Нормализация текста запроса: нижний регистр и схлопывание пробелов.
fn normalize_query(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Deterministic intent router: нормализует prompt и активные `open_questions`,
/// сопоставляет их с versioned таблицей capability keywords и применяет
/// deny/approval rules. При конфликте правил выбирается более безопасный
/// read-only результат.
pub fn route_intent(
    rules: &IntentRules,
    prompt: &str,
    open_questions: &[String],
) -> IntentDecision {
    let mut haystack = normalize_query(prompt);
    for question in open_questions {
        haystack.push(' ');
        haystack.push_str(&normalize_query(question));
    }

    let mut matches: Vec<(&IntentRule, usize)> = Vec::new();
    for rule in &rules.rules {
        let hits = rule
            .keywords
            .iter()
            .filter(|keyword| haystack.contains(keyword.as_str()))
            .count();
        if hits > 0 {
            matches.push((rule, hits));
        }
    }

    if matches.is_empty() {
        return IntentDecision {
            intent: FALLBACK_INTENT.to_string(),
            confidence: 0.0,
            matched_rules: Vec::new(),
            rules_version: rules.version.clone(),
            allows_mutation: false,
            fallback: true,
        };
    }

    // Детерминированный порядок: больше совпадений, затем read-only впереди
    // mutation, затем id лексикографически.
    // Смешанные задания часто содержат и исследование, и явное изменение
    // (например, «изучи проект, затем создай файл»). Если у mutation-правила
    // есть собственное action-слово, оно должно победить read-only этап, иначе
    // модель увидит только inspect-loadout и будет вынуждена вызывать
    // отсутствующие filesystem.write/shell.execute.
    let non_mutation_keywords: std::collections::HashSet<&str> = rules
        .rules
        .iter()
        .filter(|rule| !rule.allows_mutation)
        .flat_map(|rule| rule.keywords.iter().map(String::as_str))
        .collect();
    let explicit_mutation = matches.iter().any(|(rule, _)| {
        rule.allows_mutation
            && rule.keywords.iter().any(|keyword| {
                !non_mutation_keywords.contains(keyword.as_str())
                    && haystack.contains(keyword.as_str())
            })
    });

    matches.sort_by(|left, right| {
        let mutation_priority = |rule: &IntentRule| explicit_mutation && rule.allows_mutation;
        right
            .1
            .cmp(&left.1)
            .then_with(|| mutation_priority(right.0).cmp(&mutation_priority(left.0)))
            .then_with(|| left.0.allows_mutation.cmp(&right.0.allows_mutation))
            .then_with(|| left.0.id.cmp(&right.0.id))
    });

    let best_hits = matches[0].1;
    let top: Vec<&IntentRule> = matches
        .iter()
        .filter(|(_, hits)| *hits == best_hits)
        .map(|(rule, _)| *rule)
        .collect();
    // При конфликте правил (несколько разных intent с одинаковым весом)
    // выбирается более безопасный read-only результат.
    let conflicting = top.iter().any(|rule| rule.intent != top[0].intent);
    let chosen = if conflicting {
        if explicit_mutation {
            top.iter()
                .find(|rule| rule.allows_mutation)
                .copied()
                .unwrap_or(top[0])
        } else {
            top.iter()
                .find(|rule| !rule.allows_mutation)
                .copied()
                .unwrap_or(top[0])
        }
    } else {
        top[0]
    };

    let total_keywords = chosen.keywords.len().max(1);
    let confidence = (best_hits as f64 / total_keywords as f64).min(1.0);

    IntentDecision {
        intent: chosen.intent.clone(),
        confidence,
        matched_rules: top.iter().map(|rule| rule.id.clone()).collect(),
        rules_version: rules.version.clone(),
        allows_mutation: chosen.allows_mutation,
        fallback: false,
    }
}

/// Собранный loadout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolLoadout {
    pub loadout_id: String,
    pub decision: IntentDecision,
    /// Инструменты в детерминированном порядке: обязательные, затем остальные.
    pub tools: Vec<ToolRegistryEntry>,
    pub schema_tokens: u32,
    /// Инструменты, не поместившиеся в `tool_schema_reserve`.
    pub omitted_tool_ids: Vec<String>,
    /// Инструменты, отклонённые deny-правилами.
    pub denied_tool_ids: Vec<String>,
}

impl ToolLoadout {
    /// Разрешён ли вызов инструмента в этом loadout.
    pub fn allows(&self, tool_id: &str) -> bool {
        self.tools.iter().any(|tool| tool.id == tool_id)
    }

    /// Проекция для ledger.
    pub fn to_record(&self) -> LoadoutRecord {
        LoadoutRecord {
            loadout_id: self.loadout_id.clone(),
            intent: self.decision.intent.clone(),
            rules_version: self.decision.rules_version.clone(),
            matched_rule: self.decision.matched_rules.first().cloned(),
            tool_ids: self.tools.iter().map(|tool| tool.id.clone()).collect(),
            schema_tokens: self.schema_tokens,
            fallback: self.decision.fallback,
        }
    }
}

/// Bounded diagnostic отклонённого вызова. Содержит только tool id, intent,
/// loadout id, matched rule и policy reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadoutMiss {
    pub tool_id: String,
    pub intent: String,
    pub loadout_id: String,
    pub matched_rule: Option<String>,
    pub policy_reason: String,
}

/// Лимиты сборки loadout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadoutLimits {
    /// Лимит на схемы необязательных инструментов.
    pub tool_schema_reserve: u32,
    /// Отдельный резерв под обязательные инструменты.
    pub mandatory_schema_reserve: u32,
}

/// Сборка loadout. Обязательные инструменты входят всегда и расходуют
/// собственный резерв; остальные добавляются, пока помещаются в
/// `tool_schema_reserve`.
pub fn build_loadout(
    registry: &[ToolRegistryEntry],
    rules: &IntentRules,
    decision: IntentDecision,
    limits: LoadoutLimits,
    estimate_schema: &dyn Fn(&str) -> u32,
) -> ToolLoadout {
    let denied: Vec<String> = registry
        .iter()
        .filter(|tool| {
            rules
                .deny
                .iter()
                .any(|deny| deny.capability == tool.capability)
        })
        .map(|tool| tool.id.clone())
        .collect();

    let mut mandatory: Vec<&ToolRegistryEntry> = registry
        .iter()
        .filter(|tool| {
            (tool.group == ToolGroup::Mandatory || tool.mandatory_for_capability)
                && !denied.contains(&tool.id)
        })
        .collect();
    mandatory.sort_by(|left, right| left.id.cmp(&right.id));

    // Capabilities выбранного intent. Пустой список означает «без ограничения
    // по capability»; при fallback ограничение не применяется, потому что
    // безопасный read-only набор собирается из всего registry.
    let intent_capabilities: Option<&Vec<String>> = if decision.fallback {
        None
    } else {
        rules
            .rules
            .iter()
            .find(|rule| rule.intent == decision.intent)
            .map(|rule| &rule.capabilities)
            .filter(|capabilities| !capabilities.is_empty())
    };

    let mut optional: Vec<&ToolRegistryEntry> = registry
        .iter()
        .filter(|tool| {
            tool.group != ToolGroup::Mandatory
                && !tool.mandatory_for_capability
                && !denied.contains(&tool.id)
                && (decision.allows_mutation || tool.group == ToolGroup::ReadOnly)
                && match intent_capabilities {
                    Some(capabilities) => capabilities.contains(&tool.capability),
                    None => true,
                }
        })
        .collect();
    // Детерминированный порядок: read-only впереди mutation, затем id.
    optional.sort_by(|left, right| {
        left.group
            .cmp(&right.group)
            .then_with(|| left.id.cmp(&right.id))
    });

    let mut tools: Vec<ToolRegistryEntry> = Vec::new();
    let mut omitted: Vec<String> = Vec::new();
    let mut mandatory_tokens = 0_u32;
    for tool in mandatory {
        // Обязательный инструмент никогда не выбрасывается: он расходует
        // собственный `mandatory_schema_reserve`, а факт перерасхода виден по
        // `schema_tokens` записи ledger.
        mandatory_tokens = mandatory_tokens.saturating_add(estimate_schema(&tool.schema_json));
        tools.push(tool.clone());
    }

    let mut optional_tokens = 0_u32;
    for tool in optional {
        let cost = estimate_schema(&tool.schema_json);
        if optional_tokens.saturating_add(cost) > limits.tool_schema_reserve {
            omitted.push(tool.id.clone());
            continue;
        }
        optional_tokens = optional_tokens.saturating_add(cost);
        tools.push(tool.clone());
    }

    let loadout_id = format!(
        "loadout-{}",
        crate::hash::sha256_hex(&format!(
            "{}|{}|{}",
            decision.rules_version,
            decision.intent,
            tools
                .iter()
                .map(|tool| tool.id.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ))
        .chars()
        .take(16)
        .collect::<String>()
    );

    ToolLoadout {
        loadout_id,
        decision,
        tools,
        schema_tokens: mandatory_tokens.saturating_add(optional_tokens),
        omitted_tool_ids: omitted,
        denied_tool_ids: denied,
    }
}

/// Проверка вызова инструмента до эффекта. Возвращает `Err(LoadoutMiss)`, если
/// инструмент вне loadout: Core отклоняет такой вызов до эффекта.
pub fn check_tool_call(loadout: &ToolLoadout, tool_id: &str) -> Result<(), LoadoutMiss> {
    if loadout.allows(tool_id) {
        return Ok(());
    }
    let policy_reason = if loadout.denied_tool_ids.iter().any(|id| id == tool_id) {
        "denied by policy rule"
    } else if loadout.omitted_tool_ids.iter().any(|id| id == tool_id) {
        "omitted: tool_schema_reserve exhausted"
    } else {
        "not selected for the current intent"
    };
    Err(LoadoutMiss {
        tool_id: tool_id.to_string(),
        intent: loadout.decision.intent.clone(),
        loadout_id: loadout.loadout_id.clone(),
        matched_rule: loadout.decision.matched_rules.first().cloned(),
        policy_reason: policy_reason.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> IntentRules {
        IntentRules {
            version: INTENT_RULES_VERSION.to_string(),
            rules: vec![
                IntentRule {
                    id: "inspect".to_string(),
                    intent: "inspect".to_string(),
                    keywords: vec!["проверь".to_string(), "покажи".to_string()],
                    allows_mutation: false,
                    capabilities: vec!["filesystem".to_string()],
                },
                IntentRule {
                    id: "edit".to_string(),
                    intent: "edit".to_string(),
                    keywords: vec![
                        "измени".to_string(),
                        "создай".to_string(),
                        "проверь".to_string(),
                    ],
                    allows_mutation: true,
                    capabilities: vec!["filesystem".to_string()],
                },
            ],
            deny: vec![DenyRule {
                id: "no-network".to_string(),
                capability: "network".to_string(),
                reason: "network capability is disabled".to_string(),
            }],
        }
    }

    fn registry() -> Vec<ToolRegistryEntry> {
        vec![
            ToolRegistryEntry {
                id: "task.status".to_string(),
                capability: "lifecycle".to_string(),
                group: ToolGroup::Mandatory,
                schema_json: "{\"name\":\"task.status\"}".to_string(),
                approval_required: false,
                permission_label: "read-only".to_string(),
                mandatory_for_capability: true,
            },
            ToolRegistryEntry {
                id: "fs.read".to_string(),
                capability: "filesystem".to_string(),
                group: ToolGroup::ReadOnly,
                schema_json: "{\"name\":\"fs.read\"}".to_string(),
                approval_required: false,
                permission_label: "read workspace".to_string(),
                mandatory_for_capability: false,
            },
            ToolRegistryEntry {
                id: "fs.write".to_string(),
                capability: "filesystem".to_string(),
                group: ToolGroup::Mutation,
                schema_json: "{\"name\":\"fs.write\"}".to_string(),
                approval_required: true,
                permission_label: "write workspace (approval)".to_string(),
                mandatory_for_capability: false,
            },
            ToolRegistryEntry {
                id: "net.fetch".to_string(),
                capability: "network".to_string(),
                group: ToolGroup::ReadOnly,
                schema_json: "{\"name\":\"net.fetch\"}".to_string(),
                approval_required: true,
                permission_label: "network (approval)".to_string(),
                mandatory_for_capability: false,
            },
        ]
    }

    fn limits() -> LoadoutLimits {
        LoadoutLimits {
            tool_schema_reserve: 1_000,
            mandatory_schema_reserve: 1_000,
        }
    }

    fn estimator() -> impl Fn(&str) -> u32 {
        |schema: &str| schema.len() as u32
    }

    #[test]
    fn undefined_intent_yields_a_safe_read_only_fallback() {
        let decision = route_intent(&rules(), "совершенно посторонний текст", &[]);
        assert!(decision.fallback);
        assert_eq!(decision.intent, FALLBACK_INTENT);
        assert!(!decision.allows_mutation);

        let loadout = build_loadout(&registry(), &rules(), decision, limits(), &estimator());
        assert!(
            loadout.allows("task.status"),
            "обязательные всегда в loadout"
        );
        assert!(loadout.allows("fs.read"));
        assert!(!loadout.allows("fs.write"), "fallback не даёт mutation");
        assert!(!loadout.tools.is_empty());
    }

    #[test]
    fn conflicting_rules_resolve_to_the_read_only_intent() {
        // "проверь" совпадает и с inspect, и с edit.
        let decision = route_intent(&rules(), "проверь репозиторий", &[]);
        assert_eq!(decision.intent, "inspect");
        assert!(!decision.allows_mutation);
        assert_eq!(decision.matched_rules.len(), 2);
    }

    #[test]
    fn mutation_intent_admits_mutation_tools() {
        let decision = route_intent(&rules(), "измени файл", &[]);
        assert_eq!(decision.intent, "edit");
        assert!(decision.allows_mutation);
        let loadout = build_loadout(&registry(), &rules(), decision, limits(), &estimator());
        assert!(loadout.allows("fs.write"));
    }

    #[test]
    fn explicit_mutation_wins_over_an_inspection_step() {
        let decision = route_intent(&rules(), "проверь проект и измени файл", &[]);
        assert_eq!(decision.intent, "edit");
        assert!(decision.allows_mutation);
        let loadout = build_loadout(&registry(), &rules(), decision, limits(), &estimator());
        assert!(loadout.allows("fs.write"));
    }

    #[test]
    fn explicit_creation_wins_over_a_research_step() {
        let mut intent_rules = rules();
        intent_rules.rules.push(IntentRule {
            id: "research".to_string(),
            intent: "research".to_string(),
            keywords: vec!["изучи".to_string()],
            allows_mutation: false,
            capabilities: vec!["filesystem".to_string()],
        });
        let decision = route_intent(&intent_rules, "изучи проект и создай файл", &[]);
        assert_eq!(decision.intent, "edit");
        assert!(decision.allows_mutation);
    }

    #[test]
    fn open_questions_participate_in_routing() {
        let decision = route_intent(&rules(), "продолжай", &["нужно измени файл".to_string()]);
        assert_eq!(decision.intent, "edit");
    }

    #[test]
    fn deny_rules_remove_tools_regardless_of_intent() {
        let decision = route_intent(&rules(), "измени файл", &[]);
        let loadout = build_loadout(&registry(), &rules(), decision, limits(), &estimator());
        assert!(!loadout.allows("net.fetch"));
        assert!(loadout.denied_tool_ids.contains(&"net.fetch".to_string()));
    }

    #[test]
    fn permission_semantics_stay_visible_for_selected_tools() {
        let decision = route_intent(&rules(), "измени файл", &[]);
        let loadout = build_loadout(&registry(), &rules(), decision, limits(), &estimator());
        let write = loadout
            .tools
            .iter()
            .find(|tool| tool.id == "fs.write")
            .expect("mutation tool selected");
        assert!(write.approval_required);
        assert_eq!(write.permission_label, "write workspace (approval)");
    }

    #[test]
    fn schema_reserve_limits_optional_tools_but_not_mandatory_ones() {
        let decision = route_intent(&rules(), "измени файл", &[]);
        let tight = LoadoutLimits {
            tool_schema_reserve: 1,
            mandatory_schema_reserve: 1_000,
        };
        let loadout = build_loadout(&registry(), &rules(), decision, tight, &estimator());
        assert!(loadout.allows("task.status"));
        assert!(!loadout.allows("fs.read"));
        assert!(loadout.omitted_tool_ids.contains(&"fs.read".to_string()));
    }

    #[test]
    fn out_of_loadout_calls_are_rejected_with_a_bounded_diagnostic() {
        let decision = route_intent(&rules(), "проверь репозиторий", &[]);
        let loadout = build_loadout(&registry(), &rules(), decision, limits(), &estimator());
        let miss = check_tool_call(&loadout, "fs.write").expect_err("mutation is out of loadout");
        assert_eq!(miss.tool_id, "fs.write");
        assert_eq!(miss.intent, "inspect");
        assert_eq!(miss.loadout_id, loadout.loadout_id);
        assert!(!miss.policy_reason.is_empty());
        // Diagnostic не содержит схемы и содержимого запроса.
        assert!(!miss.policy_reason.contains('{'));
    }

    #[test]
    fn the_same_fixture_produces_the_same_loadout() {
        let build = || {
            let decision = route_intent(&rules(), "проверь репозиторий", &[]);
            build_loadout(&registry(), &rules(), decision, limits(), &estimator())
        };
        let left = build();
        let right = build();
        assert_eq!(left.loadout_id, right.loadout_id);
        assert_eq!(left.to_record(), right.to_record());
    }

    #[test]
    fn loadout_record_carries_only_bounded_fields() {
        let decision = route_intent(&rules(), "проверь репозиторий", &[]);
        let record =
            build_loadout(&registry(), &rules(), decision, limits(), &estimator()).to_record();
        assert_eq!(record.intent, "inspect");
        assert_eq!(record.rules_version, INTENT_RULES_VERSION);
        assert!(record.matched_rule.is_some());
        assert!(record.tool_ids.iter().all(|id| !id.contains('{')));
    }
}
