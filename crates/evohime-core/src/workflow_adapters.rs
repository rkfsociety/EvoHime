//! Адаптеры узлов workflow к реальному Core execution path (план 06.2).
//!
//! Каждый профиль узла ведёт в уже существующий контур Core, а не в новый
//! runtime:
//!
//! | профиль            | путь исполнения                                    |
//! |--------------------|----------------------------------------------------|
//! | `child`            | `child_contracts::TypedChildTaskRequest` и report   |
//! | `tool`             | `ToolRegistry` с существующим approval path         |
//! | `mcp_tool`         | тот же `ToolRegistry`, Core-owned tool `mcp.call`   |
//! | `context_provider` | read-only источники с проверкой свежести            |
//! | `research`         | сохранённые research evidence                       |
//! | `condition`/`transform` | deterministic-операции Core                   |
//!
//! Approval-узел сюда не попадает: его решает runtime через тот же approval
//! registry, что и обычные инструменты.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::workflow::{ConditionMode, NodeType};
use crate::workflow_registry::ContextSourceKind;
use crate::workflow_runtime::{NodeAdapter, NodeError, NodeFuture, NodeInvocation, NodeSuccess};
use crate::EventJournal;

use evohime_tool_runtime::{ToolContext, ToolError, ToolRegistry};

/// Потолок текста, попадающего в результат узла. Он же ограничивает объём,
/// который может уйти дальше по графу.
pub const MAX_ADAPTER_OUTPUT_CHARS: usize = 8 * 1024;

/// Результат child-узла. Родитель принимает только typed report; произвольный
/// текст модели наружу не выходит.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildOutcome {
    pub status: String,
    pub summary: String,
    pub findings: Vec<String>,
    pub evidence: u32,
}

pub type ChildFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<ChildOutcome, NodeError>> + Send + 'a>,
>;

/// Исполнитель child-узла. Он получает уже проверенный typed-запрос: grants,
/// бюджет, allowlist и схема выхода зафиксированы контрактом до вызова.
pub trait ChildExecutor: Send + Sync {
    fn run<'a>(
        &'a self,
        request: &'a crate::child_contracts::TypedChildTaskRequest,
    ) -> ChildFuture<'a>;
}

/// Исполнитель по умолчанию: child-контур не подключён.
///
/// Он не притворяется успешным и не выдаёт пустой ответ за результат — узел
/// честно помечается вырожденным с явной причиной, как требует решение 6
/// плана 06-0.
pub struct UnavailableChildExecutor;

impl ChildExecutor for UnavailableChildExecutor {
    fn run<'a>(
        &'a self,
        request: &'a crate::child_contracts::TypedChildTaskRequest,
    ) -> ChildFuture<'a> {
        let role = request.role.clone();
        Box::pin(async move {
            Ok(ChildOutcome {
                status: "degraded".into(),
                summary: format!("исполнитель роли {role} недоступен в этой сборке"),
                findings: Vec::new(),
                evidence: 0,
            })
        })
    }
}

/// Всё, что адаптеру нужно от Core. Поля опциональны: отсутствие реестра
/// инструментов не должно валить весь запуск, но и не должно выдавать
/// отсутствие результата за успех.
pub struct CoreNodeAdapter {
    adapter_descriptor: crate::adapter_contract::AdapterDescriptor,
    adapter_session: crate::adapter_contract::AdapterSession,
    journal: EventJournal,
    tools: Option<Arc<ToolRegistry>>,
    workspace_root: std::path::PathBuf,
    task_id: uuid::Uuid,
    session_id: Option<uuid::Uuid>,
    child_executor: Arc<dyn ChildExecutor>,
}

