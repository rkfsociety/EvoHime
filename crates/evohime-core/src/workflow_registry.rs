//! Core-owned реестр возможностей workflow: блоки, MCP-серверы, контекстные
//! провайдеры и допущенные инструменты.
//!
//! Реестр — единственный источник identity. Граф ссылается на запись по
//! идентификатору, а URL, транспорт, host allowlist и разрешение живут здесь,
//! в коде Core. Ни model output, ни renderer не могут добавить сервер,
//! расширить список инструментов или подменить транспорт: реестр статичен и
//! редактируется как исходный код.
//!
//! Здесь же выполняется вторая половина валидации графа. `WorkflowGraph::
//! validate` проверяет структуру, а `WorkflowRegistry::validate_bindings` —
//! разрешимость идентичностей, allowlist инструментов, свежесть провайдеров и
//! то, что child не может поднять grants или бюджет выше родительских.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::workflow::{
    ChildActionProfile, NodeBudget, NodeType, Port, PortType, WorkflowEdge, WorkflowGraph,
    WorkflowNode, MAX_FRESHNESS_MS, MAX_GRAPH_NODES,
};

/// Переменная окружения host allowlist, которую уже использует Core-owned
/// инструмент `mcp.call`. Реестр обязан согласовываться с ней, иначе запуск
/// узла упёрся бы в SSRF-проверку уже после dispatch marker.
pub const MCP_ALLOWED_HOSTS_ENV: &str = "EVOHIME_MCP_ALLOWED_HOSTS";

/// Единственный исполнитель `mcp_tool`: существующий Core tool.
pub const MCP_TOOL_NAME: &str = "mcp.call";

/// Транспорты MCP. Поддержан только удалённый JSON-RPC: остальные записи
/// валидны в контракте, но помечаются `transport_unavailable` и не могут быть
/// выбраны запуском.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    RemoteJsonRpc,
    Stdio,
    WebSocket,
}

impl McpTransport {
    pub fn is_available(self) -> bool {
        matches!(self, McpTransport::RemoteJsonRpc)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            McpTransport::RemoteJsonRpc => "remote_json_rpc",
            McpTransport::Stdio => "stdio",
            McpTransport::WebSocket => "websocket",
        }
    }
}

/// Запись MCP-сервера. `endpoint` принадлежит Core: граф на него не влияет.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerEntry {
    pub server_id: String,
    pub display_name: String,
    pub transport: McpTransport,
    pub endpoint: String,
    pub allowed_tools: Vec<String>,
}

impl McpServerEntry {
    pub fn host(&self) -> Option<String> {
        let without_scheme = self.endpoint.split_once("://").map(|(_, rest)| rest)?;
        let authority = without_scheme
            .split(['/', '?', '#'])
            .next()
            .unwrap_or_default();
        let host = authority
            .rsplit_once('@')
            .map(|(_, host)| host)
            .unwrap_or(authority);
        let host = host.split(':').next().unwrap_or_default();
        if host.is_empty() {
            None
        } else {
            Some(host.to_ascii_lowercase())
        }
    }
}

/// Источник контекстного провайдера. Все источники read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    /// Локальный индекс рабочего каталога (FTS5/RAG).
    WorkspaceKnowledge,
    /// Сохранённые research evidence.
    ResearchEvidence,
    /// Память задачи и проекта.
    TaskMemory,
}

impl ContextSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ContextSourceKind::WorkspaceKnowledge => "workspace_knowledge",
            ContextSourceKind::ResearchEvidence => "research_evidence",
            ContextSourceKind::TaskMemory => "task_memory",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextProviderEntry {
    pub provider_id: String,
    pub display_name: String,
    pub source: ContextSourceKind,
    /// Потолок элементов и свежести. Профиль узла может быть строже, но не
    /// мягче.
    pub max_items: u32,
    pub max_age_ms: u64,
    pub evidence_schema: String,
}

/// Порт блока: имя, тип и обязательность. Схема блока — часть его версии.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPort {
    pub name: String,
    pub value_type: PortType,
    pub required: bool,
}

impl BlockPort {
    fn matches(&self, port: &Port) -> bool {
        self.name == port.name && self.value_type == port.value_type
    }
}

/// Детерминированная пара «вход → ожидаемый выход». Она существует, чтобы
/// изменение поведения блока ловилось тестом, а не пользователем.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockFixture {
    pub input_json: String,
    pub output_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockDescriptor {
    pub block_id: String,
    pub block_version: u32,
    pub display_name: String,
    pub description: String,
    /// `NodeType::action_kind` узла, который может ссылаться на блок.
    pub action_kind: String,
    pub inputs: Vec<BlockPort>,
    pub outputs: Vec<BlockPort>,
    pub fixture: BlockFixture,
}

/// Родительские возможности запуска. Всё, что получает child, обязано быть их
/// подмножеством.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentCapabilities {
    pub grants: BTreeSet<String>,
    pub budget: NodeBudget,
    pub context_allowlist: BTreeSet<String>,
}

