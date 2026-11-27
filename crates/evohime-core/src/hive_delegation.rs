use super::*;
use crate::persistent_agent_registry::{
    self, LifecycleStatus, OrganizationScope, OrganizationScopeKind, PersistentAgent,
    ResponsibilityKind, ResponsibilityScope, RoleProfileRef,
};
use evohime_tool_runtime::{ToolContext, ToolError, ToolResult};

const HIVE_CHILD_OUTPUT_BYTES: usize = 24 * 1024;
const HIVE_CHILD_DEFAULT_STEPS: usize = 16;

/// Returns `true` when `tool_name` is a direct tool that the Hive Queen must
/// not call herself; the Queen only orchestrates and delegates to child agents.
pub(crate) fn queen_excluded_direct_tool(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        evohime_tool_runtime::agent::HIVE_DELEGATE_NAME
            | evohime_tool_runtime::agent::HIVE_REGISTER_ROLE_NAME
            | "goal_create"
            | "goal_verify"
    )
}

/// Returns the default tool allowlist for a built-in Hive role, if recognized.
pub(crate) fn builtin_role_allowed_tools(role: &str) -> Option<Vec<String>> {
    let tools: &[&str] = match role {
        "researcher" => &[
            "filesystem_list",
            "filesystem_read",
            "filesystem_search",
            "git_status",
            "git_diff",
            "git_log",
        ],
        "planner" | "architect" => &[
            "filesystem_list",
            "filesystem_read",
            "filesystem_search",
            "git_status",
            "git_log",
        ],
        "reviewer" => &[
            "filesystem_list",
            "filesystem_read",
            "filesystem_search",
            "git_status",
            "git_diff",
            "git_log",
        ],
        "implementer" => &[
            "filesystem_list",
            "filesystem_read",
            "filesystem_search",
            "filesystem_write",
            "filesystem_patch",
            "filesystem_edit",
            "filesystem_delete",
            "git_status",
            "git_diff",
            "shell_execute",
            "script_execute",
        ],
        "tester" => &[
            "filesystem_list",
            "filesystem_read",
            "filesystem_search",
            "git_status",
            "git_diff",
            "shell_execute",
            "script_execute",
        ],
        "releaser" => &[
            "filesystem_list",
            "filesystem_read",
            "git_status",
            "git_diff",
            "git_log",
            "git_commit",
            "shell_execute",
            "script_execute",
        ],
        _ => return None,
    };
    Some(tools.iter().map(|name| (*name).to_owned()).collect())
}

fn push_unique_tool(sanitized: &mut Vec<String>, tool: &str) {
    if !sanitized.iter().any(|existing| existing == tool) {
        sanitized.push(tool.to_owned());
    }
}

fn sanitize_custom_role_tools(tools: &[String]) -> Vec<String> {
    let mut sanitized = Vec::new();
    for tool in tools {
        let trimmed = tool.trim();
        if trimmed.is_empty()
            || trimmed == evohime_tool_runtime::agent::NAME
            || trimmed == evohime_tool_runtime::agent::HIVE_DELEGATE_NAME
            || trimmed == evohime_tool_runtime::agent::HIVE_REGISTER_ROLE_NAME
            || trimmed == "git_push"
            || trimmed.starts_with(evohime_tool_runtime::self_development::TOOL_PREFIX)
        {
            continue;
        }
        match trimmed {
            "workspace.read" => {
                push_unique_tool(&mut sanitized, "filesystem_list");
                push_unique_tool(&mut sanitized, "filesystem_read");
            }
            "workspace.search" => {
                push_unique_tool(&mut sanitized, "filesystem_search");
            }
            "workspace.write" => {
                push_unique_tool(&mut sanitized, "filesystem_write");
                push_unique_tool(&mut sanitized, "filesystem_patch");
                push_unique_tool(&mut sanitized, "filesystem_edit");
                push_unique_tool(&mut sanitized, "filesystem_delete");
            }
            "test.execute" => {
                push_unique_tool(&mut sanitized, "shell_execute");
            }
            other => {
                push_unique_tool(&mut sanitized, other);
            }
        }
    }
    if sanitized.is_empty() {
        vec![
            "filesystem_list".to_owned(),
            "filesystem_read".to_owned(),
            "filesystem_search".to_owned(),
        ]
    } else {
        sanitized
    }
}

