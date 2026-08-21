//! Core-owned библиотека workflow-шаблонов.
//!
//! Шаблон — это versioned definition в коде Core, а не динамически
//! загружаемый агент: у него нет ни файла на диске, ни импорта, ни внешнего
//! runtime. Запуск получает immutable snapshot графа, поэтому изменение
//! библиотеки не меняет уже идущий запуск.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::workflow::{
    ApprovalPolicy, CancellationPolicy, ChildActionProfile, ContextProviderProfile,
    ExecutionPolicy, JoinMode, NodeBudget, NodeType, PortType, RetryPolicy, WorkflowBudget,
    WorkflowEdge, WorkflowGraph, WorkflowNode, MAX_TEXT_CHARS, WORKFLOW_CONTRACT_VERSION,
};

pub const MAX_TEMPLATE_INPUTS: usize = 16;
pub const MAX_INPUT_VALUE_CHARS: usize = MAX_TEXT_CHARS;

/// Пригодность к расписанию. Supervisor-контракт сегодня умеет только
/// `once`/`interval`, поэтому календарные правила обязаны получать typed
/// отказ, а не молча пропускать запуск.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleEligibility {
    /// Разрешены только `once` и `interval`.
    IntervalOnly,
    /// Расписание для шаблона недоступно (например, обязательный approval).
    Unavailable,
}

impl ScheduleEligibility {
    pub fn as_str(self) -> &'static str {
        match self {
            ScheduleEligibility::IntervalOnly => "interval_only",
            ScheduleEligibility::Unavailable => "unavailable",
        }
    }
}

/// Описание одного входа шаблона. Значение всегда строка: ни объект, ни
/// произвольный JSON в шаблон не приходит.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInput {
    pub name: String,
    pub title: String,
    pub required: bool,
    pub max_chars: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTemplate {
    pub template_id: String,
    pub version: u32,
    pub display_name: String,
    pub description: String,
    pub inputs: Vec<TemplateInput>,
    pub required_capabilities: Vec<String>,
    pub schedule_eligibility: ScheduleEligibility,
    /// Безопасный preview: только имена узлов, роли и порядок. Ни prompt, ни
    /// содержимое контекста сюда не попадают.
    pub preview: Vec<String>,
    graph: WorkflowGraph,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateError {
    UnknownTemplate(String),
    UnknownInput(String),
    MissingInput(String),
    InputTooLong {
        name: String,
        actual: usize,
        maximum: usize,
    },
    InvalidInput {
        name: String,
    },
    TooManyInputs {
        actual: usize,
        maximum: usize,
    },
    /// Итоговый граф не прошёл валидацию контракта.
    InvalidGraph(Vec<crate::workflow::ValidationError>),
}

impl TemplateError {
    pub fn code(&self) -> &'static str {
        match self {
            TemplateError::UnknownTemplate(_) => "unknown_template",
            TemplateError::UnknownInput(_) => "unknown_input",
            TemplateError::MissingInput(_) => "missing_input",
            TemplateError::InputTooLong { .. } => "input_too_long",
            TemplateError::InvalidInput { .. } => "invalid_input",
            TemplateError::TooManyInputs { .. } => "too_many_inputs",
            TemplateError::InvalidGraph(_) => "invalid_graph",
        }
    }
}

impl WorkflowTemplate {
    /// Read-only проекция графа шаблона. Мутировать её нельзя: запуск
    /// получает собственную копию через `instantiate`.
    pub fn graph(&self) -> &WorkflowGraph {
        &self.graph
    }