impl Default for ParentCapabilities {
    fn default() -> Self {
        Self {
            grants: BTreeSet::new(),
            budget: NodeBudget {
                max_tokens: u64::MAX,
                max_seconds: u64::MAX,
                max_tool_calls: u64::MAX,
            },
            context_allowlist: BTreeSet::new(),
        }
    }
}

impl ParentCapabilities {
    /// Родитель без ограничений по allowlist контекста: пустой список
    /// означает «ограничений сверху нет», иначе шаблон без явного родителя
    /// не смог бы объявить ни одного источника.
    pub fn unrestricted_context(mut self) -> Self {
        self.context_allowlist.clear();
        self
    }

    pub fn with_grants<I: IntoIterator<Item = &'static str>>(mut self, grants: I) -> Self {
        self.grants = grants.into_iter().map(str::to_string).collect();
        self
    }

    pub fn with_budget(mut self, budget: NodeBudget) -> Self {
        self.budget = budget;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingError {
    UnknownBlock {
        node_id: String,
        block_id: String,
    },
    BlockVersionMismatch {
        node_id: String,
        block_id: String,
        expected: u32,
        actual: u32,
    },
    BlockKindMismatch {
        node_id: String,
        block_id: String,
        expected: String,
        actual: String,
    },
    BlockSchemaMismatch {
        node_id: String,
        block_id: String,
        port: String,
    },
    UnknownTool {
        node_id: String,
        tool_name: String,
    },
    UnknownMcpServer {
        node_id: String,
        server_id: String,
    },
    McpToolNotAllowed {
        node_id: String,
        server_id: String,
        tool_name: String,
    },
    TransportUnavailable {
        node_id: String,
        server_id: String,
        transport: &'static str,
    },
    McpHostNotAllowed {
        node_id: String,
        server_id: String,
        host: String,
    },
    UnknownContextProvider {
        node_id: String,
        provider_id: String,
    },
    ProviderBudgetExceeded {
        node_id: String,
        provider_id: String,
        field: &'static str,
    },
    GrantNotGrantedByParent {
        node_id: String,
        grant: String,
    },
    ChildBudgetExceedsParent {
        node_id: String,
    },
    ContextNotAllowedByParent {
        node_id: String,
        item: String,
    },
    UnknownSubgraph {
        node_id: String,
        graph_id: String,
    },
    NestedSubgraph {
        node_id: String,
        graph_id: String,
    },
    ExpansionTooLarge {
        actual: usize,
        maximum: usize,
    },
}

impl BindingError {
    /// Стабильный bounded-код для IPC, событий и телеметрии.
    pub fn code(&self) -> &'static str {
        match self {
            BindingError::UnknownBlock { .. } => "unknown_block",
            BindingError::BlockVersionMismatch { .. } => "block_version_mismatch",
            BindingError::BlockKindMismatch { .. } => "block_kind_mismatch",
            BindingError::BlockSchemaMismatch { .. } => "block_schema_mismatch",
            BindingError::UnknownTool { .. } => "unknown_tool",
            BindingError::UnknownMcpServer { .. } => "unknown_mcp_server",
            BindingError::McpToolNotAllowed { .. } => "mcp_tool_not_allowed",
            BindingError::TransportUnavailable { .. } => "transport_unavailable",
            BindingError::McpHostNotAllowed { .. } => "mcp_host_not_allowed",
            BindingError::UnknownContextProvider { .. } => "unknown_context_provider",
            BindingError::ProviderBudgetExceeded { .. } => "provider_budget_exceeded",
            BindingError::GrantNotGrantedByParent { .. } => "grant_escalation",
            BindingError::ChildBudgetExceedsParent { .. } => "budget_escalation",
            BindingError::ContextNotAllowedByParent { .. } => "context_escalation",
            BindingError::UnknownSubgraph { .. } => "unknown_subgraph",
            BindingError::NestedSubgraph { .. } => "nested_subgraph",
            BindingError::ExpansionTooLarge { .. } => "expansion_too_large",
        }
    }
}

/// Core-owned реестр. `default()` возвращает поставляемый каталог.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowRegistry {
    blocks: BTreeMap<String, BlockDescriptor>,
    mcp_servers: BTreeMap<String, McpServerEntry>,
    providers: BTreeMap<String, ContextProviderEntry>,
    tools: BTreeSet<String>,
    subgraphs: BTreeMap<String, WorkflowGraph>,
    /// Host allowlist, уже применённый к `mcp.call`. `None` — переменная не
    /// задана, и Core полагается на SSRF-проверку самого инструмента.
    mcp_host_allowlist: Option<BTreeSet<String>>,
}

impl Default for WorkflowRegistry {
    fn default() -> Self {
        Self::bootstrap()
    }
}