impl CoreNodeAdapter {
    pub fn new(journal: EventJournal, workspace_root: impl Into<std::path::PathBuf>) -> Self {
        let workspace_root = workspace_root.into();
        Self {
            adapter_descriptor: crate::adapter_contract::AdapterDescriptor::builtin_tool(),
            adapter_session: crate::adapter_contract::AdapterSession {
                negotiated_capabilities: vec!["tool_calling".into()],
                policy_hash: "workflow-policy-v1".into(),
                target_generation: 0,
                workspace_scope: workspace_root.to_string_lossy().into_owned(),
                deadline_ms: 30_000,
                cancellation_requested: false,
                secret_ref: None,
            },
            journal,
            tools: None,
            workspace_root,
            task_id: uuid::Uuid::new_v4(),
            session_id: None,
            child_executor: Arc::new(UnavailableChildExecutor),
        }
    }

    pub fn with_tools(mut self, tools: Arc<ToolRegistry>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn with_task(mut self, task_id: uuid::Uuid, session_id: Option<uuid::Uuid>) -> Self {
        self.task_id = task_id;
        self.session_id = session_id;
        self
    }

    pub fn with_child_executor(mut self, executor: Arc<dyn ChildExecutor>) -> Self {
        self.child_executor = executor;
        self
    }

    fn tool_context(&self) -> ToolContext {
        ToolContext {
            workspace_root: self.workspace_root.clone(),
            task_id: self.task_id,
            session_id: self.session_id,
            progress_tx: None,
        }
    }

    async fn run_tool(&self, tool_name: &str, input: Value) -> Result<Value, NodeError> {
        self.adapter_descriptor
            .validate()
            .map_err(|error| NodeError::permanent("adapter_unavailable", error.to_string()))?;
        self.adapter_session
            .validate(&self.adapter_descriptor)
            .map_err(|error| NodeError::permanent("adapter_session_rejected", error.to_string()))?;
        let Some(tools) = &self.tools else {
            return Err(NodeError::permanent(
                "tool_registry_unavailable",
                "реестр инструментов не подключён",
            ));
        };
        let context = self.tool_context();
        let input_bytes = serde_json::to_vec(&input).map_err(|_| {
            NodeError::permanent("adapter_invalid_input", "вход адаптера не сериализуется")
        })?;
        crate::adapter_contract::validate_request(&crate::adapter_contract::AdapterRequest {
            correlation_id: format!("{}:{tool_name}", context.task_id),
            input: input_bytes,
        })
        .map_err(|error| NodeError::permanent("adapter_input_rejected", error.to_string()))?;
        let scope = input
            .get("path")
            .or_else(|| input.get("cwd"))
            .or_else(|| input.get("url"))
            .and_then(Value::as_str)
            .unwrap_or("workspace");
        let action_id = uuid::Uuid::now_v7();
        let snapshot = crate::policy_gate::default_snapshot(
            action_id,
            context.task_id,
            context.session_id,
            tool_name,
            scope,
        )
        .map_err(|error| NodeError::permanent("policy_error", error))?;
        let gate = crate::policy_gate::PolicyGate::new(snapshot)
            .map_err(|decision| NodeError::permanent("policy_error", decision.reason_code))?;
        let binding = gate
            .preflight(
                &action_id.to_string(),
                tool_name,
                scope,
                &input,
                evohime_receipts::capability::PolicyOutcome::Allowed,
            )
            .map_err(|decision| NodeError::permanent("policy_error", decision.reason_code))?;
        gate.recheck_before_effect(
            &binding,
            tool_name,
            scope,
            &input,
            evohime_receipts::capability::PolicyOutcome::Allowed,
        )
        .map_err(|decision| NodeError::permanent("policy_error", decision.reason_code))?;
        match tools.execute(&context, tool_name, input).await {
            Ok(result) => {
                let output = json!({
                    "output": bounded(&result.output),
                    "structured": result.structured,
                });
                let output_bytes = serde_json::to_vec(&output).map_err(|_| {
                    NodeError::permanent(
                        "adapter_invalid_output",
                        "выход адаптера не сериализуется",
                    )
                })?;
                crate::adapter_contract::validate_result(&crate::adapter_contract::AdapterResult {
                    correlation_id: format!("{}:{tool_name}", context.task_id),
                    status: crate::adapter_contract::AdapterStatus::Success,
                    output: output_bytes,
                    diagnostic: String::new(),
                })
                .map_err(|error| {
                    NodeError::permanent("adapter_output_rejected", error.to_string())
                })?;
                Ok(output)
            }
            Err(error) => Err(map_tool_error(error)),
        }
    }
}

fn bounded(text: &str) -> String {
    text.chars().take(MAX_ADAPTER_OUTPUT_CHARS).collect()
}

/// Ошибки инструментов переводятся в классы ошибок узла. Approval и
/// permission-denied повторять нельзя: это решение, а не сбой.
fn map_tool_error(error: ToolError) -> NodeError {
    match error {
        ToolError::TimedOut(_) => NodeError::transient("timeout", "инструмент не уложился в срок"),
        ToolError::Execution(message) => {
            NodeError::transient("execution_failed", bounded(&message))
        }
        ToolError::UnknownTool(name) => NodeError::permanent(
            "unknown_tool",
            format!("инструмент {name} не зарегистрирован"),
        ),
        ToolError::InvalidInput { message, .. } => {
            NodeError::permanent("invalid_input", bounded(&message))
        }
        ToolError::PermissionDenied(permission) => NodeError::permanent(
            "permission_denied",
            format!("право {permission:?} не выдано"),
        ),
        ToolError::NeedsApproval(_) => NodeError::permanent(
            "approval_required",
            "инструменту требуется подтверждение, объявите approval у узла",
        ),
        ToolError::ApprovalMismatch => NodeError::permanent(
            "approval_mismatch",
            "подтверждение относится к другому вызову",
        ),
        ToolError::ApprovalDenied => {
            NodeError::permanent("approval_denied", "подтверждение отклонено")
        }
        ToolError::NotFound { .. } => NodeError::permanent("not_found", "объект не найден"),
    }
}

/// Подставляет значения входных портов в статические аргументы: `$port`
/// заменяется значением одноимённого входа. Ничего, кроме объявленных
/// входов, подставить нельзя.
fn resolve_arguments(
    arguments: &BTreeMap<String, String>,
    inputs: &BTreeMap<String, Value>,
) -> Value {
    let mut object = serde_json::Map::new();
    for (key, value) in arguments {
        let resolved = if let Some(port) = value.strip_prefix('$') {
            inputs.get(port).cloned().unwrap_or(Value::Null)
        } else {
            Value::String(value.clone())
        };
        object.insert(key.clone(), resolved);
    }
    Value::Object(object)
}

impl NodeAdapter for CoreNodeAdapter {
    fn execute<'a>(&'a self, invocation: NodeInvocation<'a>) -> NodeFuture<'a> {
        Box::pin(async move {
            match &invocation.node.node_type {
                NodeType::Transform => Ok(NodeSuccess::new(json!({
                    "out": merge_inputs(&invocation.inputs),
                    "text": summarize_inputs(&invocation.inputs),
                }))),
                NodeType::Condition { mode } => {
                    let values: Vec<bool> = invocation
                        .inputs
                        .values()
                        .map(|value| value.as_bool().unwrap_or(false))
                        .collect();
                    let decided = match mode {
                        ConditionMode::All => !values.is_empty() && values.iter().all(|v| *v),
                        ConditionMode::Any => values.iter().any(|v| *v),
                    };
                    Ok(NodeSuccess::new(json!({ "out": decided })))
                }
                NodeType::Approval => Ok(NodeSuccess::new(json!({
                    "approved": true,
                    "out": merge_inputs(&invocation.inputs),
                }))),
                NodeType::Loop { max_iterations } => Ok(NodeSuccess::new(json!({
                    "out": merge_inputs(&invocation.inputs),
                    "max_iterations": max_iterations,
                }))),
                NodeType::Tool { tool } => {
                    let input = resolve_arguments(&tool.arguments, &invocation.inputs);
                    let output = self.run_tool(&tool.tool_name, input).await?;
                    Ok(NodeSuccess::new(json!({ "out": output, "result": output })))
                }
                NodeType::McpTool { mcp } => {
                    // Идентичность сервера и его endpoint приходят только из
                    // Core-owned реестра. Граф даёт лишь имя записи.
                    let Some(entry) = invocation.registry.mcp_server(&mcp.server_id) else {
                        return Err(NodeError::permanent(
                            "unknown_mcp_server",
                            "сервер не зарегистрирован",
                        ));
                    };
                    if !entry.transport.is_available() {
                        return Err(NodeError::permanent(
                            "transport_unavailable",
                            format!("транспорт {} недоступен", entry.transport.as_str()),
                        ));
                    }
                    if !entry
                        .allowed_tools
                        .iter()
                        .any(|name| name == &mcp.tool_name)
                    {
                        return Err(NodeError::permanent(
                            "mcp_tool_not_allowed",
                            "инструмент не входит в allowlist сервера",
                        ));
                    }
                    let params = resolve_arguments(&mcp.arguments, &invocation.inputs);
                    let output = self
                        .run_tool(
                            crate::workflow_registry::MCP_TOOL_NAME,
                            json!({
                                "url": entry.endpoint,
                                "method": mcp.tool_name,
                                "params": params,
                            }),
                        )
                        .await?;
                    Ok(NodeSuccess::new(json!({ "out": output, "result": output })))
                }
                NodeType::IntegrationAction { integration } => {
                    let input = merge_inputs(&invocation.inputs);
                    let outcome = crate::integration_provider_runtime::invoke_fixture(
                        &integration.provider_id,
                        &integration.action_id,
                        input,
                    );
                    match outcome {
                        crate::integration_provider_runtime::ProviderOutcome::Success {
                            result,
                        } => Ok(NodeSuccess::new(json!({ "out": result, "result": result }))),
                        crate::integration_provider_runtime::ProviderOutcome::Unavailable {
                            reason,
                        } => Err(NodeError::permanent("provider_adapter_unavailable", reason)),
                        crate::integration_provider_runtime::ProviderOutcome::Unknown {
                            effect_id,
                        } => Err(NodeError::permanent("unknown_outcome", effect_id)),
                    }
                }
                NodeType::ContextProvider { provider } => {
                    let Some(entry) = invocation.registry.provider(&provider.provider_id) else {
                        return Err(NodeError::permanent(
                            "unknown_context_provider",
                            "провайдер не зарегистрирован",
                        ));
                    };
                    let now_ms = crate::task_memory::now_millis() as i64;
                    let items = self
                        .collect_evidence(entry.source, &provider.query, provider.max_items)
                        .await;
                    match items {
                        Err(reason) => Ok(NodeSuccess::new(json!({
                            "out": {"items": []},
                            "evidence": {"items": []},
                            "reason": reason,
                        }))
                        .degraded("degraded")),
                        Ok(items) => {
                            // Устаревшее evidence не превращается в уверенный
                            // ответ: оно отбрасывается, а узел честно
                            // помечается вырожденным.
                            let fresh: Vec<Value> = items
                                .iter()
                                .filter(|item| {
                                    item.get("captured_at_ms")
                                        .and_then(Value::as_i64)
                                        .map(|captured| {
                                            now_ms.saturating_sub(captured)
                                                <= provider.max_age_ms as i64
                                        })
                                        .unwrap_or(false)
                                })
                                .cloned()
                                .collect();
                            let stale = fresh.len() < items.len();
                            let payload = json!({
                                "out": {"items": fresh.clone()},
                                "evidence": {"items": fresh.clone()},
                                "source": entry.source.as_str(),
                            });
                            let success =
                                NodeSuccess::new(payload).with_evidence(fresh.len() as u32);
                            if fresh.is_empty() || stale {
                                Ok(success.degraded("degraded"))
                            } else {
                                Ok(success)
                            }
                        }
                    }
                }
                NodeType::Research => {
                    let items = self
                        .collect_evidence(ContextSourceKind::ResearchEvidence, "", 8)
                        .await
                        .unwrap_or_default();
                    let success = NodeSuccess::new(json!({
                        "out": {"items": items.clone()},
                        "evidence": {"items": items.clone()},
                    }))
                    .with_evidence(items.len() as u32);
                    if items.is_empty() {
                        Ok(success.degraded("degraded"))
                    } else {
                        Ok(success)
                    }
                }
                NodeType::Subgraph { .. } => Err(NodeError::permanent(
                    "subgraph_not_expanded",
                    "подграф обязан быть развёрнут до запуска",
                )),
                NodeType::Child { child } => {
                    let request = self.build_child_request(&invocation, child)?;
                    let outcome = self.child_executor.run(&request).await?;
                    let degraded = outcome.status == "degraded";
                    let success = NodeSuccess::new(json!({
                        "out": {
                            "status": outcome.status,
                            "summary": bounded(&outcome.summary),
                            "findings": outcome.findings,
                        },
                        "report": {
                            "status": outcome.status,
                            "summary": bounded(&outcome.summary),
                            "findings": outcome.findings,
                        },
                    }))
                    .with_evidence(outcome.evidence);
                    if degraded {
                        Ok(success.degraded("degraded"))
                    } else {
                        Ok(success)
                    }
                }
            }
        })
    }
}

impl CoreNodeAdapter {
    /// Строит typed child request и проверяет, что он остаётся подмножеством
    /// родительских возможностей. Вложенная делегация запрещена явно.
    fn build_child_request(
        &self,
        invocation: &NodeInvocation<'_>,
        child: &crate::workflow::ChildActionProfile,
    ) -> Result<crate::child_contracts::TypedChildTaskRequest, NodeError> {
        use crate::child_contracts::{
            ChildBudget, CorrelationContext, CorrelationId, Grant, Schema,
        };

        let child_id = CorrelationId::new(format!(
            "{}:{}",
            invocation.context.workflow_run_id, invocation.context.node_id
        ))
        .map_err(|error| NodeError::permanent("invalid_child_id", error.to_string()))?;
        let task_id = CorrelationId::new(invocation.context.workflow_run_id.clone())
            .map_err(|error| NodeError::permanent("invalid_task_id", error.to_string()))?;
        let correlation = CorrelationContext::new(task_id, child_id.clone(), 0);

        let mut request = crate::child_contracts::TypedChildTaskRequest::new(
            child_id.as_str(),
            &invocation.context.workflow_run_id,
            &child.role,
            child.goal.chars().take(512).collect::<String>(),
            correlation,
        )
        .map_err(|error| NodeError::permanent("invalid_child_request", error.to_string()))?;

        let mut grants = Vec::new();
        for grant in &child.grants {
            let parsed = match grant.split_once(':') {
                Some((kind, scope)) => {
                    Grant::new(kind)
                        .and_then(|grant| grant.with_scope(scope))
                        .map_err(|error| NodeError::permanent("invalid_grant", error.to_string()))?
                }
                None => Grant::new(grant)
                    .map_err(|error| NodeError::permanent("invalid_grant", error.to_string()))?,
            };
            if !invocation.parent.grants.contains(grant) {
                return Err(NodeError::permanent(
                    "grant_escalation",
                    format!("родитель не выдавал право {grant}"),
                ));
            }
            grants.push(parsed);
        }
        request = request
            .with_grants(grants)
            .map_err(|error| NodeError::permanent("invalid_grant", error.to_string()))?;
        request = request
            .with_context(child.context_allowlist.clone())
            .map_err(|error| NodeError::permanent("invalid_context", error.to_string()))?;
        request = request.with_budget(
            ChildBudget::new()
                .with_tokens(child.budget.max_tokens)
                .with_time(child.budget.max_seconds)
                .with_tool_calls(child.budget.max_tool_calls),
        );
        if let Some(schema) = &child.output_schema {
            let schema = Schema::new()
                .with_json_schema(schema.clone())
                .map_err(|error| NodeError::permanent("invalid_schema", error.to_string()))?;
            request = request.with_output_schema(schema);
        }
        request = request.with_max_revisions(child.max_revisions);
        // Вложенная делегация невозможна: узел workflow никогда не объявляет
        // себя child, поэтому дочерний запрос не может породить собственный.
        request.parent_is_child = false;
        request
            .validate()
            .map_err(|error| NodeError::permanent("invalid_child_request", error.to_string()))?;
        Ok(request)
    }