    /// Строит immutable snapshot графа под конкретные входы.
    ///
    /// Подстановка выполняется только в свободном тексте (цель child,
    /// запрос провайдера, значения аргументов инструмента). Идентичности —
    /// имя сервера, инструмента, провайдера, роль — не участвуют в
    /// подстановке, поэтому вход пользователя не может подменить capability.
    pub fn instantiate(
        &self,
        inputs: &BTreeMap<String, String>,
    ) -> Result<WorkflowGraph, TemplateError> {
        if inputs.len() > MAX_TEMPLATE_INPUTS {
            return Err(TemplateError::TooManyInputs {
                actual: inputs.len(),
                maximum: MAX_TEMPLATE_INPUTS,
            });
        }
        for name in inputs.keys() {
            if !self.inputs.iter().any(|input| &input.name == name) {
                return Err(TemplateError::UnknownInput(name.clone()));
            }
        }
        for declared in &self.inputs {
            match inputs.get(&declared.name) {
                None => {
                    if declared.required {
                        return Err(TemplateError::MissingInput(declared.name.clone()));
                    }
                }
                Some(value) => {
                    if declared.required && value.trim().is_empty() {
                        return Err(TemplateError::MissingInput(declared.name.clone()));
                    }
                    if value.chars().count() > declared.max_chars {
                        return Err(TemplateError::InputTooLong {
                            name: declared.name.clone(),
                            actual: value.chars().count(),
                            maximum: declared.max_chars,
                        });
                    }
                    if value.chars().any(|ch| ch.is_control() && ch != '\n') {
                        return Err(TemplateError::InvalidInput {
                            name: declared.name.clone(),
                        });
                    }
                }
            }
        }

        let mut graph = self.graph.clone();
        for node in &mut graph.nodes {
            match &mut node.node_type {
                NodeType::Child { child } => {
                    child.goal = substitute(&child.goal, inputs);
                }
                NodeType::ContextProvider { provider } => {
                    provider.query = substitute(&provider.query, inputs);
                }
                NodeType::Tool { tool } => {
                    for value in tool.arguments.values_mut() {
                        *value = substitute(value, inputs);
                    }
                }
                NodeType::McpTool { mcp } => {
                    for value in mcp.arguments.values_mut() {
                        *value = substitute(value, inputs);
                    }
                }
                _ => {}
            }
        }
        graph
            .validate()
            .map_err(TemplateError::InvalidGraph)
            .map(|()| graph)
    }
}

/// Подставляет `${name}` bounded-значениями. Неизвестные плейсхолдеры
/// остаются как есть: тихая подстановка пустой строкой скрыла бы ошибку
/// шаблона.
fn substitute(text: &str, inputs: &BTreeMap<String, String>) -> String {
    let mut result = text.to_string();
    for (name, value) in inputs {
        result = result.replace(&format!("${{{name}}}"), value);
    }
    result.chars().take(MAX_INPUT_VALUE_CHARS).collect()
}

fn policy(timeout_ms: u64, approval: bool) -> ExecutionPolicy {
    ExecutionPolicy {
        retry: RetryPolicy {
            max_attempts: 2,
            backoff_ms: 500,
            retryable_errors: vec!["transient".into(), "timeout".into()],
        },
        timeout_ms,
        cancellation: CancellationPolicy::Cooperative,
        approval: ApprovalPolicy {
            required: approval,
            reason: if approval {
                Some("Изменение рабочего каталога требует подтверждения".into())
            } else {
                None
            },
        },
    }
}

fn child_budget() -> NodeBudget {
    NodeBudget {
        max_tokens: 16_000,
        max_seconds: 180,
        max_tool_calls: 24,
    }
}

fn child_node(id: &str, role: &str, goal: &str, approval: bool) -> WorkflowNode {
    WorkflowNode::new(
        id,
        NodeType::Child {
            child: ChildActionProfile {
                role: role.into(),
                goal: goal.into(),
                output_schema: Some(CHILD_REPORT_SCHEMA.into()),
                context_allowlist: vec![],
                artifact_allowlist: vec![],
                grants: vec!["fs.read".into()],
                budget: child_budget(),
                max_revisions: 1,
            },
        },
        policy(120_000, approval),
    )
    .with_block("core.child.report", 1)
}

/// Схема typed child report, которую обязан вернуть каждый child-узел
/// шаблона. Без неё fan-in принимал бы произвольный текст.
pub const CHILD_REPORT_SCHEMA: &str = r#"{"type":"object","required":["status","summary"],"properties":{"status":{"type":"string"},"summary":{"type":"string"},"findings":{"type":"array","items":{"type":"string"}}}}"#;