fn role_requires_mutation_session(role: &str, allowed_tools: &[String]) -> bool {
    if matches!(role, "implementer" | "tester" | "releaser") {
        return true;
    }
    allowed_tools.iter().any(|tool| {
        matches!(
            tool.as_str(),
            "filesystem_write"
                | "filesystem_patch"
                | "filesystem_edit"
                | "filesystem_delete"
                | "shell_execute"
                | "git_commit"
        )
    })
}

async fn wait_for_trip_arrival(
    proxy: &crate::external_pulse::ExternalHiveObservationProxy,
    initial: evohime_hive_contract::honeycomb::HoneycombTripStatus,
    cancellation: &tokio_util::sync::CancellationToken,
) -> bool {
    const DELIVERY_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);
    const MAX_DELIVERY_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

    if initial.arrived || initial.trip_id.is_empty() {
        return true;
    }

    let deadline = tokio::time::Instant::now() + MAX_DELIVERY_WAIT;
    loop {
        if cancellation.is_cancelled() {
            return false;
        }
        if tokio::time::Instant::now() >= deadline {
            return true;
        }
        tokio::select! {
            () = cancellation.cancelled() => return false,
            () = tokio::time::sleep(DELIVERY_POLL_INTERVAL) => {}
        }
        match proxy.poll_trip_arrival(&initial.trip_id).await {
            Ok(status) if status.arrived => return true,
            Ok(_) => {}
            Err(_) => return true,
        }
    }
}

async fn wait_for_hive_vehicle_delivery(
    role: &str,
    topic: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) -> bool {
    let proxy = crate::external_pulse::ExternalHiveObservationProxy::new();
    let initial = match proxy.dispatch_agent_task(role, topic).await {
        Ok(status) => status,
        Err(_) => {
            // When evohime-hive-daemon is not running (e.g. unit tests or CLI), do not block delegation.
            return true;
        }
    };
    wait_for_trip_arrival(&proxy, initial, cancellation).await
}

async fn wait_for_hive_vehicle_return(
    role: &str,
    topic: &str,
    cancellation: &tokio_util::sync::CancellationToken,
) {
    let proxy = crate::external_pulse::ExternalHiveObservationProxy::new();
    let Ok(initial) = proxy.complete_agent_task(role, topic).await else {
        return;
    };
    let _ = wait_for_trip_arrival(&proxy, initial, cancellation).await;
}

fn validate_role_slug(raw: &str) -> Result<String, ToolError> {
    let normalized = raw.trim().to_ascii_lowercase();
    if normalized.len() < 2
        || normalized.len() > 48
        || !normalized.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        })
    {
        return Err(ToolError::Execution(
            "role must be 2..48 ASCII characters matching [a-z0-9_-]".into(),
        ));
    }
    Ok(normalized)
}

fn resolve_agent_display_name(requested: Option<&str>, role: &str, child_task_id: &str) -> String {
    if let Some(custom) = requested.map(str::trim).filter(|name| !name.is_empty()) {
        return custom.chars().take(64).collect();
    }
    const NAMES: [&str; 12] = [
        "Артём",
        "Алиса",
        "Марк",
        "Вера",
        "Денис",
        "Кира",
        "Олег",
        "Ника",
        "Иван",
        "Лиза",
        "Глеб",
        "Соня",
    ];
    let role_ru = match role {
        "researcher" => "Исследователь",
        "planner" => "Планировщик",
        "architect" => "Архитектор",
        "implementer" => "Разработчик",
        "tester" => "Тестировщик",
        "reviewer" => "Ревьюер",
        "releaser" => "Релиз-инженер",
        _ => role,
    };
    let hash = child_task_id.bytes().fold(0usize, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(usize::from(byte))
    });
    let person = NAMES.get(hash % NAMES.len()).copied().unwrap_or("Агент");
    format!("{role_ru} {person}")
}

/// Parameters for [`ToolAgent::execute_hive_delegate_agent`].
pub(super) struct HiveDelegateRequest<'a> {
    pub context: &'a ToolContext,
    pub parent_task_id: &'a str,
    pub parent_binding: Option<&'a crate::SessionBinding>,
    pub hive_world: bool,
    pub events: &'a EventSink,
    pub input: serde_json::Value,
    pub cancellation: &'a CancellationToken,
}

