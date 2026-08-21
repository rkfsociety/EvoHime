//! Канонический typed workflow contract `workflow/v1`.
//!
//! Модуль описывает граф целиком: identity блока, action profile узла,
//! acceptance contract, маршруты, явные failure-ветви и bounded execution
//! policy. Контракт не содержит операций редактирования: новая версия графа
//! создаётся целиком, а уже запущенный граф остаётся неизменяемым — runtime
//! работает со snapshot и его canonical hash.
//!
//! Здесь нет ни одного пути к inline script, произвольному URL, shell или
//! dynamic import: поля идентичности ограничены строгим charset
//! (`[a-z0-9._:-]`), поэтому URL, команда или путь физически не могут быть
//! записаны в имя сервера, инструмента или провайдера. Разрешение самих
//! идентичностей выполняет Core-owned реестр (`crate::workflow_registry`),
//! а не сам граф.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Версия контракта. Совместимость проверяется до любой валидации графа.
pub const WORKFLOW_CONTRACT_VERSION: &str = "workflow/v1";

pub const MAX_GRAPH_NODES: usize = 256;
pub const MAX_GRAPH_EDGES: usize = 512;
pub const MAX_NODE_PORTS: usize = 64;
pub const MAX_TIMEOUT_MS: u64 = 300_000;
pub const MAX_RETRY_ATTEMPTS: u32 = 10;
pub const MAX_LOOP_ITERATIONS: u32 = 100;

/// Bounded-лимиты action profiles.
pub const MAX_IDENTITY_CHARS: usize = 128;
pub const MAX_TEXT_CHARS: usize = 2_048;
pub const MAX_SCHEMA_CHARS: usize = 8_192;
pub const MAX_ROUTES: usize = 16;
pub const MAX_ALLOWLIST_ITEMS: usize = 32;
pub const MAX_ARGUMENTS: usize = 16;
pub const MAX_REQUIRED_EVIDENCE: u32 = 32;
pub const MAX_CHILD_REVISIONS: u32 = 3;
pub const MAX_BATCH_ITEMS: u32 = 64;
pub const MAX_PROVIDER_ITEMS: u32 = 64;
pub const MAX_RUN_PARALLELISM: u32 = 8;

/// Максимальная свежесть контекстного провайдера: сутки. Всё, что старше,
/// не может быть объявлено «свежим» контрактом.
pub const MAX_FRESHNESS_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortType {
    Text,
    Integer,
    Number,
    Boolean,
    Json,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Port {
    pub name: String,
    pub value_type: PortType,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_ms: u64,
    #[serde(default)]
    pub retryable_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    Cooperative,
    Immediate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalPolicy {
    pub required: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPolicy {
    pub retry: RetryPolicy,
    pub timeout_ms: u64,
    pub cancellation: CancellationPolicy,
    pub approval: ApprovalPolicy,
}

// ---------------------------------------------------------------------------
// Block identity (AutoGPT-inspired): стабильный идентификатор возможности.
// ---------------------------------------------------------------------------

/// Ссылка на зарегистрированный Core-owned блок. Изменение схемы или
/// поведения блока требует новой версии: silent mutation невозможен, потому
/// что запуск сохраняет `block_version` в snapshot графа.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockRef {
    pub block_id: String,
    pub block_version: u32,
}

/// Ограниченный execution context, который получает узел. Он не приходит из
/// графа: runtime заполняет его сам, а контракт лишь фиксирует форму.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeExecutionContext {
    pub workflow_run_id: String,
    pub node_id: String,
    pub attempt_id: String,
    pub graph_hash: String,
}

// ---------------------------------------------------------------------------
// Action profiles.
// ---------------------------------------------------------------------------

/// Бюджет узла. Подмножество родительского run budget проверяется runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeBudget {
    #[serde(default)]
    pub max_tokens: u64,
    #[serde(default)]
    pub max_seconds: u64,
    #[serde(default)]
    pub max_tool_calls: u64,
}

impl Default for NodeBudget {
    fn default() -> Self {
        Self {
            max_tokens: 8_000,
            max_seconds: 120,
            max_tool_calls: 16,
        }
    }
}

impl NodeBudget {
    pub fn is_within(&self, parent: &NodeBudget) -> bool {
        self.max_tokens <= parent.max_tokens
            && self.max_seconds <= parent.max_seconds
            && self.max_tool_calls <= parent.max_tool_calls
    }
}

/// Инструментальный узел: имя уже зарегистрированного Core tool и статические
/// аргументы. Значения аргументов — литералы или ссылки на входной порт вида
/// `$port_name`; произвольный код или URL сюда не помещаются.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolActionProfile {
    pub tool_name: String,
    #[serde(default)]
    pub arguments: BTreeMap<String, String>,
}

/// Child workflow: роль, цель, output schema, allowlists, grants и бюджет.
/// Nested child delegation запрещена по контракту: у профиля нет поля,
/// которым child мог бы объявить собственный child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildActionProfile {
    pub role: String,
    pub goal: String,
    #[serde(default)]
    pub output_schema: Option<String>,
    #[serde(default)]
    pub context_allowlist: Vec<String>,
    #[serde(default)]
    pub artifact_allowlist: Vec<String>,
    #[serde(default)]
    pub grants: Vec<String>,
    #[serde(default)]
    pub budget: NodeBudget,
    #[serde(default = "default_max_revisions")]
    pub max_revisions: u32,
}

fn default_max_revisions() -> u32 {
    1
}

/// MCP-узел ссылается только на запись Core-owned реестра. Ни URL, ни
/// command, ни headers в контракте нет — это структурная гарантия, а не
/// проверка значения.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpActionProfile {
    pub server_id: String,
    pub tool_name: String,
    #[serde(default)]
    pub arguments: BTreeMap<String, String>,
}