impl WorkflowRegistry {
    /// Пустой реестр для тестов и для явного построения каталога.
    pub fn empty() -> Self {
        Self {
            blocks: BTreeMap::new(),
            mcp_servers: BTreeMap::new(),
            providers: BTreeMap::new(),
            tools: BTreeSet::new(),
            subgraphs: BTreeMap::new(),
            mcp_host_allowlist: env_host_allowlist(),
        }
    }

    /// Поставляемый каталог: инструменты, которые действительно
    /// зарегистрированы в `ToolRegistry::bootstrap`, три read-only
    /// контекстных провайдера и блоки для узлов, которым нужна стабильная
    /// identity.
    pub fn bootstrap() -> Self {
        let mut registry = Self::empty();
        for tool in [
            "workspace.list",
            "workspace.read",
            "workspace.write",
            "workspace.search",
            "git.status",
            "git.diff",
            "research.fetch",
            MCP_TOOL_NAME,
        ] {
            registry.tools.insert(tool.to_string());
        }

        registry.register_provider(ContextProviderEntry {
            provider_id: "workspace.knowledge".into(),
            display_name: "Знания рабочего каталога".into(),
            source: ContextSourceKind::WorkspaceKnowledge,
            max_items: 16,
            max_age_ms: MAX_FRESHNESS_MS,
            evidence_schema: EVIDENCE_SCHEMA.into(),
        });
        registry.register_provider(ContextProviderEntry {
            provider_id: "research.evidence".into(),
            display_name: "Сохранённые research evidence".into(),
            source: ContextSourceKind::ResearchEvidence,
            max_items: 16,
            max_age_ms: MAX_FRESHNESS_MS,
            evidence_schema: EVIDENCE_SCHEMA.into(),
        });
        registry.register_provider(ContextProviderEntry {
            provider_id: "task.memory".into(),
            display_name: "Память задачи".into(),
            source: ContextSourceKind::TaskMemory,
            max_items: 16,
            max_age_ms: MAX_FRESHNESS_MS,
            evidence_schema: EVIDENCE_SCHEMA.into(),
        });

        for block in bootstrap_blocks() {
            registry.register_block(block);
        }
        registry
    }

    pub fn register_block(&mut self, block: BlockDescriptor) {
        self.blocks.insert(block.block_id.clone(), block);
    }

    pub fn register_mcp_server(&mut self, entry: McpServerEntry) {
        self.mcp_servers.insert(entry.server_id.clone(), entry);
    }

    pub fn register_provider(&mut self, entry: ContextProviderEntry) {
        self.providers.insert(entry.provider_id.clone(), entry);
    }

    pub fn register_tool(&mut self, tool_name: &str) {
        self.tools.insert(tool_name.to_string());
    }

    /// Регистрирует Core-owned подграф. Подграф обязан быть валидным и не
    /// содержать собственных `subgraph`-узлов.
    pub fn register_subgraph(&mut self, graph: WorkflowGraph) {
        self.subgraphs.insert(graph.graph_id.clone(), graph);
    }

    pub fn set_mcp_host_allowlist(&mut self, hosts: Option<BTreeSet<String>>) {
        self.mcp_host_allowlist = hosts;
    }

    pub fn block(&self, block_id: &str) -> Option<&BlockDescriptor> {
        self.blocks.get(block_id)
    }

    pub fn blocks(&self) -> impl Iterator<Item = &BlockDescriptor> {
        self.blocks.values()
    }

    pub fn mcp_server(&self, server_id: &str) -> Option<&McpServerEntry> {
        self.mcp_servers.get(server_id)
    }

    /// Resolves the Core-owned MCP endpoint for a model/workflow identity.
    /// Callers must pass only `server_id` and `tool_name`; the endpoint is
    /// never accepted from model arguments.
    pub fn resolve_mcp_call(
        &self,
        server_id: &str,
        tool_name: &str,
    ) -> Result<String, BindingError> {
        let Some(server) = self.mcp_servers.get(server_id) else {
            return Err(BindingError::UnknownMcpServer {
                node_id: "model-call".into(),
                server_id: server_id.into(),
            });
        };
        if !server.allowed_tools.iter().any(|tool| tool == tool_name) {
            return Err(BindingError::McpToolNotAllowed {
                node_id: "model-call".into(),
                server_id: server_id.into(),
                tool_name: tool_name.into(),
            });
        }
        if !server.transport.is_available() {
            return Err(BindingError::TransportUnavailable {
                node_id: "model-call".into(),
                server_id: server_id.into(),
                transport: server.transport.as_str(),
            });
        }
        if let Some(host) = server.host() {
            if let Some(allowlist) = &self.mcp_host_allowlist {
                if !allowlist.contains(&host) {
                    return Err(BindingError::McpHostNotAllowed {
                        node_id: "model-call".into(),
                        server_id: server_id.into(),
                        host,
                    });
                }
            }
        }
        Ok(server.endpoint.clone())
    }

    pub fn mcp_servers(&self) -> impl Iterator<Item = &McpServerEntry> {
        self.mcp_servers.values()
    }