impl ToolAgent {
    async fn resolve_custom_role_tools(&self, role: &str) -> Option<Vec<String>> {
        let journal = self.journal.as_ref()?;
        let database = journal.database().lock().await;
        let agents =
            evohime_local_storage::domains::agents::load_agents(database.connection(), 256).ok()?;
        for bytes in agents {
            let Ok(agent) = serde_json::from_slice::<PersistentAgent>(&bytes) else {
                continue;
            };
            if agent.status != LifecycleStatus::Active {
                continue;
            }
            if agent.role_profile_ref.id.eq_ignore_ascii_case(role) {
                return Some(sanitize_custom_role_tools(&agent.execution_policy_refs));
            }
        }
        None
    }

    async fn record_child_workflow_projection(
        &self,
        parent_task_id: &str,
        child_task_id: &str,
        role: &str,
        display_name: &str,
        state: &str,
    ) {
        let Some(journal) = self.journal.as_ref() else {
            return;
        };
        let payload = serde_json::json!({
            "child_task_id": child_task_id,
            "parent_task_id": parent_task_id,
            "role": role,
            "display_name": display_name,
            "state": state,
            "dead_letter": false,
            "depth": 1,
            "spawned_subagents": 0,
            "planned_ceiling": 1,
        });
        if let Ok(bytes) = serde_json::to_vec(&payload) {
            let _ = journal
                .record_audit(parent_task_id, "child.workflow", bytes.as_slice())
                .await;
        }
    }