fn budget() -> WorkflowBudget {
    WorkflowBudget {
        max_parallel_nodes: 2,
        max_tokens: 120_000,
        max_tool_calls: 120,
        max_wall_clock_ms: 20 * 60 * 1_000,
    }
}

fn repository_research() -> WorkflowTemplate {
    let context = WorkflowNode::new(
        "context",
        NodeType::ContextProvider {
            provider: ContextProviderProfile {
                provider_id: "workspace.knowledge".into(),
                query: "${question}".into(),
                max_items: 8,
                max_age_ms: 60 * 60 * 1_000,
                evidence_schema: None,
            },
        },
        policy(60_000, false),
    )
    .with_output("evidence", PortType::Json)
    .with_block("core.context.provider", 1);

    let researcher = child_node(
        "researcher",
        "researcher",
        "Ответь на вопрос по репозиторию: ${question}",
        false,
    )
    .with_input("input", PortType::Text, true)
    .with_output("report", PortType::Json);

    let evidence_to_text =
        WorkflowNode::new("evidence", NodeType::Transform, policy(30_000, false))
            .with_input("evidence", PortType::Json, true)
            .with_output("text", PortType::Text);

    let summary = WorkflowNode::new("summary", NodeType::Transform, policy(30_000, false))
        .with_input("report", PortType::Json, true)
        .with_output("summary", PortType::Text);

    let graph = WorkflowGraph {
        contract: WORKFLOW_CONTRACT_VERSION.into(),
        graph_id: "template.repository-research".into(),
        version: 1,
        entry_node: "context".into(),
        nodes: vec![context, evidence_to_text, researcher, summary],
        edges: vec![
            WorkflowEdge::data("context", "evidence", "evidence", "evidence"),
            WorkflowEdge::data("evidence", "text", "researcher", "input"),
            WorkflowEdge::data("researcher", "report", "summary", "report"),
        ],
        budget: budget(),
    };

    WorkflowTemplate {
        template_id: "repository-research".into(),
        version: 1,
        display_name: "Исследование репозитория".into(),
        description: "Собирает контекст рабочего каталога и отвечает на вопрос с evidence".into(),
        inputs: vec![TemplateInput {
            name: "question".into(),
            title: "Вопрос по репозиторию".into(),
            required: true,
            max_chars: 512,
        }],
        required_capabilities: vec!["workspace.knowledge".into(), "child.researcher".into()],
        schedule_eligibility: ScheduleEligibility::IntervalOnly,
        preview: vec![
            "context: контекст рабочего каталога (read-only)".into(),
            "evidence: нормализация evidence".into(),
            "researcher: child-исследователь".into(),
            "summary: итог".into(),
        ],
        graph,
    }
}

fn plan_implement_review() -> WorkflowTemplate {
    let planner = child_node(
        "planner",
        "planner",
        "Составь план по задаче: ${goal}",
        false,
    )
    .with_output("report", PortType::Json);

    let approval = WorkflowNode::new("approval", NodeType::Approval, policy(300_000, true))
        .with_input("plan", PortType::Json, true)
        .with_output("approved", PortType::Json);

    let implementer = child_node(
        "implementer",
        "implementer",
        "Выполни утверждённый план по задаче: ${goal}",
        true,
    )
    .with_input("plan", PortType::Json, true)
    .with_output("report", PortType::Json);

    let reviewer = child_node(
        "reviewer",
        "reviewer",
        "Проверь результат по задаче: ${goal}",
        false,
    )
    .with_input("changes", PortType::Json, true)
    .with_output("report", PortType::Json);

    let graph = WorkflowGraph {
        contract: WORKFLOW_CONTRACT_VERSION.into(),
        graph_id: "template.plan-implement-review".into(),
        version: 1,
        entry_node: "planner".into(),
        nodes: vec![planner, approval, implementer, reviewer],
        edges: vec![
            WorkflowEdge::data("planner", "report", "approval", "plan"),
            WorkflowEdge::data("approval", "approved", "implementer", "plan"),
            WorkflowEdge::data("implementer", "report", "reviewer", "changes"),
        ],
        budget: budget(),
    };

    WorkflowTemplate {
        template_id: "plan-implement-review".into(),
        version: 1,
        display_name: "План → реализация → ревью".into(),
        description: "План, подтверждение человеком, реализация и независимое ревью".into(),
        inputs: vec![TemplateInput {
            name: "goal".into(),
            title: "Цель задачи".into(),
            required: true,
            max_chars: 512,
        }],
        required_capabilities: vec![
            "child.planner".into(),
            "child.implementer".into(),
            "child.reviewer".into(),
        ],
        // Approval внутри графа: расписание запускало бы работу, которая
        // немедленно встала бы в ожидание человека.
        schedule_eligibility: ScheduleEligibility::Unavailable,
        preview: vec![
            "planner: child-планировщик".into(),
            "approval: подтверждение человеком".into(),
            "implementer: child-исполнитель".into(),
            "reviewer: независимое ревью".into(),
        ],
        graph,
    }
}