/// Read-only контекстный провайдер: источник, свежесть, схема evidence и
/// bounded размер результата.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProviderProfile {
    pub provider_id: String,
    #[serde(default)]
    pub query: String,
    #[serde(default = "default_provider_items")]
    pub max_items: u32,
    #[serde(default = "default_freshness_ms")]
    pub max_age_ms: u64,
    #[serde(default)]
    pub evidence_schema: Option<String>,
}

fn default_provider_items() -> u32 {
    8
}

fn default_freshness_ms() -> u64 {
    60 * 60 * 1_000
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionMode {
    /// Все входные булевы порты истинны.
    #[default]
    All,
    /// Хотя бы один входной булев порт истинен.
    Any,
}

/// Действие узла. Порядок вариантов и их имена — часть контракта.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NodeType {
    Research,
    Transform,
    Tool {
        tool: ToolActionProfile,
    },
    Condition {
        #[serde(default)]
        mode: ConditionMode,
    },
    Approval,
    /// Core-owned статическое разворачивание уже проверенного графа в
    /// пределах того же run policy, budget и approval. Это не nested child
    /// delegation: разворачивание выполняет Core до запуска, вложенные
    /// subgraph-узлы запрещены.
    Subgraph {
        graph_id: String,
    },
    Loop {
        max_iterations: u32,
    },
    Child {
        child: ChildActionProfile,
    },
    McpTool {
        mcp: McpActionProfile,
    },
    ContextProvider {
        provider: ContextProviderProfile,
    },
}