    /// Executes a specialized Hive child agent delegated by the Queen (`agent_depth == 0`).
    pub(super) async fn execute_hive_delegate_agent(
        &self,
        request: HiveDelegateRequest<'_>,
    ) -> Result<ToolResult, ToolError> {
        let HiveDelegateRequest {
            context,
            parent_task_id,
            parent_binding,
            hive_world,
            events,
            input,
            cancellation,
        } = request;
        if !hive_world || context.agent_depth != 0 {
            return Err(ToolError::Execution(
                "hive_delegate_agent is only available to the Hive Queen at depth 0".into(),
            ));
        }
        let parsed = evohime_tool_runtime::agent::parse_hive_delegate_input(&input)?;
        let role = validate_role_slug(&parsed.role)?;
        let allowed_tools = if let Some(builtin) = builtin_role_allowed_tools(&role) {
            builtin
        } else if let Some(custom) = self.resolve_custom_role_tools(&role).await {
            custom
        } else {
            return Err(ToolError::Execution(format!(
                "unknown Hive role '{role}'; use a built-in role or register it first via hive_register_role"
            )));
        };
        let read_only = !role_requires_mutation_session(&role, &allowed_tools);
        if !read_only
            && parsed
                .plan_summary
                .as_deref()
                .map(str::trim)
                .filter(|summary| !summary.is_empty())
                .is_none()
        {
            return Err(ToolError::Execution(format!(
                "role '{role}' requires a non-empty plan_summary for user approval before execution"
            )));
        }

        let max_steps = parsed
            .max_steps
            .unwrap_or(HIVE_CHILD_DEFAULT_STEPS)
            .clamp(1, DEFAULT_TOOL_ITERATIONS);
        let timeout_cap = evohime_tool_runtime::agent::TIMEOUT.as_millis() as u64;
        let timeout = std::time::Duration::from_millis(
            parsed
                .timeout_ms
                .unwrap_or(timeout_cap)
                .clamp(1_000, timeout_cap),
        );
        let child_task_id = format!("{parent_task_id}:role-{role}-{}", uuid::Uuid::new_v4());
        let display_name =
            resolve_agent_display_name(parsed.agent_name.as_deref(), &role, &child_task_id);
        self.record_child_workflow_projection(
            parent_task_id,
            &child_task_id,
            &role,
            &display_name,
            "running",
        )
        .await;

        let agent = Self {
            gateway: Arc::clone(&self.gateway),
            gateway_slot: None,
            tools: Arc::clone(&self.tools),
            max_iterations: max_steps,
            approvals: self.approvals.clone(),
            routing_approvals: self.routing_approvals.clone(),
            journal: self.journal.clone(),
            selected_model: self.selected_model.clone(),
            receipt_keys: self.receipt_keys.clone(),
            extraction_guard: Arc::clone(&self.extraction_guard),
            extraction_lease: Arc::clone(&self.extraction_lease),
            proactivity: self.proactivity.clone(),
            workflow_registry: Arc::clone(&self.workflow_registry),
            workflow_state: Arc::clone(&self.workflow_state),
        };
        let execution_context = ConversationExecutionContext {
            preferred_route_hint: Some("hive".to_owned()),
            history: Vec::new(),
            continuation_attempt: None,
            defer_completion_event: true,
            conversation_id: None,
            session_binding: parent_binding.cloned(),
            read_only_subagent: read_only,
            agent_depth: 1,
            child_role: Some(role.clone()),
            child_allowed_tools: Some(allowed_tools.clone()),
            inherited_permission_session_id: if read_only { None } else { context.session_id },
            child_plan_summary: parsed.plan_summary.clone(),
        };
        let tool_guidance = if read_only {
            ""
        } else {
            " Для чтения файлов, поиска и просмотра git-состояния всегда используй встроенные инструменты \
             (filesystem_read, filesystem_list, filesystem_search, git_status, git_diff, git_log) — не вызывай \
             shell_execute для чтения или осмотра файлов."
        };
        let child_prompt = format!(
            "[Hive Role: {role}]\nВыполни строго свою часть задачи в рамках назначенной роли ({role}). \
             Не выходи за пределы доступных инструментов и верни точный фактический отчёт.{tool_guidance}\n\n{}",
            parsed.prompt
        );

        let run_cancel = cancellation.child_token();
        if !wait_for_hive_vehicle_delivery(&role, &display_name, &run_cancel).await {
            self.record_child_workflow_projection(
                parent_task_id,
                &child_task_id,
                &role,
                &display_name,
                "cancelled",
            )
            .await;
            return Err(ToolError::Execution(
                "hive_delegate_agent was cancelled while waiting for vehicle delivery".into(),
            ));
        }

        let timed_out = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let timer = {
            let timed_out = Arc::clone(&timed_out);
            let run_cancel = run_cancel.clone();
            tokio::spawn(async move {
                tokio::time::sleep(timeout).await;
                timed_out.store(true, std::sync::atomic::Ordering::SeqCst);
                run_cancel.cancel();
            })
        };
        let result = Box::pin(agent.run_once_with_cancellation(
            child_task_id.clone(),
            child_prompt,
            context.workspace_root.clone(),
            events,
            run_cancel,
            execution_context,
        ))
        .await;
        timer.abort();
        let timed_out = timed_out.load(std::sync::atomic::Ordering::SeqCst);

        match result {
            Ok(summary) => {
                wait_for_hive_vehicle_return(&role, &display_name, cancellation).await;
                self.record_child_workflow_projection(
                    parent_task_id,
                    &child_task_id,
                    &role,
                    &display_name,
                    "accepted",
                )
                .await;
                let had_answer = !summary.trim().is_empty();
                let bounded = super::execution::bounded_tool_output(
                    if had_answer {
                        &summary
                    } else {
                        "delegated role finished without a final answer"
                    },
                    HIVE_CHILD_OUTPUT_BYTES,
                );
                let researched = matches!(role.as_str(), "researcher" | "planner" | "architect");
                let mutated = role == "implementer";
                let verified = role == "tester";
                let diff_checked = role == "tester";
                let committed = role == "releaser";
                Ok(ToolResult {
                    output: format!(
                        "hive_delegate_agent ({display_name}, role={role}, status=accepted):\n{bounded}"
                    ),
                    structured: serde_json::json!({
                        "role": role,
                        "display_name": display_name,
                        "child_task_id": child_task_id,
                        "status": "accepted",
                        "summary": bounded,
                        "read_only": read_only,
                        "researched": researched,
                        "mutated": mutated,
                        "verified": verified,
                        "diff_checked": diff_checked,
                        "committed": committed,
                        "step_budget": max_steps,
                    }),
                })
            }
            Err(error) => {
                let child_state = match &error {
                    AgentRunError::Timeout(_) => "timed_out",
                    AgentRunError::Cancelled if timed_out => "timed_out",
                    AgentRunError::Cancelled => "cancelled",
                    _ => "failed",
                };
                self.record_child_workflow_projection(
                    parent_task_id,
                    &child_task_id,
                    &role,
                    &display_name,
                    child_state,
                )
                .await;
                Err(match error {
                    AgentRunError::Timeout(seconds) => {
                        ToolError::TimedOut(std::time::Duration::from_secs(seconds))
                    }
                    AgentRunError::Cancelled if timed_out => ToolError::TimedOut(timeout),
                    AgentRunError::Cancelled => {
                        ToolError::Execution("hive_delegate_agent was cancelled".into())
                    }
                    other => ToolError::Execution(super::execution::bounded_tool_output(
                        &format!("hive_delegate_agent ({role}) failed: {other}"),
                        2_048,
                    )),
                })
            }
        }
    }