    pub fn provider(&self, provider_id: &str) -> Option<&ContextProviderEntry> {
        self.providers.get(provider_id)
    }

    pub fn providers(&self) -> impl Iterator<Item = &ContextProviderEntry> {
        self.providers.values()
    }

    pub fn subgraph(&self, graph_id: &str) -> Option<&WorkflowGraph> {
        self.subgraphs.get(graph_id)
    }

    /// Проверяет разрешимость всех идентичностей графа в стабильном порядке.
    pub fn validate_bindings(
        &self,
        graph: &WorkflowGraph,
        parent: &ParentCapabilities,
    ) -> Result<(), Vec<BindingError>> {
        let mut errors = Vec::new();
        let mut nodes: Vec<&WorkflowNode> = graph.nodes.iter().collect();
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        for node in nodes {
            self.validate_block(node, &mut errors);
            match &node.node_type {
                NodeType::Tool { tool } => {
                    if !self.tools.contains(&tool.tool_name) {
                        errors.push(BindingError::UnknownTool {
                            node_id: node.id.clone(),
                            tool_name: tool.tool_name.clone(),
                        });
                    }
                }
                NodeType::McpTool { mcp } => {
                    self.validate_mcp(&node.id, &mcp.server_id, &mcp.tool_name, &mut errors)
                }
                NodeType::ContextProvider { provider } => {
                    let Some(entry) = self.providers.get(&provider.provider_id) else {
                        errors.push(BindingError::UnknownContextProvider {
                            node_id: node.id.clone(),
                            provider_id: provider.provider_id.clone(),
                        });
                        continue;
                    };
                    if provider.max_items > entry.max_items {
                        errors.push(BindingError::ProviderBudgetExceeded {
                            node_id: node.id.clone(),
                            provider_id: provider.provider_id.clone(),
                            field: "max_items",
                        });
                    }
                    if provider.max_age_ms > entry.max_age_ms {
                        errors.push(BindingError::ProviderBudgetExceeded {
                            node_id: node.id.clone(),
                            provider_id: provider.provider_id.clone(),
                            field: "max_age_ms",
                        });
                    }
                }
                NodeType::Child { child } => {
                    self.validate_child(&node.id, child, parent, &mut errors)
                }
                NodeType::Subgraph { graph_id } => {
                    let Some(subgraph) = self.subgraphs.get(graph_id) else {
                        errors.push(BindingError::UnknownSubgraph {
                            node_id: node.id.clone(),
                            graph_id: graph_id.clone(),
                        });
                        continue;
                    };
                    if subgraph
                        .nodes
                        .iter()
                        .any(|inner| matches!(inner.node_type, NodeType::Subgraph { .. }))
                    {
                        errors.push(BindingError::NestedSubgraph {
                            node_id: node.id.clone(),
                            graph_id: graph_id.clone(),
                        });
                    }
                }
                NodeType::Research
                | NodeType::Transform
                | NodeType::Condition { .. }
                | NodeType::Approval
                | NodeType::Loop { .. } => {}
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn validate_block(&self, node: &WorkflowNode, errors: &mut Vec<BindingError>) {
        let Some(reference) = &node.block else {
            return;
        };
        let Some(descriptor) = self.blocks.get(&reference.block_id) else {
            errors.push(BindingError::UnknownBlock {
                node_id: node.id.clone(),
                block_id: reference.block_id.clone(),
            });
            return;
        };
        if descriptor.block_version != reference.block_version {
            errors.push(BindingError::BlockVersionMismatch {
                node_id: node.id.clone(),
                block_id: reference.block_id.clone(),
                expected: descriptor.block_version,
                actual: reference.block_version,
            });
            return;
        }
        if descriptor.action_kind != node.node_type.action_kind() {
            errors.push(BindingError::BlockKindMismatch {
                node_id: node.id.clone(),
                block_id: reference.block_id.clone(),
                expected: descriptor.action_kind.clone(),
                actual: node.node_type.action_kind().to_string(),
            });
            return;
        }
        for declared in &descriptor.inputs {
            match node.inputs.iter().find(|port| port.name == declared.name) {
                Some(port) if declared.matches(port) => {}
                _ => errors.push(BindingError::BlockSchemaMismatch {
                    node_id: node.id.clone(),
                    block_id: reference.block_id.clone(),
                    port: declared.name.clone(),
                }),
            }
        }
        for declared in &descriptor.outputs {
            match node.outputs.iter().find(|port| port.name == declared.name) {
                Some(port) if declared.matches(port) => {}
                _ => errors.push(BindingError::BlockSchemaMismatch {
                    node_id: node.id.clone(),
                    block_id: reference.block_id.clone(),
                    port: declared.name.clone(),
                }),
            }
        }
    }

    fn validate_mcp(
        &self,
        node_id: &str,
        server_id: &str,
        tool_name: &str,
        errors: &mut Vec<BindingError>,
    ) {
        let Some(entry) = self.mcp_servers.get(server_id) else {
            errors.push(BindingError::UnknownMcpServer {
                node_id: node_id.to_string(),
                server_id: server_id.to_string(),
            });
            return;
        };
        if !entry.transport.is_available() {
            errors.push(BindingError::TransportUnavailable {
                node_id: node_id.to_string(),
                server_id: server_id.to_string(),
                transport: entry.transport.as_str(),
            });
        }
        if !entry.allowed_tools.iter().any(|name| name == tool_name) {
            errors.push(BindingError::McpToolNotAllowed {
                node_id: node_id.to_string(),
                server_id: server_id.to_string(),
                tool_name: tool_name.to_string(),
            });
        }
        if let Some(allowlist) = &self.mcp_host_allowlist {
            let host = entry.host().unwrap_or_default();
            if !allowlist.contains(&host) {
                errors.push(BindingError::McpHostNotAllowed {
                    node_id: node_id.to_string(),
                    server_id: server_id.to_string(),
                    host,
                });
            }
        }
    }

    fn validate_child(
        &self,
        node_id: &str,
        child: &ChildActionProfile,
        parent: &ParentCapabilities,
        errors: &mut Vec<BindingError>,
    ) {
        for grant in &child.grants {
            if !parent.grants.contains(grant) {
                errors.push(BindingError::GrantNotGrantedByParent {
                    node_id: node_id.to_string(),
                    grant: grant.clone(),
                });
            }
        }
        if !child.budget.is_within(&parent.budget) {
            errors.push(BindingError::ChildBudgetExceedsParent {
                node_id: node_id.to_string(),
            });
        }
        if !parent.context_allowlist.is_empty() {
            for item in &child.context_allowlist {
                if !parent.context_allowlist.contains(item) {
                    errors.push(BindingError::ContextNotAllowedByParent {
                        node_id: node_id.to_string(),
                        item: item.clone(),
                    });
                }
            }
        }
    }

    /// Core-owned статическое разворачивание `subgraph`-узлов.
    ///
    /// Разворачивание выполняется до запуска и в пределах того же
    /// `ExecutionPolicy` родительского узла: у вложенных узлов не может быть
    /// собственного approval, timeout или retry шире родительского. Вложенные
    /// `subgraph` запрещены, поэтому рекурсия невозможна по построению.
    pub fn expand_subgraphs(
        &self,
        graph: &WorkflowGraph,
    ) -> Result<WorkflowGraph, Vec<BindingError>> {
        let mut errors = Vec::new();
        let mut nodes: Vec<WorkflowNode> = Vec::new();
        let mut edges: Vec<WorkflowEdge> = graph
            .edges
            .iter()
            .filter(|edge| {
                !is_subgraph(graph, &edge.from_node) && !is_subgraph(graph, &edge.to_node)
            })
            .cloned()
            .collect();
        let mut entry_node = graph.entry_node.clone();

        for node in &graph.nodes {
            let NodeType::Subgraph { graph_id } = &node.node_type else {
                nodes.push(node.clone());
                continue;
            };
            let Some(inner) = self.subgraphs.get(graph_id) else {
                errors.push(BindingError::UnknownSubgraph {
                    node_id: node.id.clone(),
                    graph_id: graph_id.clone(),
                });
                continue;
            };
            if inner
                .nodes
                .iter()
                .any(|item| matches!(item.node_type, NodeType::Subgraph { .. }))
            {
                errors.push(BindingError::NestedSubgraph {
                    node_id: node.id.clone(),
                    graph_id: graph_id.clone(),
                });
                continue;
            }
            let prefix = format!("{}::", node.id);
            for inner_node in &inner.nodes {
                let mut expanded = inner_node.clone();
                expanded.id = format!("{prefix}{}", inner_node.id);
                // Развёрнутые узлы наследуют политику родителя целиком:
                // подграф не может ослабить approval, таймаут или retry.
                expanded.execution = node.execution.clone();
                nodes.push(expanded);
            }
            for inner_edge in &inner.edges {
                let mut expanded = inner_edge.clone();
                expanded.from_node = format!("{prefix}{}", inner_edge.from_node);
                expanded.to_node = format!("{prefix}{}", inner_edge.to_node);
                edges.push(expanded);
            }
            let inner_entry = format!("{prefix}{}", inner.entry_node);
            if entry_node == node.id {
                entry_node = inner_entry.clone();
            }
            // Внешние рёбра переносятся на границы подграфа: вход — на его
            // entry node, выход — на узлы без исходящих рёбер.
            let sinks: Vec<String> = inner
                .nodes
                .iter()
                .filter(|item| !inner.edges.iter().any(|edge| edge.from_node == item.id))
                .map(|item| format!("{prefix}{}", item.id))
                .collect();
            for edge in &graph.edges {
                if edge.to_node == node.id {
                    let mut rewritten = edge.clone();
                    rewritten.to_node = inner_entry.clone();
                    edges.push(rewritten);
                }
                if edge.from_node == node.id {
                    for sink in &sinks {
                        let mut rewritten = edge.clone();
                        rewritten.from_node = sink.clone();
                        edges.push(rewritten);
                    }
                }
            }
        }

        if nodes.len() > MAX_GRAPH_NODES {
            errors.push(BindingError::ExpansionTooLarge {
                actual: nodes.len(),
                maximum: MAX_GRAPH_NODES,
            });
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        edges.sort_by(|left, right| {
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
        Ok(WorkflowGraph {
            contract: graph.contract.clone(),
            graph_id: graph.graph_id.clone(),
            version: graph.version,
            entry_node,
            nodes,
            edges,
            budget: graph.budget,
        })
    }
}

fn is_subgraph(graph: &WorkflowGraph, node_id: &str) -> bool {
    graph
        .node(node_id)
        .map(|node| matches!(node.node_type, NodeType::Subgraph { .. }))
        .unwrap_or(false)
}

fn env_host_allowlist() -> Option<BTreeSet<String>> {
    let raw = std::env::var(MCP_ALLOWED_HOSTS_ENV).ok()?;
    let hosts: BTreeSet<String> = raw
        .split(',')
        .map(|host| host.trim().to_ascii_lowercase())
        .filter(|host| !host.is_empty())
        .collect();
    if hosts.is_empty() {
        None
    } else {
        Some(hosts)
    }
}

/// Общая схема evidence контекстного провайдера. Она проверяется до включения
/// данных в контекст модели.
pub const EVIDENCE_SCHEMA: &str = r#"{"type":"object","required":["items"],"properties":{"items":{"type":"array","items":{"type":"object","required":["source_id","excerpt","captured_at_ms"],"properties":{"source_id":{"type":"string"},"excerpt":{"type":"string"},"captured_at_ms":{"type":"integer"}}}}}}"#;

fn bootstrap_blocks() -> Vec<BlockDescriptor> {
    vec![
        BlockDescriptor {
            block_id: "core.child.report".into(),
            block_version: 1,
            display_name: "Child с typed report".into(),
            description: "Child-узел, возвращающий typed report по контракту".into(),
            action_kind: "child".into(),
            inputs: vec![],
            outputs: vec![BlockPort {
                name: "report".into(),
                value_type: PortType::Json,
                required: false,
            }],
            fixture: BlockFixture {
                input_json: r#"{"input":"diff"}"#.into(),
                output_json: r#"{"status":"accepted"}"#.into(),
            },
        },
        BlockDescriptor {
            block_id: "core.context.provider".into(),
            block_version: 1,
            display_name: "Контекстный провайдер".into(),
            description: "Read-only выборка evidence с проверкой свежести".into(),
            action_kind: "context_provider".into(),
            inputs: vec![],
            outputs: vec![BlockPort {
                name: "evidence".into(),
                value_type: PortType::Json,
                required: false,
            }],
            fixture: BlockFixture {
                input_json: "{}".into(),
                output_json: r#"{"items":[]}"#.into(),
            },
        },
        BlockDescriptor {
            block_id: "core.mcp.call".into(),
            block_version: 1,
            display_name: "Вызов MCP-инструмента".into(),
            description: "Удалённый JSON-RPC вызов через Core-owned mcp.call".into(),
            action_kind: "mcp_tool".into(),
            inputs: vec![],
            outputs: vec![BlockPort {
                name: "result".into(),
                value_type: PortType::Json,
                required: false,
            }],
            fixture: BlockFixture {
                input_json: "{}".into(),
                output_json: r#"{"result":{}}"#.into(),
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ChildActionProfile, ExecutionPolicy, McpActionProfile,
        RetryPolicy, WorkflowBudget, WORKFLOW_CONTRACT_VERSION,
    };

    fn policy() -> ExecutionPolicy {
        ExecutionPolicy {
            retry: RetryPolicy {
                max_attempts: 1,
                backoff_ms: 0,
                retryable_errors: vec![],
            },
            timeout_ms: 1_000,
            cancellation: CancellationPolicy::Cooperative,
            approval: ApprovalPolicy {
                required: false,
                reason: None,
            },
        }
    }

    fn graph(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>, entry: &str) -> WorkflowGraph {
        WorkflowGraph {
            contract: WORKFLOW_CONTRACT_VERSION.into(),
            graph_id: "registry-test".into(),
            version: 1,
            entry_node: entry.into(),
            nodes,
            edges,
            budget: WorkflowBudget::default(),
        }
    }

    fn remote_server() -> McpServerEntry {
        McpServerEntry {
            server_id: "mcp.local-docs".into(),
            display_name: "Локальные документы".into(),
            transport: McpTransport::RemoteJsonRpc,
            endpoint: "https://docs.internal.test/rpc".into(),
            allowed_tools: vec!["search".into()],
        }
    }

    fn mcp_node(server_id: &str, tool_name: &str) -> WorkflowNode {
        WorkflowNode::new(
            "call",
            NodeType::McpTool {
                mcp: McpActionProfile {
                    server_id: server_id.into(),
                    tool_name: tool_name.into(),
                    arguments: BTreeMap::new(),
                },
            },
            policy(),
        )
        .with_output("result", PortType::Json)
    }

    #[test]
    fn unknown_server_and_tool_are_rejected_before_any_call() {
        let mut registry = WorkflowRegistry::empty();
        registry.set_mcp_host_allowlist(None);
        registry.register_mcp_server(remote_server());

        let errors = registry
            .validate_bindings(
                &graph(vec![mcp_node("mcp.other", "search")], vec![], "call"),
                &ParentCapabilities::default(),
            )
            .expect_err("unknown server");
        assert_eq!(errors[0].code(), "unknown_mcp_server");

        let errors = registry
            .validate_bindings(
                &graph(vec![mcp_node("mcp.local-docs", "write")], vec![], "call"),
                &ParentCapabilities::default(),
            )
            .expect_err("tool allowlist");
        assert_eq!(errors[0].code(), "mcp_tool_not_allowed");
    }

    #[test]
    fn unsupported_transport_is_valid_metadata_but_not_runnable() {
        let mut registry = WorkflowRegistry::empty();
        registry.set_mcp_host_allowlist(None);
        let mut stdio = remote_server();
        stdio.server_id = "mcp.stdio".into();
        stdio.transport = McpTransport::Stdio;
        registry.register_mcp_server(stdio);

        let errors = registry
            .validate_bindings(
                &graph(vec![mcp_node("mcp.stdio", "search")], vec![], "call"),
                &ParentCapabilities::default(),
            )
            .expect_err("transport");
        assert_eq!(errors[0].code(), "transport_unavailable");
    }

    #[test]
    fn host_outside_the_allowlist_is_rejected_before_dispatch() {
        let mut registry = WorkflowRegistry::empty();
        registry.register_mcp_server(remote_server());
        registry.set_mcp_host_allowlist(Some(BTreeSet::from(["allowed.test".to_string()])));

        let errors = registry
            .validate_bindings(
                &graph(vec![mcp_node("mcp.local-docs", "search")], vec![], "call"),
                &ParentCapabilities::default(),
            )
            .expect_err("host allowlist");
        assert_eq!(errors[0].code(), "mcp_host_not_allowed");

        registry.set_mcp_host_allowlist(Some(BTreeSet::from(["docs.internal.test".to_string()])));
        assert!(registry
            .validate_bindings(
                &graph(vec![mcp_node("mcp.local-docs", "search")], vec![], "call"),
                &ParentCapabilities::default(),
            )
            .is_ok());
    }

    #[test]
    fn child_cannot_raise_grants_or_budget_above_the_parent() {
        let registry = WorkflowRegistry::empty();
        let node = WorkflowNode::new(
            "child",
            NodeType::Child {
                child: ChildActionProfile {
                    role: "reviewer".into(),
                    goal: "review".into(),
                    output_schema: None,
                    context_allowlist: vec!["secrets.txt".into()],
                    artifact_allowlist: vec![],
                    grants: vec!["fs.write".into()],
                    budget: NodeBudget {
                        max_tokens: 1_000,
                        max_seconds: 10,
                        max_tool_calls: 5,
                    },
                    max_revisions: 1,
                },
            },
            policy(),
        );
        let parent = ParentCapabilities {
            grants: BTreeSet::from(["fs.read".to_string()]),
            budget: NodeBudget {
                max_tokens: 500,
                max_seconds: 5,
                max_tool_calls: 1,
            },
            context_allowlist: BTreeSet::from(["docs/readme.md".to_string()]),
        };
        let errors = registry
            .validate_bindings(&graph(vec![node], vec![], "child"), &parent)
            .expect_err("escalation");
        let codes: Vec<_> = errors.iter().map(BindingError::code).collect();
        assert!(codes.contains(&"grant_escalation"));
        assert!(codes.contains(&"budget_escalation"));
        assert!(codes.contains(&"context_escalation"));
    }

    #[test]
    fn provider_cannot_ask_for_more_items_or_staler_data_than_registered() {
        let mut registry = WorkflowRegistry::empty();
        registry.register_provider(ContextProviderEntry {
            provider_id: "workspace.knowledge".into(),
            display_name: "Знания".into(),
            source: ContextSourceKind::WorkspaceKnowledge,
            max_items: 4,
            max_age_ms: 60_000,
            evidence_schema: EVIDENCE_SCHEMA.into(),
        });
        let node = WorkflowNode::new(
            "context",
            NodeType::ContextProvider {
                provider: crate::workflow::ContextProviderProfile {
                    provider_id: "workspace.knowledge".into(),
                    query: "архитектура".into(),
                    max_items: 32,
                    max_age_ms: 3_600_000,
                    evidence_schema: None,
                },
            },
            policy(),
        )
        .with_output("evidence", PortType::Json);
        let errors = registry
            .validate_bindings(
                &graph(vec![node], vec![], "context"),
                &ParentCapabilities::default(),
            )
            .expect_err("provider budget");
        assert_eq!(errors.len(), 2);
        assert!(errors
            .iter()
            .all(|error| error.code() == "provider_budget_exceeded"));
    }

    #[test]
    fn block_version_and_schema_mismatch_stop_the_node() {
        let registry = WorkflowRegistry::bootstrap();
        let node = WorkflowNode::new(
            "context",
            NodeType::ContextProvider {
                provider: crate::workflow::ContextProviderProfile {
                    provider_id: "workspace.knowledge".into(),
                    query: String::new(),
                    max_items: 4,
                    max_age_ms: 60_000,
                    evidence_schema: None,
                },
            },
            policy(),
        )
        .with_output("evidence", PortType::Json)
        .with_block("core.context.provider", 2);
        let errors = registry
            .validate_bindings(
                &graph(vec![node], vec![], "context"),
                &ParentCapabilities::default(),
            )
            .expect_err("version");
        assert_eq!(errors[0].code(), "block_version_mismatch");

        let node = WorkflowNode::new(
            "context",
            NodeType::ContextProvider {
                provider: crate::workflow::ContextProviderProfile {
                    provider_id: "workspace.knowledge".into(),
                    query: String::new(),
                    max_items: 4,
                    max_age_ms: 60_000,
                    evidence_schema: None,
                },
            },
            policy(),
        )
        .with_output("evidence", PortType::Text)
        .with_block("core.context.provider", 1);
        let errors = registry
            .validate_bindings(
                &graph(vec![node], vec![], "context"),
                &ParentCapabilities::default(),
            )
            .expect_err("schema");
        assert_eq!(errors[0].code(), "block_schema_mismatch");
    }

    #[test]
    fn bootstrap_block_fixtures_are_valid_json_and_uniquely_versioned() {
        let registry = WorkflowRegistry::bootstrap();
        let mut seen = BTreeSet::new();
        for block in registry.blocks() {
            assert!(seen.insert(block.block_id.clone()), "duplicate block id");
            assert!(block.block_version > 0);
            serde_json::from_str::<serde_json::Value>(&block.fixture.input_json)
                .expect("fixture input is JSON");
            serde_json::from_str::<serde_json::Value>(&block.fixture.output_json)
                .expect("fixture output is JSON");
        }
    }

    #[test]
    fn subgraph_expansion_inherits_parent_policy_and_forbids_nesting() {
        let mut registry = WorkflowRegistry::empty();
        registry.register_tool("workspace.read");
        let inner = graph(
            vec![
                WorkflowNode::new("first", NodeType::Transform, policy())
                    .with_output("out", PortType::Text),
                WorkflowNode::new("second", NodeType::Transform, policy()).with_input(
                    "in",
                    PortType::Text,
                    true,
                ),
            ],
            vec![WorkflowEdge::data("first", "out", "second", "in")],
            "first",
        );
        let mut inner = inner;
        inner.graph_id = "inner.graph".into();
        registry.register_subgraph(inner);

        let mut strict = policy();
        strict.approval.required = true;
        let outer = graph(
            vec![
                WorkflowNode::new(
                    "expand",
                    NodeType::Subgraph {
                        graph_id: "inner.graph".into(),
                    },
                    strict,
                )
                .with_output("out", PortType::Text),
                WorkflowNode::new("sink", NodeType::Transform, policy()).with_input(
                    "in",
                    PortType::Text,
                    true,
                ),
            ],
            vec![WorkflowEdge::data("expand", "out", "sink", "in")],
            "expand",
        );
        let expanded = registry.expand_subgraphs(&outer).expect("expansion");
        assert_eq!(expanded.entry_node, "expand::first");
        assert!(expanded
            .nodes
            .iter()
            .all(|node| !matches!(node.node_type, NodeType::Subgraph { .. })));
        for node in expanded.nodes.iter().filter(|node| node.id.contains("::")) {
            assert!(
                node.execution.approval.required,
                "развёрнутый узел обязан наследовать approval родителя"
            );
        }

        let nested_child = graph(
            vec![WorkflowNode::new(
                "nested",
                NodeType::Subgraph {
                    graph_id: "inner.graph".into(),
                },
                policy(),
            )],
            vec![],
            "nested",
        );
        let mut nested_child = nested_child;
        nested_child.graph_id = "nested.graph".into();
        registry.register_subgraph(nested_child);
        let outer = graph(
            vec![WorkflowNode::new(
                "expand",
                NodeType::Subgraph {
                    graph_id: "nested.graph".into(),
                },
                policy(),
            )],
            vec![],
            "expand",
        );
        let errors = registry.expand_subgraphs(&outer).expect_err("nested");
        assert_eq!(errors[0].code(), "nested_subgraph");
    }
}