impl NodeType {
    /// Стабильное имя профиля для projection, событий и реестра блоков.
    pub fn action_kind(&self) -> &'static str {
        match self {
            NodeType::Research => "research",
            NodeType::Transform => "transform",
            NodeType::Tool { .. } => "tool",
            NodeType::Condition { .. } => "condition",
            NodeType::Approval => "approval",
            NodeType::Subgraph { .. } => "subgraph",
            NodeType::Loop { .. } => "loop",
            NodeType::Child { .. } => "child",
            NodeType::McpTool { .. } => "mcp_tool",
            NodeType::ContextProvider { .. } => "context_provider",
        }
    }

    /// Узлы с внешним побочным эффектом или собственным состоянием не
    /// выполняются параллельно, если граф не объявил безопасную
    /// concurrency-семантику явно.
    pub fn is_stateful(&self) -> bool {
        matches!(
            self,
            NodeType::Tool { .. } | NodeType::McpTool { .. } | NodeType::Child { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// Acceptance, маршруты и failure-ветви.
// ---------------------------------------------------------------------------

/// Контракт приёмки узла: схема результата, минимум evidence, разрешённые
/// статусы и retryable-классы ошибок.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NodeAcceptance {
    #[serde(default)]
    pub output_schema: Option<String>,
    #[serde(default)]
    pub required_evidence: u32,
    #[serde(default)]
    pub allowed_statuses: Vec<String>,
    #[serde(default)]
    pub retryable_error_classes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    /// Ошибка узла останавливает ветвь. Downstream остаётся заблокированным.
    #[default]
    Fail,
    /// Ошибка уходит только в объявленные failure-рёбра.
    Branch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinMode {
    /// Узел ждёт все входящие data-рёбра.
    #[default]
    All,
    /// Узлу достаточно одной успешной входящей ветви.
    Any,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConcurrencyClass {
    /// Выполняется строго последовательно относительно других узлов запуска.
    #[default]
    Sequential,
    /// Может выполняться параллельно с другими такими же узлами.
    Parallel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchPolicy {
    pub max_items: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeChannel {
    #[default]
    Data,
    Failure,
}

// ---------------------------------------------------------------------------
// Граф.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub node_type: NodeType,
    #[serde(default)]
    pub inputs: Vec<Port>,
    #[serde(default)]
    pub outputs: Vec<Port>,
    pub execution: ExecutionPolicy,
    /// Идентичность блока. Отсутствие ссылки означает встроенный
    /// deterministic узел без внешней capability.
    #[serde(default)]
    pub block: Option<BlockRef>,
    /// Allowlist имён исходящих маршрутов. Ребро с маршрутом вне списка
    /// отклоняется валидацией, поэтому модель не может выбрать произвольный
    /// node ID.
    #[serde(default)]
    pub routes: Vec<String>,
    #[serde(default)]
    pub acceptance: NodeAcceptance,
    #[serde(default)]
    pub on_failure: FailurePolicy,
    #[serde(default)]
    pub join: JoinMode,
    #[serde(default)]
    pub concurrency: ConcurrencyClass,
    #[serde(default)]
    pub batch: Option<BatchPolicy>,
}

impl WorkflowNode {
    /// Узел с deterministic-профилем и bounded execution policy.
    pub fn new(id: &str, node_type: NodeType, execution: ExecutionPolicy) -> Self {
        Self {
            id: id.into(),
            node_type,
            inputs: Vec::new(),
            outputs: Vec::new(),
            execution,
            block: None,
            routes: Vec::new(),
            acceptance: NodeAcceptance::default(),
            on_failure: FailurePolicy::Fail,
            join: JoinMode::All,
            concurrency: ConcurrencyClass::Sequential,
            batch: None,
        }
    }

    pub fn with_input(mut self, name: &str, value_type: PortType, required: bool) -> Self {
        self.inputs.push(Port {
            name: name.into(),
            value_type,
            required,
        });
        self
    }

    pub fn with_output(mut self, name: &str, value_type: PortType) -> Self {
        self.outputs.push(Port {
            name: name.into(),
            value_type,
            required: false,
        });
        self
    }

    pub fn with_block(mut self, block_id: &str, block_version: u32) -> Self {
        self.block = Some(BlockRef {
            block_id: block_id.into(),
            block_version,
        });
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
    #[serde(default)]
    pub channel: EdgeChannel,
    #[serde(default)]
    pub route: Option<String>,
}

impl WorkflowEdge {
    pub fn data(from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> Self {
        Self {
            from_node: from_node.into(),
            from_port: from_port.into(),
            to_node: to_node.into(),
            to_port: to_port.into(),
            channel: EdgeChannel::Data,
            route: None,
        }
    }

    pub fn failure(from_node: &str, from_port: &str, to_node: &str, to_port: &str) -> Self {
        Self {
            from_node: from_node.into(),
            from_port: from_port.into(),
            to_node: to_node.into(),
            to_port: to_port.into(),
            channel: EdgeChannel::Failure,
            route: None,
        }
    }

    pub fn with_route(mut self, route: &str) -> Self {
        self.route = Some(route.into());
        self
    }
}

/// Run-level budget snapshot графа. Он фиксируется в момент запуска и не
/// меняется вместе с библиотекой шаблонов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowBudget {
    pub max_parallel_nodes: u32,
    pub max_tokens: u64,
    pub max_tool_calls: u64,
    pub max_wall_clock_ms: u64,
}

impl Default for WorkflowBudget {
    fn default() -> Self {
        Self {
            max_parallel_nodes: 2,
            max_tokens: 200_000,
            max_tool_calls: 200,
            max_wall_clock_ms: 30 * 60 * 1_000,
        }
    }
}

fn default_contract() -> String {
    WORKFLOW_CONTRACT_VERSION.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowGraph {
    #[serde(default = "default_contract")]
    pub contract: String,
    pub graph_id: String,
    pub version: u64,
    pub entry_node: String,
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
    #[serde(default)]
    pub budget: WorkflowBudget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    UnsupportedContract(String),
    EmptyGraphId,
    InvalidVersion,
    EmptyEntryNode,
    TooManyNodes {
        actual: usize,
        maximum: usize,
    },
    TooManyEdges {
        actual: usize,
        maximum: usize,
    },
    EmptyNodeId,
    DuplicateNodeId(String),
    UnknownEntryNode(String),
    UnknownNode(String),
    DuplicatePort {
        node_id: String,
        port: String,
    },
    TooManyPorts {
        node_id: String,
        actual: usize,
        maximum: usize,
    },
    EmptyPortName {
        node_id: String,
    },
    EmptySubgraphId {
        node_id: String,
    },
    InvalidLoopBound {
        node_id: String,
        actual: u32,
        maximum: u32,
    },
    InvalidRetryAttempts {
        node_id: String,
        actual: u32,
        maximum: u32,
    },
    InvalidTimeout {
        node_id: String,
        actual: u64,
        maximum: u64,
    },
    InvalidBackoff {
        node_id: String,
    },
    EmptyRetryableError {
        node_id: String,
    },
    /// Идентификатор нарушает строгий charset и потому мог бы нести URL,
    /// путь или команду.
    InvalidIdentity {
        node_id: String,
        field: &'static str,
        value: String,
    },
    TextTooLong {
        node_id: String,
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    EmptyField {
        node_id: String,
        field: &'static str,
    },
    TooManyItems {
        node_id: String,
        field: &'static str,
        actual: usize,
        maximum: usize,
    },
    InvalidBound {
        node_id: String,
        field: &'static str,
        actual: u64,
        maximum: u64,
    },
    InvalidBlockRef {
        node_id: String,
    },
    UnknownSourcePort {
        node_id: String,
        port: String,
    },
    UnknownTargetPort {
        node_id: String,
        port: String,
    },
    TypeMismatch {
        edge: WorkflowEdge,
        from: PortType,
        to: PortType,
    },
    RequiredInputUnconnected {
        node_id: String,
        port: String,
    },
    DuplicateInputConnection {
        node_id: String,
        port: String,
    },
    /// Маршрут ребра не объявлен исходящим узлом.
    UnknownRoute {
        node_id: String,
        route: String,
    },
    /// Failure-ребро выходит из узла, который не объявил failure-ветвление.
    UnexpectedFailureBranch {
        node_id: String,
    },
    /// Узел объявил failure-ветвление, но ветви нет: ошибка была бы
    /// проглочена.
    MissingFailureBranch {
        node_id: String,
    },
    SelfLoop(String),
    Cycle(Vec<String>),
    UnreachableNode(String),
}

impl WorkflowGraph {
    /// Проверяет полный граф в стабильном порядке и возвращает ошибки в том же порядке.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        let mut nodes = BTreeMap::new();

        if self.contract != WORKFLOW_CONTRACT_VERSION {
            errors.push(ValidationError::UnsupportedContract(self.contract.clone()));
        }
        if self.graph_id.trim().is_empty() {
            errors.push(ValidationError::EmptyGraphId);
        }
        if self.version == 0 {
            errors.push(ValidationError::InvalidVersion);
        }
        if self.entry_node.trim().is_empty() {
            errors.push(ValidationError::EmptyEntryNode);
        }
        if self.nodes.len() > MAX_GRAPH_NODES {
            errors.push(ValidationError::TooManyNodes {
                actual: self.nodes.len(),
                maximum: MAX_GRAPH_NODES,
            });
        }
        if self.edges.len() > MAX_GRAPH_EDGES {
            errors.push(ValidationError::TooManyEdges {
                actual: self.edges.len(),
                maximum: MAX_GRAPH_EDGES,
            });
        }
        if self.budget.max_parallel_nodes == 0
            || self.budget.max_parallel_nodes > MAX_RUN_PARALLELISM
        {
            errors.push(ValidationError::InvalidBound {
                node_id: String::new(),
                field: "budget.max_parallel_nodes",
                actual: u64::from(self.budget.max_parallel_nodes),
                maximum: u64::from(MAX_RUN_PARALLELISM),
            });
        }
        if self.budget.max_wall_clock_ms == 0 {
            errors.push(ValidationError::InvalidBound {
                node_id: String::new(),
                field: "budget.max_wall_clock_ms",
                actual: 0,
                maximum: u64::MAX,
            });
        }

        for node in &self.nodes {
            if node.id.trim().is_empty() {
                errors.push(ValidationError::EmptyNodeId);
                continue;
            }
            if nodes.insert(node.id.clone(), node).is_some() {
                errors.push(ValidationError::DuplicateNodeId(node.id.clone()));
            }
        }

        if !self.entry_node.trim().is_empty() && !nodes.contains_key(&self.entry_node) {
            errors.push(ValidationError::UnknownEntryNode(self.entry_node.clone()));
        }

        for node in nodes.values() {
            validate_node(node, &mut errors);
        }

        let mut incoming = BTreeMap::<(String, String), usize>::new();
        let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
        let mut failure_sources = BTreeSet::<String>::new();
        for edge in &self.edges {
            let Some(source) = nodes.get(&edge.from_node) else {
                errors.push(ValidationError::UnknownNode(edge.from_node.clone()));
                continue;
            };
            let Some(target) = nodes.get(&edge.to_node) else {
                errors.push(ValidationError::UnknownNode(edge.to_node.clone()));
                continue;
            };
            let Some(from) = source
                .outputs
                .iter()
                .find(|port| port.name == edge.from_port)
            else {
                errors.push(ValidationError::UnknownSourcePort {
                    node_id: edge.from_node.clone(),
                    port: edge.from_port.clone(),
                });
                continue;
            };
            let Some(to) = target.inputs.iter().find(|port| port.name == edge.to_port) else {
                errors.push(ValidationError::UnknownTargetPort {
                    node_id: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                });
                continue;
            };
            if from.value_type != to.value_type {
                errors.push(ValidationError::TypeMismatch {
                    edge: edge.clone(),
                    from: from.value_type.clone(),
                    to: to.value_type.clone(),
                });
            }
            if let Some(route) = edge.route.as_ref() {
                if !source.routes.iter().any(|declared| declared == route) {
                    errors.push(ValidationError::UnknownRoute {
                        node_id: edge.from_node.clone(),
                        route: route.clone(),
                    });
                }
            }
            match edge.channel {
                EdgeChannel::Failure => {
                    if source.on_failure != FailurePolicy::Branch {
                        errors.push(ValidationError::UnexpectedFailureBranch {
                            node_id: edge.from_node.clone(),
                        });
                    }
                    failure_sources.insert(edge.from_node.clone());
                }
                EdgeChannel::Data => {}
            }
            let key = (edge.to_node.clone(), edge.to_port.clone());
            let count = incoming.entry(key).or_default();
            *count += 1;
            if *count > 1 {
                errors.push(ValidationError::DuplicateInputConnection {
                    node_id: edge.to_node.clone(),
                    port: edge.to_port.clone(),
                });
            }
            if edge.from_node == edge.to_node {
                errors.push(ValidationError::SelfLoop(edge.from_node.clone()));
            }
            adjacency
                .entry(edge.from_node.clone())
                .or_default()
                .insert(edge.to_node.clone());
        }

        for node in nodes.values() {
            for port in node.inputs.iter().filter(|port| port.required) {
                if !incoming.contains_key(&(node.id.clone(), port.name.clone())) {
                    errors.push(ValidationError::RequiredInputUnconnected {
                        node_id: node.id.clone(),
                        port: port.name.clone(),
                    });
                }
            }
            if node.on_failure == FailurePolicy::Branch && !failure_sources.contains(&node.id) {
                errors.push(ValidationError::MissingFailureBranch {
                    node_id: node.id.clone(),
                });
            }
        }

        let cycle = find_cycle(&nodes, &adjacency);
        if let Some(cycle) = cycle {
            errors.push(ValidationError::Cycle(cycle));
        }
        let reachable = reachable_nodes(&self.entry_node, &adjacency);
        for node_id in nodes.keys() {
            if !reachable.contains(node_id) {
                errors.push(ValidationError::UnreachableNode(node_id.clone()));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Нормализованный JSON: ключи объектов, узлы и рёбра отсортированы, а
    /// пробелы отсутствуют. Именно эта строка связывает graph snapshot с
    /// model-request provenance и receipts.
    pub fn canonical_json(&self) -> String {
        let mut normalized = self.clone();
        normalized
            .nodes
            .sort_by(|left, right| left.id.cmp(&right.id));
        normalized.edges.sort_by(|left, right| {
            (
                &left.from_node,
                &left.from_port,
                &left.to_node,
                &left.to_port,
            )
                .cmp(&(
                    &right.from_node,
                    &right.from_port,
                    &right.to_node,
                    &right.to_port,
                ))
        });
        let value = serde_json::to_value(&normalized).unwrap_or(serde_json::Value::Null);
        let mut buffer = String::new();
        write_canonical(&value, &mut buffer);
        buffer
    }

    /// SHA-256 канонического представления, hex в нижнем регистре.
    pub fn canonical_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn node(&self, node_id: &str) -> Option<&WorkflowNode> {
        self.nodes.iter().find(|node| node.id == node_id)
    }
}

/// Детерминированная сериализация: объекты пишутся с отсортированными
/// ключами, числа и строки — стандартным JSON-экранированием.
fn write_canonical(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::Object(map) => {
            let ordered: BTreeMap<&String, &serde_json::Value> = map.iter().collect();
            out.push('{');
            for (index, (key, item)) in ordered.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::Value::String(key.clone()).to_string());
                out.push(':');
                write_canonical(item, out);
            }
            out.push('}');
        }
        serde_json::Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Строгий charset идентификаторов. `/`, пробел, обратный слэш и любые
/// управляющие символы исключены, поэтому URL, путь или команда не могут
/// выдать себя за identity.
pub fn is_valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_IDENTITY_CHARS
        && value.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '_' | '-' | ':')
        })
        && !value.starts_with(['.', '-', ':'])
        && !value.contains("::")
}

fn check_identity(
    node_id: &str,
    field: &'static str,
    value: &str,
    errors: &mut Vec<ValidationError>,
) {
    if !is_valid_identity(value) {
        errors.push(ValidationError::InvalidIdentity {
            node_id: node_id.to_string(),
            field,
            value: value.chars().take(MAX_IDENTITY_CHARS).collect(),
        });
    }
}

fn check_text(
    node_id: &str,
    field: &'static str,
    value: &str,
    maximum: usize,
    required: bool,
    errors: &mut Vec<ValidationError>,
) {
    if required && value.trim().is_empty() {
        errors.push(ValidationError::EmptyField {
            node_id: node_id.to_string(),
            field,
        });
        return;
    }
    if value.chars().count() > maximum {
        errors.push(ValidationError::TextTooLong {
            node_id: node_id.to_string(),
            field,
            actual: value.chars().count(),
            maximum,
        });
    }
    if value.chars().any(|ch| ch.is_control() && ch != '\n') {
        errors.push(ValidationError::InvalidIdentity {
            node_id: node_id.to_string(),
            field,
            value: "<control characters>".into(),
        });
    }
}

fn check_items(
    node_id: &str,
    field: &'static str,
    actual: usize,
    maximum: usize,
    errors: &mut Vec<ValidationError>,
) {
    if actual > maximum {
        errors.push(ValidationError::TooManyItems {
            node_id: node_id.to_string(),
            field,
            actual,
            maximum,
        });
    }
}

fn validate_node(node: &WorkflowNode, errors: &mut Vec<ValidationError>) {
    let port_count = node.inputs.len() + node.outputs.len();
    if port_count > MAX_NODE_PORTS {
        errors.push(ValidationError::TooManyPorts {
            node_id: node.id.clone(),
            actual: port_count,
            maximum: MAX_NODE_PORTS,
        });
    }
    let mut names = BTreeSet::new();
    for port in node.inputs.iter().chain(node.outputs.iter()) {
        if port.name.trim().is_empty() {
            errors.push(ValidationError::EmptyPortName {
                node_id: node.id.clone(),
            });
        } else if !names.insert(port.name.clone()) {
            errors.push(ValidationError::DuplicatePort {
                node_id: node.id.clone(),
                port: port.name.clone(),
            });
        }
    }

    if let Some(block) = &node.block {
        if !is_valid_identity(&block.block_id) || block.block_version == 0 {
            errors.push(ValidationError::InvalidBlockRef {
                node_id: node.id.clone(),
            });
        }
    }

    check_items(&node.id, "routes", node.routes.len(), MAX_ROUTES, errors);
    for route in &node.routes {
        check_identity(&node.id, "route", route, errors);
    }

    validate_acceptance(node, errors);
    validate_action_profile(node, errors);

    if let Some(batch) = node.batch {
        if batch.max_items == 0 || batch.max_items > MAX_BATCH_ITEMS {
            errors.push(ValidationError::InvalidBound {
                node_id: node.id.clone(),
                field: "batch.max_items",
                actual: u64::from(batch.max_items),
                maximum: u64::from(MAX_BATCH_ITEMS),
            });
        }
    }

    let retry = &node.execution.retry;
    if retry.max_attempts == 0 || retry.max_attempts > MAX_RETRY_ATTEMPTS {
        errors.push(ValidationError::InvalidRetryAttempts {
            node_id: node.id.clone(),
            actual: retry.max_attempts,
            maximum: MAX_RETRY_ATTEMPTS,
        });
    }
    if retry.max_attempts > 1 && retry.backoff_ms == 0 {
        errors.push(ValidationError::InvalidBackoff {
            node_id: node.id.clone(),
        });
    }
    for error in &retry.retryable_errors {
        if error.trim().is_empty() {
            errors.push(ValidationError::EmptyRetryableError {
                node_id: node.id.clone(),
            });
        }
    }
    if node.execution.timeout_ms == 0 || node.execution.timeout_ms > MAX_TIMEOUT_MS {
        errors.push(ValidationError::InvalidTimeout {
            node_id: node.id.clone(),
            actual: node.execution.timeout_ms,
            maximum: MAX_TIMEOUT_MS,
        });
    }
}

fn validate_acceptance(node: &WorkflowNode, errors: &mut Vec<ValidationError>) {
    let acceptance = &node.acceptance;
    if let Some(schema) = &acceptance.output_schema {
        check_text(
            &node.id,
            "acceptance.output_schema",
            schema,
            MAX_SCHEMA_CHARS,
            true,
            errors,
        );
        if serde_json::from_str::<serde_json::Value>(schema).is_err() {
            errors.push(ValidationError::EmptyField {
                node_id: node.id.clone(),
                field: "acceptance.output_schema",
            });
        }
    }
    if acceptance.required_evidence > MAX_REQUIRED_EVIDENCE {
        errors.push(ValidationError::InvalidBound {
            node_id: node.id.clone(),
            field: "acceptance.required_evidence",
            actual: u64::from(acceptance.required_evidence),
            maximum: u64::from(MAX_REQUIRED_EVIDENCE),
        });
    }
    check_items(
        &node.id,
        "acceptance.allowed_statuses",
        acceptance.allowed_statuses.len(),
        MAX_ALLOWLIST_ITEMS,
        errors,
    );
    for status in &acceptance.allowed_statuses {
        check_identity(&node.id, "acceptance.allowed_statuses", status, errors);
    }
    check_items(
        &node.id,
        "acceptance.retryable_error_classes",
        acceptance.retryable_error_classes.len(),
        MAX_ALLOWLIST_ITEMS,
        errors,
    );
    for class in &acceptance.retryable_error_classes {
        check_identity(
            &node.id,
            "acceptance.retryable_error_classes",
            class,
            errors,
        );
    }
}

fn validate_action_profile(node: &WorkflowNode, errors: &mut Vec<ValidationError>) {
    match &node.node_type {
        NodeType::Research | NodeType::Transform | NodeType::Approval => {}
        NodeType::Condition { .. } => {
            if node
                .inputs
                .iter()
                .any(|port| port.value_type != PortType::Boolean)
            {
                errors.push(ValidationError::EmptyField {
                    node_id: node.id.clone(),
                    field: "condition.inputs",
                });
            }
        }
        NodeType::Tool { tool } => {
            check_identity(&node.id, "tool.tool_name", &tool.tool_name, errors);
            check_items(
                &node.id,
                "tool.arguments",
                tool.arguments.len(),
                MAX_ARGUMENTS,
                errors,
            );
            for (key, value) in &tool.arguments {
                check_identity(&node.id, "tool.arguments.key", key, errors);
                check_text(
                    &node.id,
                    "tool.arguments.value",
                    value,
                    MAX_TEXT_CHARS,
                    false,
                    errors,
                );
            }
        }
        NodeType::McpTool { mcp } => {
            check_identity(&node.id, "mcp.server_id", &mcp.server_id, errors);
            check_identity(&node.id, "mcp.tool_name", &mcp.tool_name, errors);
            check_items(
                &node.id,
                "mcp.arguments",
                mcp.arguments.len(),
                MAX_ARGUMENTS,
                errors,
            );
            for (key, value) in &mcp.arguments {
                check_identity(&node.id, "mcp.arguments.key", key, errors);
                check_text(
                    &node.id,
                    "mcp.arguments.value",
                    value,
                    MAX_TEXT_CHARS,
                    false,
                    errors,
                );
            }
        }
        NodeType::ContextProvider { provider } => {
            check_identity(
                &node.id,
                "provider.provider_id",
                &provider.provider_id,
                errors,
            );
            check_text(
                &node.id,
                "provider.query",
                &provider.query,
                MAX_TEXT_CHARS,
                false,
                errors,
            );
            if provider.max_items == 0 || provider.max_items > MAX_PROVIDER_ITEMS {
                errors.push(ValidationError::InvalidBound {
                    node_id: node.id.clone(),
                    field: "provider.max_items",
                    actual: u64::from(provider.max_items),
                    maximum: u64::from(MAX_PROVIDER_ITEMS),
                });
            }
            if provider.max_age_ms == 0 || provider.max_age_ms > MAX_FRESHNESS_MS {
                errors.push(ValidationError::InvalidBound {
                    node_id: node.id.clone(),
                    field: "provider.max_age_ms",
                    actual: provider.max_age_ms,
                    maximum: MAX_FRESHNESS_MS,
                });
            }
            if let Some(schema) = &provider.evidence_schema {
                check_text(
                    &node.id,
                    "provider.evidence_schema",
                    schema,
                    MAX_SCHEMA_CHARS,
                    true,
                    errors,
                );
            }
        }
        NodeType::Child { child } => {
            check_identity(&node.id, "child.role", &child.role, errors);
            check_text(
                &node.id,
                "child.goal",
                &child.goal,
                MAX_TEXT_CHARS,
                true,
                errors,
            );
            if let Some(schema) = &child.output_schema {
                check_text(
                    &node.id,
                    "child.output_schema",
                    schema,
                    MAX_SCHEMA_CHARS,
                    true,
                    errors,
                );
                if serde_json::from_str::<serde_json::Value>(schema).is_err() {
                    errors.push(ValidationError::EmptyField {
                        node_id: node.id.clone(),
                        field: "child.output_schema",
                    });
                }
            }
            check_items(
                &node.id,
                "child.context_allowlist",
                child.context_allowlist.len(),
                MAX_ALLOWLIST_ITEMS,
                errors,
            );
            check_items(
                &node.id,
                "child.artifact_allowlist",
                child.artifact_allowlist.len(),
                MAX_ALLOWLIST_ITEMS,
                errors,
            );
            check_items(
                &node.id,
                "child.grants",
                child.grants.len(),
                MAX_ALLOWLIST_ITEMS,
                errors,
            );
            for grant in &child.grants {
                check_identity(&node.id, "child.grants", grant, errors);
            }
            for item in child
                .context_allowlist
                .iter()
                .chain(child.artifact_allowlist.iter())
            {
                check_text(
                    &node.id,
                    "child.allowlist",
                    item,
                    MAX_TEXT_CHARS,
                    true,
                    errors,
                );
            }
            if child.max_revisions > MAX_CHILD_REVISIONS {
                errors.push(ValidationError::InvalidBound {
                    node_id: node.id.clone(),
                    field: "child.max_revisions",
                    actual: u64::from(child.max_revisions),
                    maximum: u64::from(MAX_CHILD_REVISIONS),
                });
            }
            if child.budget.max_tokens == 0
                || child.budget.max_seconds == 0
                || child.budget.max_tool_calls == 0
            {
                errors.push(ValidationError::InvalidBound {
                    node_id: node.id.clone(),
                    field: "child.budget",
                    actual: 0,
                    maximum: u64::MAX,
                });
            }
        }
        NodeType::Subgraph { graph_id } => {
            if graph_id.trim().is_empty() {
                errors.push(ValidationError::EmptySubgraphId {
                    node_id: node.id.clone(),
                });
            } else {
                check_identity(&node.id, "subgraph.graph_id", graph_id, errors);
            }
        }
        NodeType::Loop { max_iterations } => {
            if *max_iterations == 0 || *max_iterations > MAX_LOOP_ITERATIONS {
                errors.push(ValidationError::InvalidLoopBound {
                    node_id: node.id.clone(),
                    actual: *max_iterations,
                    maximum: MAX_LOOP_ITERATIONS,
                });
            }
        }
    }
}

fn reachable_nodes(
    entry: &str,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([entry.to_string()]);
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node.clone()) {
            continue;
        }
        if let Some(next) = adjacency.get(&node) {
            queue.extend(next.iter().cloned());
        }
    }
    seen
}

fn find_cycle(
    nodes: &BTreeMap<String, &WorkflowNode>,
    adjacency: &BTreeMap<String, BTreeSet<String>>,
) -> Option<Vec<String>> {
    fn visit(
        node: &str,
        adjacency: &BTreeMap<String, BTreeSet<String>>,
        colors: &mut BTreeMap<String, u8>,
        stack: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        colors.insert(node.to_string(), 1);
        stack.push(node.to_string());
        for next in adjacency.get(node).into_iter().flatten() {
            match colors.get(next).copied().unwrap_or(0) {
                0 => {
                    if let Some(cycle) = visit(next, adjacency, colors, stack) {
                        return Some(cycle);
                    }
                }
                1 => {
                    let start = stack.iter().position(|item| item == next).unwrap_or(0);
                    let mut cycle = stack[start..].to_vec();
                    cycle.push(next.clone());
                    return Some(cycle);
                }
                _ => {}
            }
        }
        stack.pop();
        colors.insert(node.to_string(), 2);
        None
    }
    let mut colors = BTreeMap::new();
    for node in nodes.keys() {
        if colors.get(node).copied().unwrap_or(0) == 0 {
            if let Some(cycle) = visit(node, adjacency, &mut colors, &mut Vec::new()) {
                return Some(cycle);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 2,
                backoff_ms: 10,
                retryable_errors: vec!["transient".into()],
            },
            timeout_ms: 1_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: false,
                reason: None,
            },
        }
    }

    fn node(
        id: &str,
        input: Option<(&str, PortType)>,
        output: Option<(&str, PortType)>,
    ) -> WorkflowNode {
        let mut node = WorkflowNode::new(id, NodeType::Transform, policy());
        if let Some((name, value_type)) = input {
            node = node.with_input(name, value_type, true);
        }
        if let Some((name, value_type)) = output {
            node = node.with_output(name, value_type);
        }
        node
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> WorkflowGraph {
        WorkflowGraph {
            contract: WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "graph-1".into(),
            version: 1,
            entry_node: "source".into(),
            nodes,
            edges,
            budget: WorkflowBudget::default(),
        }
    }

    #[test]
    fn accepts_a_typed_bounded_static_graph() {
        let result = graph(
            vec![
                node("source", None, Some(("text", PortType::Text))),
                node("sink", Some(("text", PortType::Text)), None),
            ],
            vec![WorkflowEdge::data("source", "text", "sink", "text")],
        )
        .validate();
        assert_eq!(result, Ok(()));
    }

    #[test]
    fn rejects_type_mismatch_and_missing_required_input() {
        let errors = graph(
            vec![
                node("source", None, Some(("value", PortType::Integer))),
                node("sink", Some(("text", PortType::Text)), None),
            ],
            vec![WorkflowEdge::data("source", "value", "sink", "text")],
        )
        .validate()
        .expect_err("invalid types");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::TypeMismatch { .. })));
        assert!(!errors
            .iter()
            .any(|error| matches!(error, ValidationError::RequiredInputUnconnected { .. })));
    }

    #[test]
    fn rejects_cycles_and_unreachable_nodes_deterministically() {
        let errors = graph(
            vec![
                node(
                    "source",
                    Some(("in", PortType::Text)),
                    Some(("out", PortType::Text)),
                ),
                node("orphan", None, None),
            ],
            vec![WorkflowEdge::data("source", "out", "source", "in")],
        )
        .validate()
        .expect_err("cycle");
        assert!(matches!(errors[0], ValidationError::SelfLoop(_)));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::Cycle(_))));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::UnreachableNode(id) if id == "orphan")));
    }

    #[test]
    fn rejects_unbounded_execution_controls() {
        let mut invalid = node("source", None, None);
        invalid.execution.retry.max_attempts = MAX_RETRY_ATTEMPTS + 1;
        invalid.execution.timeout_ms = MAX_TIMEOUT_MS + 1;
        invalid.node_type = NodeType::Loop {
            max_iterations: MAX_LOOP_ITERATIONS + 1,
        };
        let errors = graph(vec![invalid], vec![]).validate().expect_err("bounds");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidRetryAttempts { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidTimeout { .. })));
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::InvalidLoopBound { .. })));
    }

    #[test]
    fn serializes_typed_nodes_with_execution_contract() {
        let mut approval = node("approval", None, Some(("approved", PortType::Boolean)));
        approval.node_type = NodeType::Approval;
        approval.execution.approval = ApprovalPolicy {
            required: true,
            reason: Some("external mutation".into()),
        };
        let mut workflow = graph(vec![approval], vec![]);
        workflow.entry_node = "approval".into();
        let json = serde_json::to_value(&workflow).expect("serializes");
        assert_eq!(json["nodes"][0]["node_type"]["type"], "approval");
        assert_eq!(json["nodes"][0]["execution"]["timeout_ms"], 1000);
        assert_eq!(json["contract"], WORKFLOW_CONTRACT_VERSION);
        let round_trip: WorkflowGraph = serde_json::from_value(json).expect("deserializes");
        assert_eq!(round_trip, workflow);
    }

    #[test]
    fn legacy_graph_without_new_fields_still_deserializes() {
        // Старый снимок графа не знает про contract, budget, routes и
        // acceptance: additive-поля обязаны получить умолчания, иначе
        // сохранённые запуски перестали бы читаться.
        let legacy = serde_json::json!({
            "graph_id": "legacy",
            "version": 1,
            "entry_node": "a",
            "nodes": [{
                "id": "a",
                "node_type": {"type": "transform"},
                "inputs": [],
                "outputs": [],
                "execution": {
                    "retry": {"max_attempts": 1, "backoff_ms": 0},
                    "timeout_ms": 1000,
                    "cancellation": "cooperative",
                    "approval": {"required": false}
                }
            }],
            "edges": []
        });
        let graph: WorkflowGraph = serde_json::from_value(legacy).expect("legacy graph");
        assert_eq!(graph.contract, WORKFLOW_CONTRACT_VERSION);
        assert_eq!(graph.budget, WorkflowBudget::default());
        assert_eq!(graph.validate(), Ok(()));
    }

    #[test]
    fn canonical_hash_ignores_node_and_edge_order() {
        let canonical = graph(
            vec![
                node("source", None, Some(("text", PortType::Text))),
                node("sink", Some(("text", PortType::Text)), None),
            ],
            vec![WorkflowEdge::data("source", "text", "sink", "text")],
        );
        let mut shuffled = canonical.clone();
        shuffled.nodes.reverse();
        assert_eq!(canonical.canonical_hash(), shuffled.canonical_hash());
        assert_eq!(canonical.canonical_hash().len(), 64);

        let mut changed = canonical.clone();
        changed.version = 2;
        assert_ne!(canonical.canonical_hash(), changed.canonical_hash());
    }

    #[test]
    fn identity_fields_cannot_carry_a_url_command_or_path() {
        for hostile in [
            "https://evil.test/mcp",
            "C:\\Windows\\system32\\cmd.exe",
            "python -c print(1)",
            "../escape",
            "Server",
        ] {
            assert!(!is_valid_identity(hostile), "{hostile} must be rejected");
        }
        assert!(is_valid_identity("mcp.local-docs"));
        assert!(is_valid_identity("workspace:rag"));
    }

    #[test]
    fn mcp_node_with_hostile_server_identity_is_rejected() {
        let mut hostile = node("call", None, Some(("out", PortType::Json)));
        hostile.node_type = NodeType::McpTool {
            mcp: McpActionProfile {
                server_id: "https://evil.test".into(),
                tool_name: "search".into(),
                arguments: BTreeMap::new(),
            },
        };
        let mut workflow = graph(vec![hostile], vec![]);
        workflow.entry_node = "call".into();
        let errors = workflow.validate().expect_err("hostile identity");
        assert!(errors.iter().any(|error| matches!(
            error,
            ValidationError::InvalidIdentity { field, .. } if *field == "mcp.server_id"
        )));
    }

    #[test]
    fn unknown_route_and_undeclared_failure_branch_are_rejected() {
        let mut source = node("source", None, Some(("text", PortType::Text)));
        source.routes = vec!["ok".into()];
        let sink = node("sink", Some(("text", PortType::Text)), None);
        let edge = WorkflowEdge::data("source", "text", "sink", "text").with_route("unknown");
        let errors = graph(vec![source.clone(), sink.clone()], vec![edge])
            .validate()
            .expect_err("unknown route");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::UnknownRoute { .. })));

        let failure_edge = WorkflowEdge::failure("source", "text", "sink", "text");
        let errors = graph(vec![source, sink], vec![failure_edge])
            .validate()
            .expect_err("undeclared failure branch");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::UnexpectedFailureBranch { .. })));
    }

    #[test]
    fn declared_failure_branch_without_an_edge_is_rejected() {
        let mut source = node("source", None, Some(("text", PortType::Text)));
        source.on_failure = FailurePolicy::Branch;
        let sink = node("sink", Some(("text", PortType::Text)), None);
        let errors = graph(
            vec![source, sink],
            vec![WorkflowEdge::data("source", "text", "sink", "text")],
        )
        .validate()
        .expect_err("missing failure branch");
        assert!(errors
            .iter()
            .any(|error| matches!(error, ValidationError::MissingFailureBranch { .. })));
    }

    #[test]
    fn declared_failure_branch_with_an_edge_is_accepted() {
        let mut source = node("source", None, Some(("text", PortType::Text)));
        source.on_failure = FailurePolicy::Branch;
        source.outputs.push(Port {
            name: "error".into(),
            value_type: PortType::Text,
            required: false,
        });
        let sink = node("sink", Some(("text", PortType::Text)), None);
        let fallback = node("fallback", Some(("text", PortType::Text)), None);
        assert_eq!(
            graph(
                vec![source, sink, fallback],
                vec![
                    WorkflowEdge::data("source", "text", "sink", "text"),
                    WorkflowEdge::failure("source", "error", "fallback", "text"),
                ],
            )
            .validate(),
            Ok(())
        );
    }

    #[test]
    fn child_profile_bounds_grants_budget_and_revisions() {
        let mut child = node("child", None, Some(("out", PortType::Json)));
        child.node_type = NodeType::Child {
            child: ChildActionProfile {
                role: "reviewer".into(),
                goal: "review the diff".into(),
                output_schema: Some("{\"type\":\"object\"}".into()),
                context_allowlist: vec!["docs/architecture.md".into()],
                artifact_allowlist: vec![],
                grants: vec!["fs.read".into()],
                budget: NodeBudget::default(),
                max_revisions: MAX_CHILD_REVISIONS + 1,
            },
        };
        let mut workflow = graph(vec![child], vec![]);
        workflow.entry_node = "child".into();
        let errors = workflow.validate().expect_err("revision bound");
        assert!(errors.iter().any(|error| matches!(
            error,
            ValidationError::InvalidBound { field, .. } if *field == "child.max_revisions"
        )));
    }

    #[test]
    fn unsupported_contract_version_is_rejected_before_use() {
        let mut workflow = graph(vec![node("source", None, None)], vec![]);
        workflow.contract = "workflow/v2".into();
        let errors = workflow.validate().expect_err("contract");
        assert!(matches!(errors[0], ValidationError::UnsupportedContract(_)));
    }
}