    /// Read-only выборка evidence. `Err` означает недоступный источник, а не
    /// пустой результат: разница видна в состоянии узла.
    async fn collect_evidence(
        &self,
        source: ContextSourceKind,
        query: &str,
        max_items: u32,
    ) -> Result<Vec<Value>, String> {
        let limit = max_items.max(1) as usize;
        match source {
            ContextSourceKind::WorkspaceKnowledge => {
                let result = self
                    .journal
                    .search_workspace_knowledge(
                        &self.workspace_root,
                        query,
                        crate::workspace_rag::QueryFilters {
                            path: None,
                            language: None,
                        },
                        false,
                    )
                    .await
                    .map_err(|error| format!("{error:?}"))?;
                let now_ms = crate::task_memory::now_millis() as i64;
                Ok(result
                    .evidence
                    .into_iter()
                    .filter(|chunk| !chunk.stale)
                    .take(limit)
                    .map(|chunk| {
                        json!({
                            "source_id": chunk.source_id,
                            "excerpt": bounded(chunk.content.as_deref().unwrap_or_default()),
                            "captured_at_ms": now_ms,
                            "path": chunk.relative_path,
                        })
                    })
                    .collect())
            }
            ContextSourceKind::ResearchEvidence => {
                let records = self
                    .journal
                    .list_research_evidence(query)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(records
                    .into_iter()
                    .take(limit)
                    .map(|record| {
                        json!({
                            "source_id": record.source_ref,
                            "excerpt": bounded(&record.redacted_excerpt),
                            "captured_at_ms": parse_timestamp_ms(&record.fetched_at),
                        })
                    })
                    .collect())
            }
            ContextSourceKind::TaskMemory => {
                let now = chrono::Utc::now().to_rfc3339();
                let records = self
                    .journal
                    .search_memory(
                        evohime_local_storage::memory_store::MemoryScope::Project,
                        &self.workspace_root.to_string_lossy(),
                        query,
                        &now,
                        limit as u32,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                let now_ms = crate::task_memory::now_millis() as i64;
                Ok(records
                    .into_iter()
                    .take(limit)
                    .map(|record| {
                        json!({
                            "source_id": record.id,
                            "excerpt": bounded(&record.content),
                            "captured_at_ms": now_ms,
                        })
                    })
                    .collect())
            }
        }
    }
}

fn parse_timestamp_ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|stamp| stamp.timestamp_millis())
        .unwrap_or_default()
}