    /// Registers or updates a persistent dynamic Hive role and its sector metadata in SQLite.
    pub(super) async fn execute_hive_register_role(
        &self,
        context: &ToolContext,
        hive_world: bool,
        input: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        if !hive_world || context.agent_depth != 0 {
            return Err(ToolError::Execution(
                "hive_register_role is only available to the Hive Queen at depth 0".into(),
            ));
        }
        let parsed = evohime_tool_runtime::agent::parse_hive_register_role_input(&input)?;
        let role_id = validate_role_slug(&parsed.role)?;
        if builtin_role_allowed_tools(&role_id).is_some() {
            return Err(ToolError::Execution(format!(
                "role '{role_id}' is a built-in Hive role and does not need registration"
            )));
        }
        let allowed_tools = sanitize_custom_role_tools(&parsed.capabilities);
        let journal = self.journal.as_ref().ok_or_else(|| {
            ToolError::Execution("hive_register_role requires the Core journal".into())
        })?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let agent_id = format!("hive-role-{role_id}");
        let database = journal.database().lock().await;
        let existing =
            evohime_local_storage::domains::agents::load_agent(database.connection(), &agent_id)
                .map_err(|error| ToolError::Execution(error.to_string()))?
                .and_then(|bytes| serde_json::from_slice::<PersistentAgent>(&bytes).ok());

        let revision = existing
            .as_ref()
            .map(|agent| agent.revision.saturating_add(1))
            .unwrap_or(1);
        let created_at_ms = existing
            .as_ref()
            .map(|agent| agent.created_at_ms)
            .unwrap_or(now_ms);

        let mut agent = PersistentAgent {
            schema_version: persistent_agent_registry::CONTRACT_VERSION,
            id: agent_id.clone(),
            revision,
            display_name: parsed.display_name.clone(),
            role_profile_ref: RoleProfileRef {
                id: role_id.clone(),
                revision,
                content_hash: format!("role-{role_id}-v{revision}"),
            },
            organization_scope: OrganizationScope {
                kind: OrganizationScopeKind::Workspace,
                id: context.workspace_root.to_string_lossy().into_owned(),
            },
            reporting_to_agent_id: Some("queen".to_owned()),
            responsibility_scope: vec![ResponsibilityScope {
                kind: ResponsibilityKind::Subsystem,
                selector: parsed.department.clone(),
            }],
            default_goal_bindings: Vec::new(),
            execution_policy_refs: allowed_tools.clone(),
            status: LifecycleStatus::Active,
            created_at_ms,
            updated_at_ms: now_ms,
            content_hash: String::new(),
        };
        let hash = persistent_agent_registry::canonical_hash(&agent)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        agent.content_hash = hash.clone();
        persistent_agent_registry::validate_agent(&agent)
            .map_err(|error| ToolError::Execution(error.to_string()))?;
        let agent_json =
            serde_json::to_vec(&agent).map_err(|error| ToolError::Execution(error.to_string()))?;

        let saved = evohime_local_storage::domains::agents::save_agent_revision(
            database.connection(),
            evohime_local_storage::domains::agents::SaveAgentRevisionInput {
                id: &agent_id,
                revision,
                status: "active",
                content_hash: &hash,
                agent_json: &agent_json,
                actor: "queen",
                now_ms,
            },
        )
        .map_err(|error| ToolError::Execution(error.to_string()))?;

        if !saved {
            return Err(ToolError::Execution(format!(
                "failed to persist Hive role '{role_id}'"
            )));
        }

        Ok(ToolResult {
            output: format!(
                "Registered dynamic Hive role '{role_id}' ({}) with tools: {}",
                parsed.display_name,
                allowed_tools.join(", ")
            ),
            structured: serde_json::json!({
                "role": role_id,
                "display_name": parsed.display_name,
                "department": parsed.department,
                "objective": parsed.objective,
                "strategy": parsed.strategy,
                "allowed_tools": allowed_tools,
                "revision": revision,
                "status": "active",
            }),
        })
    }
}