fn parallel_security_review() -> WorkflowTemplate {
    let scope = WorkflowNode::new("scope", NodeType::Transform, policy(30_000, false))
        .with_output("left", PortType::Text)
        .with_output("right", PortType::Text);

    let mut secrets = child_node(
        "secrets",
        "security-reviewer",
        "Проверь утечки секретов в области: ${scope}",
        false,
    )
    .with_input("input", PortType::Text, true)
    .with_output("report", PortType::Json);
    secrets.concurrency = crate::workflow::ConcurrencyClass::Parallel;

    let mut permissions = child_node(
        "permissions",
        "security-reviewer",
        "Проверь превышение прав в области: ${scope}",
        false,
    )
    .with_input("input", PortType::Text, true)
    .with_output("report", PortType::Json);
    permissions.concurrency = crate::workflow::ConcurrencyClass::Parallel;

    let mut merge = WorkflowNode::new("merge", NodeType::Transform, policy(30_000, false))
        .with_input("secrets", PortType::Json, true)
        .with_input("permissions", PortType::Json, true)
        .with_output("summary", PortType::Text);
    merge.join = JoinMode::All;

    let graph = WorkflowGraph {
        contract: WORKFLOW_CONTRACT_VERSION.into(),
        graph_id: "template.parallel-security-review".into(),
        version: 1,
        entry_node: "scope".into(),
        nodes: vec![scope, secrets, permissions, merge],
        edges: vec![
            WorkflowEdge::data("scope", "left", "secrets", "input"),
            WorkflowEdge::data("scope", "right", "permissions", "input"),
            WorkflowEdge::data("secrets", "report", "merge", "secrets"),
            WorkflowEdge::data("permissions", "report", "merge", "permissions"),
        ],
        budget: budget(),
    };

    WorkflowTemplate {
        template_id: "parallel-security-review".into(),
        version: 1,
        display_name: "Параллельное security review".into(),
        description: "Две независимые проверки и детерминированный fan-in".into(),
        inputs: vec![TemplateInput {
            name: "scope".into(),
            title: "Область проверки".into(),
            required: true,
            max_chars: 512,
        }],
        required_capabilities: vec!["child.security-reviewer".into()],
        schedule_eligibility: ScheduleEligibility::IntervalOnly,
        preview: vec![
            "scope: разбиение области".into(),
            "secrets: child-проверка секретов".into(),
            "permissions: child-проверка прав".into(),
            "merge: детерминированный fan-in".into(),
        ],
        graph,
    }
}

/// Полный Core-owned каталог. Порядок стабилен.
pub fn catalog() -> Vec<WorkflowTemplate> {
    vec![
        parallel_security_review(),
        plan_implement_review(),
        repository_research(),
    ]
}