fn merge_inputs(inputs: &BTreeMap<String, Value>) -> Value {
    if inputs.len() == 1 {
        return inputs.values().next().cloned().unwrap_or(Value::Null);
    }
    Value::Object(inputs.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

fn summarize_inputs(inputs: &BTreeMap<String, Value>) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (key, value) in inputs {
        let rendered = match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        parts.push(format!("{key}={rendered}"));
    }
    bounded(&parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::{
        ApprovalPolicy, CancellationPolicy, ChildActionProfile, ExecutionPolicy, McpActionProfile,
        NodeBudget, NodeExecutionContext, RetryPolicy, ToolActionProfile, WorkflowNode,
    };
    use crate::workflow_registry::{ParentCapabilities, WorkflowRegistry};
    use std::collections::BTreeSet;

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

    fn adapter() -> (CoreNodeAdapter, tempfile::TempDir) {
        let directory = tempfile::tempdir().expect("temp dir");
        let journal = EventJournal::open(directory.path().join("core.db")).expect("journal");
        let adapter = CoreNodeAdapter::new(journal, directory.path());
        (adapter, directory)
    }

    fn invocation<'a>(
        node: &'a WorkflowNode,
        registry: &'a WorkflowRegistry,
        parent: &'a ParentCapabilities,
        inputs: BTreeMap<String, Value>,
    ) -> NodeInvocation<'a> {
        NodeInvocation {
            context: NodeExecutionContext {
                workflow_run_id: "run-1".into(),
                node_id: node.id.clone(),
                attempt_id: "run-1:node:1".into(),
                graph_hash: "hash".into(),
            },
            node,
            inputs,
            parent,
            registry,
        }
    }

    #[tokio::test]
    async fn a_condition_node_decides_deterministically_by_mode() {
        let (adapter, _dir) = adapter();
        let registry = WorkflowRegistry::empty();
        let parent = ParentCapabilities::default();
        let inputs = BTreeMap::from([
            ("left".to_string(), json!(true)),
            ("right".to_string(), json!(false)),
        ]);

        let all = WorkflowNode::new(
            "condition",
            NodeType::Condition {
                mode: ConditionMode::All,
            },
            policy(),
        );
        let result = adapter
            .execute(invocation(&all, &registry, &parent, inputs.clone()))
            .await
            .expect("all");
        assert_eq!(result.output["out"], json!(false));

        let any = WorkflowNode::new(
            "condition",
            NodeType::Condition {
                mode: ConditionMode::Any,
            },
            policy(),
        );
        let result = adapter
            .execute(invocation(&any, &registry, &parent, inputs))
            .await
            .expect("any");
        assert_eq!(result.output["out"], json!(true));
    }

    #[tokio::test]
    async fn a_tool_node_without_a_registry_fails_instead_of_pretending_success() {
        let (adapter, _dir) = adapter();
        let registry = WorkflowRegistry::empty();
        let parent = ParentCapabilities::default();
        let node = WorkflowNode::new(
            "tool",
            NodeType::Tool {
                tool: ToolActionProfile {
                    tool_name: "workspace.read".into(),
                    arguments: BTreeMap::new(),
                },
            },
            policy(),
        );
        let error = adapter
            .execute(invocation(&node, &registry, &parent, BTreeMap::new()))
            .await
            .expect_err("no registry");
        assert_eq!(error.code, "tool_registry_unavailable");
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn an_mcp_node_refuses_a_tool_outside_the_server_allowlist() {
        let (adapter, _dir) = adapter();
        let mut registry = WorkflowRegistry::empty();
        registry.set_mcp_host_allowlist(None);
        registry.register_mcp_server(crate::workflow_registry::McpServerEntry {
            server_id: "mcp.docs".into(),
            display_name: "Документы".into(),
            transport: crate::workflow_registry::McpTransport::RemoteJsonRpc,
            endpoint: "https://docs.test/rpc".into(),
            allowed_tools: vec!["search".into()],
        });
        let parent = ParentCapabilities::default();
        let node = WorkflowNode::new(
            "call",
            NodeType::McpTool {
                mcp: McpActionProfile {
                    server_id: "mcp.docs".into(),
                    tool_name: "write".into(),
                    arguments: BTreeMap::new(),
                },
            },
            policy(),
        );
        let error = adapter
            .execute(invocation(&node, &registry, &parent, BTreeMap::new()))
            .await
            .expect_err("allowlist");
        assert_eq!(error.code, "mcp_tool_not_allowed");
    }

    #[tokio::test]
    async fn a_child_node_cannot_take_a_grant_the_parent_never_had() {
        let (adapter, _dir) = adapter();
        let registry = WorkflowRegistry::empty();
        let parent = ParentCapabilities {
            grants: BTreeSet::from(["fs.read".to_string()]),
            budget: NodeBudget::default(),
            context_allowlist: BTreeSet::new(),
        };
        let node = WorkflowNode::new(
            "child",
            NodeType::Child {
                child: ChildActionProfile {
                    role: "reviewer".into(),
                    goal: "проверь".into(),
                    output_schema: None,
                    context_allowlist: vec![],
                    artifact_allowlist: vec![],
                    grants: vec!["fs.write".into()],
                    budget: NodeBudget::default(),
                    max_revisions: 1,
                },
            },
            policy(),
        );
        let error = adapter
            .execute(invocation(&node, &registry, &parent, BTreeMap::new()))
            .await
            .expect_err("escalation");
        assert_eq!(error.code, "grant_escalation");
    }

    #[tokio::test]
    async fn an_unavailable_child_executor_returns_degraded_not_success() {
        let (adapter, _dir) = adapter();
        let registry = WorkflowRegistry::empty();
        let parent = ParentCapabilities {
            grants: BTreeSet::from(["fs.read".to_string()]),
            budget: NodeBudget::default(),
            context_allowlist: BTreeSet::new(),
        };
        let node = WorkflowNode::new(
            "child",
            NodeType::Child {
                child: ChildActionProfile {
                    role: "reviewer".into(),
                    goal: "проверь".into(),
                    output_schema: None,
                    context_allowlist: vec![],
                    artifact_allowlist: vec![],
                    grants: vec!["fs.read".into()],
                    budget: NodeBudget::default(),
                    max_revisions: 1,
                },
            },
            policy(),
        );
        let result = adapter
            .execute(invocation(&node, &registry, &parent, BTreeMap::new()))
            .await
            .expect("degraded result");
        assert!(result.degraded);
        assert_eq!(result.status, "degraded");
    }

    #[tokio::test]
    async fn an_unregistered_context_provider_is_refused() {
        let (adapter, _dir) = adapter();
        let registry = WorkflowRegistry::empty();
        let parent = ParentCapabilities::default();
        let node = WorkflowNode::new(
            "context",
            NodeType::ContextProvider {
                provider: crate::workflow::ContextProviderProfile {
                    provider_id: "workspace.knowledge".into(),
                    query: "тест".into(),
                    max_items: 4,
                    max_age_ms: 60_000,
                    evidence_schema: None,
                },
            },
            policy(),
        );
        let error = adapter
            .execute(invocation(&node, &registry, &parent, BTreeMap::new()))
            .await
            .expect_err("unknown provider");
        assert_eq!(error.code, "unknown_context_provider");
    }

    #[tokio::test]
    async fn an_unavailable_source_degrades_instead_of_answering_confidently() {
        let (adapter, _dir) = adapter();
        let registry = WorkflowRegistry::bootstrap();
        let parent = ParentCapabilities::default();
        let node = WorkflowNode::new(
            "context",
            NodeType::ContextProvider {
                provider: crate::workflow::ContextProviderProfile {
                    provider_id: "workspace.knowledge".into(),
                    query: "тест".into(),
                    max_items: 4,
                    max_age_ms: 60_000,
                    evidence_schema: None,
                },
            },
            policy(),
        );
        // Индекса рабочего каталога нет, поэтому источник недоступен.
        let result = adapter
            .execute(invocation(&node, &registry, &parent, BTreeMap::new()))
            .await
            .expect("degraded");
        assert!(result.degraded);
        assert_eq!(result.evidence, 0);
    }

    #[test]
    fn arguments_resolve_only_declared_input_ports() {
        let inputs = BTreeMap::from([("path".to_string(), json!("docs/readme.md"))]);
        let arguments = BTreeMap::from([
            ("path".to_string(), "$path".to_string()),
            ("mode".to_string(), "read".to_string()),
            ("missing".to_string(), "$nope".to_string()),
        ]);
        let resolved = resolve_arguments(&arguments, &inputs);
        assert_eq!(resolved["path"], json!("docs/readme.md"));
        assert_eq!(resolved["mode"], json!("read"));
        assert_eq!(resolved["missing"], Value::Null);
    }
}