pub fn template(template_id: &str) -> Option<WorkflowTemplate> {
    catalog()
        .into_iter()
        .find(|item| item.template_id == template_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow_registry::{ParentCapabilities, WorkflowRegistry};
    use std::collections::BTreeSet;

    fn parent() -> ParentCapabilities {
        ParentCapabilities {
            grants: BTreeSet::from(["fs.read".to_string()]),
            budget: NodeBudget {
                max_tokens: 100_000,
                max_seconds: 1_800,
                max_tool_calls: 200,
            },
            context_allowlist: BTreeSet::new(),
        }
    }

    #[test]
    fn every_template_validates_against_contract_and_registry() {
        let registry = WorkflowRegistry::bootstrap();
        for template in catalog() {
            template
                .graph()
                .validate()
                .unwrap_or_else(|errors| panic!("{}: {errors:?}", template.template_id));
            registry
                .validate_bindings(template.graph(), &parent())
                .unwrap_or_else(|errors| panic!("{}: {errors:?}", template.template_id));
        }
    }

    #[test]
    fn instantiation_substitutes_only_free_text_and_keeps_identity() {
        let template = template("repository-research").expect("template");
        let graph = template
            .instantiate(&BTreeMap::from([(
                "question".to_string(),
                "как устроен supervisor".to_string(),
            )]))
            .expect("instantiates");
        let node = graph.node("context").expect("context node");
        match &node.node_type {
            NodeType::ContextProvider { provider } => {
                assert_eq!(provider.query, "как устроен supervisor");
                assert_eq!(provider.provider_id, "workspace.knowledge");
            }
            other => panic!("unexpected node type {other:?}"),
        }
    }

    #[test]
    fn hostile_input_cannot_change_a_capability_identity() {
        let template = template("repository-research").expect("template");
        let graph = template
            .instantiate(&BTreeMap::from([(
                "question".to_string(),
                "https://evil.test/exfiltrate".to_string(),
            )]))
            .expect("instantiates");
        let registry = WorkflowRegistry::bootstrap();
        // Вход попал только в свободный текст запроса, поэтому граф остаётся
        // разрешимым по реестру, а provider_id не изменился.
        registry
            .validate_bindings(&graph, &parent())
            .expect("bindings still resolve");
        match &graph.node("context").expect("context").node_type {
            NodeType::ContextProvider { provider } => {
                assert_eq!(provider.provider_id, "workspace.knowledge")
            }
            other => panic!("unexpected node type {other:?}"),
        }
    }

    #[test]
    fn missing_unknown_and_oversized_inputs_are_rejected() {
        let template = template("repository-research").expect("template");
        assert_eq!(
            template.instantiate(&BTreeMap::new()).expect_err("missing"),
            TemplateError::MissingInput("question".into())
        );
        assert_eq!(
            template
                .instantiate(&BTreeMap::from([("other".into(), "x".into())]))
                .expect_err("unknown")
                .code(),
            "unknown_input"
        );
        assert_eq!(
            template
                .instantiate(&BTreeMap::from([("question".to_string(), "x".repeat(600))]))
                .expect_err("too long")
                .code(),
            "input_too_long"
        );
    }

    #[test]
    fn a_running_snapshot_does_not_change_when_the_library_changes() {
        let template = template("repository-research").expect("template");
        let inputs = BTreeMap::from([("question".to_string(), "вопрос".to_string())]);
        let snapshot = template.instantiate(&inputs).expect("snapshot");
        let hash = snapshot.canonical_hash();

        // «Изменение библиотеки»: новая версия шаблона с другим графом.
        let mut newer = template.clone();
        newer.version = 2;
        newer.graph.version = 2;
        let newer_snapshot = newer.instantiate(&inputs).expect("snapshot");

        assert_eq!(snapshot.canonical_hash(), hash);
        assert_ne!(newer_snapshot.canonical_hash(), hash);
    }

    #[test]
    fn approval_bearing_template_is_not_schedulable() {
        let template = template("plan-implement-review").expect("template");
        assert_eq!(
            template.schedule_eligibility,
            ScheduleEligibility::Unavailable
        );
        let others = ["repository-research", "parallel-security-review"];
        for id in others {
            assert_eq!(
                super::template(id).expect("template").schedule_eligibility,
                ScheduleEligibility::IntervalOnly
            );
        }
    }
}
