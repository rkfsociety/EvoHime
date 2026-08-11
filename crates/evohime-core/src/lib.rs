pub struct CoreVersion;

pub const AGENT_IDENTITY_PROMPT: &str =
    "Ты — Ева, AI-агент приложения EvoHime. Ева — короткое имя EvoHime; понимай обращения к тебе «Ева» и «EvoHime» как к одному агенту.";

fn build_agent_system_prompt(workspace: &std::path::Path, tool_names: &[String]) -> String {
    format!(
        "{AGENT_IDENTITY_PROMPT}\n\n\
Ты работаешь автономно внутри уже выбранного рабочего пространства.\n\
Текущий workspace: {}\n\
Этот путь доступен инструментам как их корневая папка; не проси пользователя сообщать его повторно.\n\n\
Правила выполнения:\n\
- Выполняй задачу самостоятельно и используй инструменты, когда они нужны для фактической проверки.\n\
- За один ответ вызывай только один инструмент и жди его результата перед следующим вызовом.\n\
- Если пользователь просит изучить, проверить, найти или объяснить проект, сначала вызови filesystem.list с path точкой (.).\n\
- Затем прочитай подходящие manifest-файлы и документацию (например Cargo.toml, *.csproj, package.json, README и архитектурные документы), а для поиска по коду используй filesystem.search.\n\
- Для изучения проекта не используй shell.execute: filesystem.list, filesystem.read и filesystem.search безопаснее и достаточно информативны.\n\
- Не проси пользователя прислать структуру проекта, путь или команды, если workspace уже указан.\n\
- Не утверждай, что изучила файл или выполнила действие, пока соответствующий инструмент не вернул результат.\n\
- Для чтения используй безопасные read-only инструменты. Перед изменениями и опасными действиями учитывай approval.\n\
- Если пользователь просит исправить, изменить или реализовать код, обязательно внеси изменение через filesystem.patch или filesystem.write; для просьбы о проверке запусти соответствующий тест/сборку, а для просьбы о commit вызови git.commit. Не выдавай план за результат.\n\
- Все улучшения EvoHime выполняй самостоятельно через доступные инструменты Евы. Если попытка не удалась, не завершай задачу: зафиксируй причину в trace, дождись следующего шага настройки и повтори работу через инструменты до успешного результата.\n\
- После исследования дай отчёт: что обнаружено, какие файлы проверены, какие проблемы найдены и что предлагается сделать дальше.\n\n\
Доступные инструменты в этой сессии:\n{}",
        workspace.display(),
        tool_names
            .iter()
            .map(|name| format!("- {name}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn tool_parameters(name: &str) -> serde_json::Value {
    match name {
        "filesystem.list" => serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Workspace-relative directory, usually ." } }
        }),
        "filesystem.read" => serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Workspace-relative UTF-8 file path" } },
            "required": ["path"]
        }),
        "filesystem.write" => serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative UTF-8 file path" },
                "content": { "type": "string", "description": "Complete UTF-8 file content" }
            },
            "required": ["path", "content"]
        }),
        "filesystem.patch" => serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Workspace-relative file path" },
                "patch": { "type": "string", "description": "Unified diff to apply to the file" }
            },
            "required": ["path", "patch"]
        }),
        "filesystem.search" => serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Text or pattern to search for" },
                "path": { "type": "string", "description": "Optional workspace-relative directory" },
                "glob": { "type": "string", "description": "Optional file glob" },
                "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
            },
            "required": ["query"]
        }),
        "git.diff" => serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Optional workspace-relative path" } }
        }),
        "git.commit" => serde_json::json!({
            "type": "object",
            "properties": { "message": { "type": "string", "description": "Commit message" } },
            "required": ["message"]
        }),
        "shell.execute" => serde_json::json!({
            "type": "object",
            "properties": {
                "program": { "type": "string", "description": "Executable name, for example cargo or dotnet" },
                "args": { "type": "array", "items": { "type": "string" } },
                "cwd": { "type": "string" },
                "timeout_ms": { "type": "integer" }
            },
            "required": ["program"]
        }),
        _ => serde_json::json!({ "type": "object", "additionalProperties": true }),
    }
}

fn write_model_trace(event: &str, fields: serde_json::Value) {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"));
    let logs_dir = data_dir.join("logs");
    if fs::create_dir_all(&logs_dir).is_err() {
        return;
    }
    let timestamp_ms = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let record = serde_json::json!({
        "timestamp_ms": timestamp_ms,
        "event": event,
        "fields": fields,
    });
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs_dir.join("model-trace.jsonl"))
    {
        if serde_json::to_writer(&mut file, &record).is_ok() {
            let _ = file.write_all(b"\n");
        }
    }
}

fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn attribute_value(tag: &str, attribute: &str) -> Option<String> {
    let marker = format!("{attribute}=\"");
    let start = tag.find(&marker)? + marker.len();
    let end = tag[start..].find('\"')? + start;
    Some(xml_unescape(&tag[start..end]))
}

fn parse_legacy_function_calls(content: &str, iteration: usize) -> Vec<NativeToolCall> {
    let mut calls = Vec::new();
    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find("<invoke") {
        let start = cursor + relative_start;
        let Some(tag_end_relative) = content[start..].find('>') else {
            break;
        };
        let tag_end = start + tag_end_relative;
        let Some(name) = attribute_value(&content[start..=tag_end], "name") else {
            cursor = tag_end + 1;
            continue;
        };
        let legacy_read_only = matches!(
            name.as_str(),
            "filesystem.list"
                | "filesystem.read"
                | "filesystem.search"
                | "git.status"
                | "git.diff"
                | "memory.search"
                | "browser.open"
                | "browser.extract"
                | "browser.session.read"
                | "browser.session.screenshot"
        );
        let body_start = tag_end + 1;
        let Some(body_end_relative) = content[body_start..].find("</invoke>") else {
            break;
        };
        let body_end = body_start + body_end_relative;
        let body = &content[body_start..body_end];
        let mut arguments = serde_json::Map::new();
        let mut parameter_cursor = 0;
        while let Some(relative_parameter) = body[parameter_cursor..].find("<parameter") {
            let parameter_start = parameter_cursor + relative_parameter;
            let Some(parameter_tag_end_relative) = body[parameter_start..].find('>') else {
                break;
            };
            let parameter_tag_end = parameter_start + parameter_tag_end_relative;
            let tag = &body[parameter_start..=parameter_tag_end];
            let Some(parameter_name) = attribute_value(tag, "name") else {
                parameter_cursor = parameter_tag_end + 1;
                continue;
            };
            let value_start = parameter_tag_end + 1;
            let Some(value_end_relative) = body[value_start..].find("</parameter>") else {
                break;
            };
            let value_end = value_start + value_end_relative;
            arguments.insert(
                parameter_name,
                serde_json::Value::String(xml_unescape(body[value_start..value_end].trim())),
            );
            parameter_cursor = value_end + "</parameter>".len();
        }
        if legacy_read_only {
            calls.push(NativeToolCall {
                id: format!("legacy-{iteration}-{}", calls.len()),
                name,
                arguments: serde_json::Value::Object(arguments).to_string(),
            });
        }
        cursor = body_end + "</invoke>".len();
    }
    calls
}

fn parse_natural_tool_intent(content: &str, iteration: usize) -> Option<NativeToolCall> {
    let lower = content.to_lowercase();
    let explicit_action = ["вызываю", "вызову", "вызвать", "запрашиваю"]
        .iter()
        .any(|marker| lower.contains(marker));
    let name = ["filesystem.list", "filesystem.read", "filesystem.search"]
        .iter()
        .find(|candidate| content.contains(**candidate))
        .copied()?;
    if !explicit_action {
        return None;
    }

    let path = content
        .split('`')
        .nth(1)
        .filter(|value| !value.contains('.'))
        .unwrap_or(".");
    let arguments = match name {
        "filesystem.list" | "filesystem.read" => serde_json::json!({ "path": path }),
        "filesystem.search" => serde_json::json!({ "query": path }),
        _ => return None,
    };
    Some(NativeToolCall {
        id: format!("natural-{iteration}"),
        name: name.to_string(),
        arguments: arguments.to_string(),
    })
}

fn parse_tagged_tool_call(content: &str, iteration: usize) -> Option<NativeToolCall> {
    if let Some(start) = content.find("<tool_code>") {
        let body_start = start + "<tool_code>".len();
        let body_end = content[body_start..].find("</tool_code>")? + body_start;
        let wrapped = format!("<tool_call>{}</tool_call>", content[body_start..body_end].trim());
        return parse_tagged_tool_call(&wrapped, iteration);
    }
    if let (Some(name_start), Some(input_start)) = (
        content.find("<tool_name>"),
        content.find("<tool_input>"),
    ) {
        let name_start = name_start + "<tool_name>".len();
        let name_end = content[name_start..].find("</tool_name>")? + name_start;
        let input_start = input_start + "<tool_input>".len();
        let input_end = content[input_start..].find("</tool_input>")? + input_start;
        let name = content[name_start..name_end].trim();
        if !matches!(
            name,
            "filesystem.list"
                | "filesystem.read"
                | "filesystem.search"
                | "git.status"
                | "git.diff"
        ) {
            return None;
        }
        let arguments = serde_json::from_str::<serde_json::Value>(content[input_start..input_end].trim()).ok()?;
        return Some(NativeToolCall {
            id: format!("tagged-{iteration}"),
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }
    let start_marker = "<tool_call>";
    let end_marker = "</tool_call>";
    let start = content.find(start_marker)? + start_marker.len();
    let end = content[start..].find(end_marker)? + start;
    let body = content[start..end].trim();
    let (name, arguments) = if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        let name = value.get("name")?.as_str()?.to_string();
        let arguments = value
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        (name, arguments)
    } else {
        let open = body.find('(')?;
        let name = body[..open].trim().to_string();
        let params = body[open + 1..].strip_suffix(')')?.trim();
        let mut arguments = serde_json::Map::new();
        for pair in params.split(',').filter(|pair| !pair.trim().is_empty()) {
            let (key, value) = pair.split_once('=')?;
            let value = value.trim().trim_matches('"').trim_matches('\'');
            arguments.insert(key.trim().to_string(), serde_json::Value::String(value.to_string()));
        }
        (name, serde_json::Value::Object(arguments))
    };
    if !matches!(
        name.as_str(),
        "filesystem.list" | "filesystem.read" | "filesystem.search" | "git.status" | "git.diff"
    ) {
        return None;
    }
    Some(NativeToolCall {
        id: format!("tagged-{iteration}"),
        name,
        arguments: arguments.to_string(),
    })
}

fn parse_plain_tool_call(content: &str, iteration: usize) -> Option<NativeToolCall> {
    let name = ["filesystem.list", "filesystem.read", "filesystem.search", "git.status", "git.diff"]
        .iter()
        .find(|candidate| content.lines().any(|line| line.trim() == **candidate))
        .copied()?;
    let (key, value) = content
        .lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim(), value.trim()))
        .find(|(key, _)| matches!(*key, "path" | "query"))?;
    Some(NativeToolCall {
        id: format!("plain-{iteration}"),
        name: name.to_string(),
        arguments: serde_json::json!({ key: value }).to_string(),
    })
}

fn parse_xml_named_tool_call(content: &str, iteration: usize) -> Option<NativeToolCall> {
    let name = ["filesystem.list", "filesystem.read", "filesystem.search", "git.status", "git.diff"]
        .iter()
        .find(|candidate| content.contains(&format!("<{}>", candidate)))
        .copied()?;
    let start_marker = format!("<{}>", name);
    let end_marker = format!("</{}>", name);
    let start = content.find(&start_marker)? + start_marker.len();
    let end = content[start..].find(&end_marker)? + start;
    let body = &content[start..end];
    let parameter_start = body.find("<parameter")?;
    let tag_end = body[parameter_start..].find('>')? + parameter_start;
    let value_end = body[tag_end + 1..].find("</parameter>")? + tag_end + 1;
    let tag = &body[parameter_start..=tag_end];
    let parameter_name = attribute_value(tag, "name").or_else(|| {
        body[tag_end + 1..value_end]
            .split_once('>')
            .map(|(key, _)| key.trim().to_string())
    })?;
    let value = if tag.contains("name=") {
        body[tag_end + 1..value_end].trim().to_string()
    } else {
        body[tag_end + 1..value_end]
            .split_once('>')
            .map(|(_, value)| value.trim().to_string())?
    };
    Some(NativeToolCall {
        id: format!("xml-{iteration}"),
        name: name.to_string(),
        arguments: serde_json::json!({ parameter_name: value }).to_string(),
    })
}

fn strip_legacy_function_blocks(content: &str) -> String {
    let mut cleaned = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find("<function_calls>") {
        let start = cursor + relative_start;
        cleaned.push_str(&content[cursor..start]);
        let block_start = start + "<function_calls>".len();
        let Some(relative_end) = content[block_start..].find("</function_calls>") else {
            break;
        };
        cursor = block_start + relative_end + "</function_calls>".len();
    }
    cleaned.push_str(&content[cursor..]);
    cleaned.trim().to_string()
}

#[derive(Debug, Default, Clone, Copy)]
struct DeliveryRequirements {
    mutation: bool,
    verification: bool,
    commit: bool,
}

impl DeliveryRequirements {
    fn from_prompt(prompt: &str) -> Self {
        let prompt = prompt.to_lowercase();
        Self {
            mutation: ["исправ", "измен", "добав", "реализ", "сделай", "улучш", "удал", "убер"]
                .iter()
                .any(|marker| prompt.contains(marker)),
            verification: ["проверь", "провер", "тест", "test", "собери", "запусти"]
                .iter()
                .any(|marker| prompt.contains(marker)),
            commit: prompt.contains("коммит") || prompt.contains("commit"),
        }
    }

    fn missing(
        self,
        mutation_done: bool,
        verification_done: bool,
        commit_done: bool,
    ) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.mutation && !mutation_done {
            missing.push("внести изменение");
        }
        if self.verification && !verification_done {
            missing.push("проверить результат");
        }
        if self.commit && !commit_done {
            missing.push("создать commit");
        }
        missing
    }
}

mod ipc_bridge;
pub use ipc_bridge::{IpcBridge, IpcBridgeError, ModelConfigSnapshot};
mod logging;
pub use logging::StructuredLogger;

#[cfg(windows)]
pub async fn run_windows_pipe(
    pipe_name: &str,
    bridge: IpcBridge,
    logger: std::sync::Arc<StructuredLogger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::split;
    use tokio::net::windows::named_pipe::ServerOptions;

    loop {
        let server = ServerOptions::new().create(pipe_name)?;
        server.connect().await?;
        let (mut reader, mut writer) = split(server);
        loop {
            if let Err(error) = bridge.process_once(&mut reader, &mut writer).await {
                let _ = logger.write(
                    "warn",
                    "ipc.connection_closed",
                    serde_json::json!({"error": error.to_string()}),
                );
                break;
            }
        }
    }
}

impl CoreVersion {
    pub const fn current() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use evohime_local_storage::{
    EventRecord, LocalDatabase, StorageError, WorkItemRecord,
};
use evohime_model_gateway::{
    providers::{ChatMessage, ChatRole, ProviderError},
    ModelGateway, NativeToolCall, ToolSpec,
};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

pub mod prd;

pub enum CoreCommand {
    StartTask {
        task_id: String,
        prompt: String,
        workspace_root: Option<PathBuf>,
    },
    StopTask {
        task_id: String,
    },
    CreateProject {
        client_id: String,
        request_id: String,
        command_hash: String,
        project_id: String,
        title: String,
        workspace_path: String,
        source_ref: Option<String>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    CreateTask {
        client_id: String,
        request_id: String,
        command_hash: String,
        item: WorkItemRecord,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    UpdateTaskStatus {
        client_id: String,
        request_id: String,
        command_hash: String,
        task_id: String,
        expected_version: i64,
        status: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    AddTaskEdge {
        client_id: String,
        request_id: String,
        command_hash: String,
        from_task_id: String,
        to_task_id: String,
        kind: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskGraph {
        project_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    NextReadyTask {
        project_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CoreEvent {
    ModelContext {
        task_id: String,
        workspace_path: String,
        model: String,
        system_prompt: String,
        user_prompt: String,
        tools: Vec<String>,
        estimated_tokens: usize,
        context_limit_tokens: usize,
    },
    TaskStarted {
        task_id: String,
        prompt: String,
    },
    AssistantDelta {
        task_id: String,
        content: String,
    },
    ToolStarted {
        task_id: String,
        tool_name: String,
    },
    ToolOutput {
        task_id: String,
        tool_name: String,
        output: String,
    },
    ApprovalRequired {
        task_id: String,
        approval_id: String,
        tool_name: String,
        permission: String,
        scope: String,
        input: serde_json::Value,
    },
    TaskCompleted {
        task_id: String,
        final_message: String,
    },
    TaskFailed {
        task_id: String,
        error: String,
    },
    TaskStopped {
        task_id: String,
    },
}

#[derive(Clone)]
pub struct EventJournal {
    database: Arc<Mutex<LocalDatabase>>,
}

impl EventJournal {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Self, StorageError> {
        Ok(Self {
            database: Arc::new(Mutex::new(LocalDatabase::open(path)?)),
        })
    }

    pub async fn record(&self, event: &CoreEvent) -> Result<i64, StorageError> {
        let task_id = match event {
            CoreEvent::ModelContext { task_id, .. }
            | CoreEvent::TaskStarted { task_id, .. }
            | CoreEvent::AssistantDelta { task_id, .. }
            | CoreEvent::ToolStarted { task_id, .. }
            | CoreEvent::ToolOutput { task_id, .. }
            | CoreEvent::ApprovalRequired { task_id, .. }
            | CoreEvent::TaskCompleted { task_id, .. }
            | CoreEvent::TaskFailed { task_id, .. }
            | CoreEvent::TaskStopped { task_id } => task_id,
        };
        let event_type = match event {
            CoreEvent::ModelContext { .. } => "model.context",
            CoreEvent::TaskStarted { .. } => "task.started",
            CoreEvent::AssistantDelta { .. } => "agent.message.delta",
            CoreEvent::ToolStarted { .. } => "tool.started",
            CoreEvent::ToolOutput { .. } => "tool.output",
            CoreEvent::ApprovalRequired { .. } => "approval.required",
            CoreEvent::TaskCompleted { .. } => "task.completed",
            CoreEvent::TaskFailed { .. } => "task.failed",
            CoreEvent::TaskStopped { .. } => "task.stopped",
        };
        let payload = serde_json::to_vec(event).expect("core events serialize");
        let database = self.database.lock().await;
        database.append_event(task_id, event_type, &payload)
    }

    pub async fn replay(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_events_after(after_sequence, limit)
    }

    pub async fn create_project(
        &self,
        id: &str,
        title: &str,
        workspace_path: &str,
        source_ref: Option<&str>,
    ) -> Result<evohime_local_storage::ProjectRecord, StorageError> {
        let database = self.database.lock().await;
        database.create_project(id, title, workspace_path, source_ref)
    }

    pub async fn create_work_item(
        &self,
        item: &WorkItemRecord,
    ) -> Result<WorkItemRecord, StorageError> {
        let database = self.database.lock().await;
        database.create_work_item(item)
    }

    pub async fn update_work_item_status(
        &self,
        id: &str,
        expected_version: i64,
        status: &str,
    ) -> Result<WorkItemRecord, StorageError> {
        let database = self.database.lock().await;
        database.update_work_item_status(id, expected_version, status)
    }

    pub async fn add_dependency(
        &self,
        from_id: &str,
        to_id: &str,
        kind: &str,
    ) -> Result<(), StorageError> {
        let database = self.database.lock().await;
        database.add_dependency(from_id, to_id, kind)
    }

    pub async fn list_task_graph(
        &self,
        project_id: &str,
    ) -> Result<(Vec<WorkItemRecord>, Vec<(String, String, String)>), StorageError> {
        let database = self.database.lock().await;
        Ok((
            database.list_work_items(project_id)?,
            database.list_dependencies(project_id)?,
        ))
    }

    pub async fn next_ready_task(
        &self,
        project_id: &str,
    ) -> Result<Option<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.next_ready(project_id)
    }

    pub async fn record_deduplicated(
        &self,
        client_id: &str,
        request_id: &str,
        command_hash: &str,
        result: &[u8],
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let database = self.database.lock().await;
        database.record_deduplicated(client_id, request_id, command_hash, result)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentRunError {
    #[error("model request failed: {0}")]
    Provider(#[from] ProviderError),
    #[error("agent execution was cancelled")]
    Cancelled,
    #[error("agent execution timed out after {0} seconds")]
    Timeout(u64),
}

#[derive(Clone, Default)]
pub struct ApprovalCoordinator {
    pending: Arc<Mutex<HashMap<uuid::Uuid, oneshot::Sender<bool>>>>,
}

impl ApprovalCoordinator {
    pub async fn register(&self, approval_id: uuid::Uuid) -> oneshot::Receiver<bool> {
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(approval_id, sender);
        receiver
    }

    pub async fn resolve(&self, approval_id: uuid::Uuid, granted: bool) -> bool {
        self.pending
            .lock()
            .await
            .remove(&approval_id)
            .map(|sender| sender.send(granted).is_ok())
            .unwrap_or(false)
    }
}

pub trait TaskExecutor: Send + Sync {
    fn execute(
        &self,
        task_id: String,
        prompt: String,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>>;

    fn execute_in_workspace(
        &self,
        task_id: String,
        prompt: String,
        workspace_root: PathBuf,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let _ = workspace_root;
        self.execute(task_id, prompt, cancellation, events)
    }
}

pub struct ModelAgent {
    gateway: Arc<ModelGateway>,
}

impl ModelAgent {
    pub fn new(gateway: Arc<ModelGateway>) -> Self {
        Self { gateway }
    }

    pub async fn run_once(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<String, AgentRunError> {
        self.run_once_with_cancellation(task_id, prompt, events, CancellationToken::new())
            .await
    }

    async fn run_once_with_cancellation(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        events: &broadcast::Sender<CoreEvent>,
        cancellation: CancellationToken,
    ) -> Result<String, AgentRunError> {
        let task_id = task_id.into();
        let messages = [
            ChatMessage::text(ChatRole::System, AGENT_IDENTITY_PROMPT),
            ChatMessage::text(ChatRole::User, prompt),
        ];
        let mut stream = self.gateway.stream_chat(&messages);
        let mut final_message = String::new();
        while let Some(item) = tokio::select! {
            _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
            item = stream.next() => item,
        } {
            match item? {
                evohime_model_gateway::ChatStreamItem::Delta(content) => {
                    final_message.push_str(&content);
                    let _ = events.send(CoreEvent::AssistantDelta {
                        task_id: task_id.clone(),
                        content,
                    });
                }
                evohime_model_gateway::ChatStreamItem::Thinking(_)
                | evohime_model_gateway::ChatStreamItem::Usage(_) => {}
            }
        }
        let _ = events.send(CoreEvent::TaskCompleted {
            task_id,
            final_message: final_message.clone(),
        });
        Ok(final_message)
    }
}

impl TaskExecutor for ModelAgent {
    fn execute(
        &self,
        task_id: String,
        prompt: String,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let agent = Self {
            gateway: Arc::clone(&self.gateway),
        };
        Box::pin(async move {
            agent
                .run_once_with_cancellation(task_id, prompt, &events, cancellation)
                .await
        })
    }
}

pub struct ToolAgent {
    gateway: Arc<ModelGateway>,
    tools: Arc<ToolRegistry>,
    max_iterations: usize,
    approvals: ApprovalCoordinator,
}

impl ToolAgent {
    pub fn new(gateway: Arc<ModelGateway>, tools: Arc<ToolRegistry>) -> Self {
        Self::new_with_approvals(gateway, tools, ApprovalCoordinator::default())
    }

    pub fn new_with_approvals(
        gateway: Arc<ModelGateway>,
        tools: Arc<ToolRegistry>,
        approvals: ApprovalCoordinator,
    ) -> Self {
        Self {
            gateway,
            tools,
            max_iterations: 8,
            approvals,
        }
    }

    pub async fn run_once(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        events: &broadcast::Sender<CoreEvent>,
    ) -> Result<String, AgentRunError> {
        self.run_once_with_cancellation(
            task_id,
            prompt,
            workspace_root,
            events,
            CancellationToken::new(),
        )
        .await
    }

    async fn run_once_with_cancellation(
        &self,
        task_id: impl Into<String>,
        prompt: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        events: &broadcast::Sender<CoreEvent>,
        cancellation: CancellationToken,
    ) -> Result<String, AgentRunError> {
        let task_id = task_id.into();
        let task_uuid = uuid::Uuid::parse_str(&task_id).unwrap_or_else(|_| uuid::Uuid::new_v4());
        let context = ToolContext {
            workspace_root: workspace_root.into(),
            task_id: task_uuid,
            session_id: None,
            progress_tx: None,
        };
        let specs = self
            .tools
            .list()
            .into_iter()
            .map(|tool| {
                let name = tool.name.to_string();
                ToolSpec::function(
                    name,
                    tool.description,
                    tool_parameters(tool.name),
                )
            })
            .collect::<Vec<_>>();
        let tool_names = specs
            .iter()
            .map(|spec| spec.function.name.clone())
            .collect::<Vec<_>>();
        let system_prompt = build_agent_system_prompt(&context.workspace_root, &tool_names);
        let mut messages = vec![
            ChatMessage::text(ChatRole::System, system_prompt.clone()),
            ChatMessage::text(ChatRole::User, prompt),
        ];

        let user_prompt = messages[1].content.clone();
        let context_text = format!("{system_prompt}\n{user_prompt}\n{}", tool_names.join("\n"));
        let delivery_requirements = DeliveryRequirements::from_prompt(&user_prompt);
        let _ = events.send(CoreEvent::ModelContext {
            task_id: task_id.clone(),
            workspace_path: context.workspace_root.display().to_string(),
            model: self.gateway.model_name().to_string(),
            system_prompt,
            user_prompt,
            tools: tool_names,
            estimated_tokens: context_text.chars().count().div_ceil(4),
            context_limit_tokens: 128_000,
        });

        let mut legacy_seen = HashSet::new();
        let mut seen_tool_calls = HashSet::new();
        let mut mutation_done = false;
        let mut verification_done = false;
        let mut commit_done = false;
        for iteration in 0..self.max_iterations {
            write_model_trace(
                "model.request",
                serde_json::json!({
                    "task_id": task_id,
                    "model": self.gateway.model_name(),
                    "workspace_path": context.workspace_root,
                    "messages": messages,
                    "tools": specs,
                    "tool_choice": "auto"
                }),
            );
            let result = tokio::select! {
                _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
                result = self.gateway.chat_with_tools_for_route("default", None, &messages, &specs) => result?,
            };
            write_model_trace(
                "model.response",
                serde_json::json!({
                    "task_id": task_id,
                    "content": result.content,
                    "thinking": result.thinking,
                    "tool_calls": result.tool_calls,
                    "usage": result.usage
                }),
            );
            let mut tool_calls = result.tool_calls.clone();
            if tool_calls.is_empty() {
                let parsed_legacy_calls = parse_legacy_function_calls(&result.content, iteration);
                if !parsed_legacy_calls.is_empty() {
                    write_model_trace(
                        "legacy.tool_calls.parsed",
                        serde_json::json!({
                            "task_id": task_id,
                            "tool_calls": parsed_legacy_calls
                        }),
                    );
                    // Legacy models often print an entire future plan in one
                    // response. Execute every new, valid safe call from that
                    // plan before asking the model for its next observation.
                    // Unsafe calls are excluded by the parser; the directory
                    // read below is also invalid for the filesystem tool.
                    for call in parsed_legacy_calls.into_iter().filter(|call| {
                        let invalid_directory_read = call.name == "filesystem.read"
                            && serde_json::from_str::<serde_json::Value>(&call.arguments)
                                .ok()
                                .and_then(|value| value.get("path").and_then(|path| path.as_str()).map(str::to_string))
                                .is_some_and(|path| path == ".");
                        let key = format!("{}:{}", call.name, call.arguments);
                        !invalid_directory_read && legacy_seen.insert(key)
                    }) {
                        tool_calls.push(call);
                    }
                }
            }
            if tool_calls.is_empty() {
                if let Some(call) = parse_natural_tool_intent(&result.content, iteration) {
                    write_model_trace(
                        "natural.tool_intent.parsed",
                        serde_json::json!({
                            "task_id": task_id,
                            "tool_call": call
                        }),
                    );
                    tool_calls.push(call);
                }
            }
            if tool_calls.is_empty() {
                if let Some(call) = parse_tagged_tool_call(&result.content, iteration) {
                    write_model_trace(
                        "tagged.tool_call.parsed",
                        serde_json::json!({
                            "task_id": task_id,
                            "tool_call": call
                        }),
                    );
                    tool_calls.push(call);
                }
            }
            if tool_calls.is_empty() {
                if let Some(call) = parse_plain_tool_call(&result.content, iteration) {
                    write_model_trace(
                        "plain.tool_call.parsed",
                        serde_json::json!({
                            "task_id": task_id,
                            "tool_call": call
                        }),
                    );
                    tool_calls.push(call);
                }
            }
            if tool_calls.is_empty() {
                if let Some(call) = parse_xml_named_tool_call(&result.content, iteration) {
                    write_model_trace(
                        "xml.tool_call.parsed",
                        serde_json::json!({
                            "task_id": task_id,
                            "tool_call": call
                        }),
                    );
                    tool_calls.push(call);
                }
            }
            tool_calls.retain(|call| {
                seen_tool_calls.insert(format!("{}:{}", call.name, call.arguments))
            });
            if tool_calls.is_empty() {
                let missing =
                    delivery_requirements.missing(mutation_done, verification_done, commit_done);
                if !missing.is_empty() && iteration + 1 < self.max_iterations {
                    let continuation = format!(
                        "Задача ещё не завершена. Обязательные результаты не выполнены: {}. Не пиши план и не заверши ответ текстом. Немедленно вызови нужный инструмент: для изменения — filesystem.patch или filesystem.write, для проверки — shell.execute с тестом/сборкой, для commit — git.commit. Выполни следующий шаг прямо сейчас.",
                        missing.join(", ")
                    );
                    write_model_trace(
                        "task.delivery_gate",
                        serde_json::json!({
                            "task_id": task_id,
                            "missing": missing,
                            "continuation": continuation
                        }),
                    );
                    messages.push(ChatMessage::text(ChatRole::Assistant, result.content));
                    messages.push(ChatMessage::text(ChatRole::User, continuation));
                    continue;
                }
                if !missing.is_empty() {
                    let message = format!(
                        "Задача не завершена: не выполнены обязательные результаты: {}.",
                        missing.join(", ")
                    );
                    let _ = events.send(CoreEvent::TaskFailed {
                        task_id,
                        error: message.clone(),
                    });
                    return Ok(message);
                }
                let final_message = strip_legacy_function_blocks(&result.content);
                let _ = events.send(CoreEvent::TaskCompleted {
                    task_id,
                    final_message: final_message.clone(),
                });
                return Ok(final_message);
            }

            messages.push(ChatMessage::assistant_tool_calls(
                result.content,
                tool_calls.clone(),
            ));
            for call in tool_calls {
                let _ = events.send(CoreEvent::ToolStarted {
                    task_id: task_id.clone(),
                    tool_name: call.name.clone(),
                });
                write_model_trace(
                    "tool.started",
                    serde_json::json!({
                        "task_id": task_id,
                        "tool_name": call.name,
                        "arguments": call.arguments
                    }),
                );
                let input = serde_json::from_str(&call.arguments)
                    .unwrap_or_else(|_| serde_json::Value::Null);
                let output = match tokio::select! {
                    _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
                    result = self.tools.execute_with_cancellation(&context, &call.name, input, cancellation.clone()) => result,
                } {
                    Ok(result) => result.output,
                    Err(evohime_tool_runtime::ToolError::NeedsApproval {
                        tool,
                        permission,
                        scope,
                        approval_id,
                        input,
                    }) => {
                        let receiver = self.approvals.register(approval_id).await;
                        let _ = events.send(CoreEvent::ApprovalRequired {
                            task_id: task_id.clone(),
                            approval_id: approval_id.to_string(),
                            tool_name: tool.clone(),
                            permission: format!("{permission:?}"),
                            scope,
                            input: input.clone(),
                        });
                        let granted = tokio::select! {
                            _ = cancellation.cancelled() => return Err(AgentRunError::Cancelled),
                            result = receiver => result.unwrap_or(false),
                        };
                        if !granted {
                            "approval denied".to_string()
                        } else {
                            match self.tools.execute_after_approval(
                                &context,
                                &tool,
                                input,
                                approval_id,
                                cancellation.clone(),
                            ).await {
                                Ok(result) => result.output,
                                Err(error) => error.to_string(),
                            }
                        }
                    }
                    Err(error) => error.to_string(),
                };
                let _ = events.send(CoreEvent::ToolOutput {
                    task_id: task_id.clone(),
                    tool_name: call.name.clone(),
                    output: output.clone(),
                });
                write_model_trace(
                    "tool.output",
                    serde_json::json!({
                        "task_id": task_id,
                        "tool_name": call.name,
                        "output": output
                    }),
                );
                let failed = output.to_lowercase().contains("failed")
                    || output.to_lowercase().contains("ошиб")
                    || output.to_lowercase().contains("не удалось");
                mutation_done |= !failed
                    && matches!(call.name.as_str(), "filesystem.write" | "filesystem.patch");
                commit_done |= !failed && call.name == "git.commit";
                if call.name == "shell.execute" && !failed {
                    let arguments = call.arguments.to_lowercase();
                    verification_done |= arguments.contains("test")
                        || arguments.contains("check")
                        || arguments.contains("build")
                        || arguments.contains("собер");
                }
                messages.push(ChatMessage::tool_observation(call.id, output));
            }
        }

        let message = "agent exceeded the tool iteration limit".to_string();
        let _ = events.send(CoreEvent::TaskFailed {
            task_id,
            error: message.clone(),
        });
        Ok(message)
    }
}

impl TaskExecutor for ToolAgent {
    fn execute(
        &self,
        task_id: String,
        prompt: String,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        self.execute_in_workspace(
            task_id,
            prompt,
            std::env::current_dir().unwrap_or_default(),
            cancellation,
            events,
        )
    }

    fn execute_in_workspace(
        &self,
        task_id: String,
        prompt: String,
        workspace_root: PathBuf,
        cancellation: CancellationToken,
        events: broadcast::Sender<CoreEvent>,
    ) -> BoxFuture<'static, Result<String, AgentRunError>> {
        let agent = Self {
            gateway: Arc::clone(&self.gateway),
            tools: Arc::clone(&self.tools),
            max_iterations: self.max_iterations,
            approvals: self.approvals.clone(),
        };
        Box::pin(async move {
            agent
                .run_once_with_cancellation(task_id, prompt, workspace_root, &events, cancellation)
                .await
        })
    }
}

#[derive(Clone)]
pub struct TaskCoordinator {
    commands: mpsc::Sender<CoreCommand>,
}

struct CoordinatorState {
    tasks: HashMap<String, CancellationToken>,
    events: broadcast::Sender<CoreEvent>,
    executor: Option<Arc<dyn TaskExecutor>>,
    journal: Option<EventJournal>,
}

impl TaskCoordinator {
    pub fn new(buffer: usize) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, None, None)
    }

    pub fn new_with_executor(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, None)
    }

    pub fn new_with_journal(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: EventJournal,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        Self::build(buffer, executor, Some(journal))
    }

    fn build(
        buffer: usize,
        executor: Option<Arc<dyn TaskExecutor>>,
        journal: Option<EventJournal>,
    ) -> (Self, broadcast::Receiver<CoreEvent>) {
        let (commands, mut command_rx) = mpsc::channel(buffer.max(1));
        let (events, event_rx) = broadcast::channel(buffer.max(1));
        let state = Arc::new(Mutex::new(CoordinatorState {
            tasks: HashMap::new(),
            events: events.clone(),
            executor,
            journal: journal.clone(),
        }));
        if let Some(journal) = journal {
            let mut journal_receiver = events.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = journal_receiver.recv().await {
                    let _ = journal.record(&event).await;
                }
            });
        }
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::handle_command(Arc::clone(&worker_state), command).await;
            }
        });
        (Self { commands }, event_rx)
    }

    pub async fn dispatch(
        &self,
        command: CoreCommand,
    ) -> Result<(), mpsc::error::SendError<CoreCommand>> {
        self.commands.send(command).await
    }

    async fn handle_command(state: Arc<Mutex<CoordinatorState>>, command: CoreCommand) {
        match command {
            CoreCommand::StartTask {
                task_id,
                prompt,
                workspace_root,
            } => {
                let cancellation = CancellationToken::new();
                let mut state_guard = state.lock().await;
                if state_guard
                    .tasks
                    .insert(task_id.clone(), cancellation.clone())
                    .is_some()
                {
                    return;
                }
                let _ = state_guard.events.send(CoreEvent::TaskStarted {
                    task_id: task_id.clone(),
                    prompt: prompt.clone(),
                });
                let events = state_guard.events.clone();
                let executor = state_guard.executor.clone();
                drop(state_guard);
                tokio::spawn(async move {
                    let result = match executor {
                        Some(executor) => match timeout(
                            Duration::from_secs(
                                std::env::var("EVOHIME_TASK_TIMEOUT_SECONDS")
                                    .ok()
                                    .and_then(|value| value.parse().ok())
                                    .unwrap_or(60),
                            ),
                            executor.execute_in_workspace(
                                task_id.clone(),
                                prompt,
                                workspace_root.unwrap_or_else(|| {
                                    std::env::current_dir().unwrap_or_default()
                                }),
                                cancellation.clone(),
                                events.clone(),
                            ),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => Err(AgentRunError::Timeout(60)),
                        },
                        None => {
                            cancellation.cancelled().await;
                            Err(AgentRunError::Cancelled)
                        }
                    };
                    let mut state_guard = state.lock().await;
                    state_guard.tasks.remove(&task_id);
                    match result {
                        Err(AgentRunError::Cancelled) => {
                            let _ = state_guard.events.send(CoreEvent::TaskStopped { task_id });
                        }
                        Err(error) => {
                            let _ = state_guard.events.send(CoreEvent::TaskFailed {
                                task_id,
                                error: error.to_string(),
                            });
                        }
                        Ok(_) => {}
                    }
                });
            }
            CoreCommand::StopTask { task_id } => {
                let mut state_guard = state.lock().await;
                if let Some(cancellation) = state_guard.tasks.remove(&task_id) {
                    cancellation.cancel();
                }
            }
            CoreCommand::CreateProject {
                client_id,
                request_id,
                command_hash,
                project_id,
                title,
                workspace_path,
                source_ref,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let project = journal
                        .create_project(&project_id, &title, &workspace_path, source_ref.as_deref())
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "project_id": project.id,
                        "title": project.title,
                        "workspace_path": project.workspace_path,
                        "version": project.version,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::CreateTask {
                client_id,
                request_id,
                command_hash,
                item,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let created = journal
                        .create_work_item(&item)
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "task_id": created.id,
                        "project_id": created.project_id,
                        "status": created.status,
                        "version": created.version,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::UpdateTaskStatus {
                client_id,
                request_id,
                command_hash,
                task_id,
                expected_version,
                status,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let updated = journal
                        .update_work_item_status(&task_id, expected_version, &status)
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "task_id": updated.id,
                        "status": updated.status,
                        "version": updated.version,
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::AddTaskEdge {
                client_id,
                request_id,
                command_hash,
                from_task_id,
                to_task_id,
                kind,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    journal
                        .add_dependency(&from_task_id, &to_task_id, &kind)
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = br#"{"from_task_id":"ok"}"#.to_vec();
                    journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, &result)
                        .await
                        .map_err(|error| error.to_string())?;
                    Ok(result)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskGraph { project_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let (tasks, edges) = journal
                        .list_task_graph(&project_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "project_id": project_id,
                        "tasks": tasks,
                        "edges": edges.into_iter().map(|(from, to, kind)| serde_json::json!({
                            "from_task_id": from,
                            "to_task_id": to,
                            "kind": kind,
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::NextReadyTask { project_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let task = journal
                        .next_ready_task(&project_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "project_id": project_id,
                        "task": task,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentRunError, CoreCommand, CoreEvent, CoreVersion, EventJournal, ModelAgent,
        TaskCoordinator, TaskExecutor, ToolAgent,
    };
    use evohime_model_gateway::{
        providers::mock::MockProvider, ChatResult, ModelGateway, NativeToolCall,
    };
    use evohime_tool_runtime::ToolRegistry;
    use futures_util::future::BoxFuture;
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct NeverExecutor;

    #[tokio::test]
    async fn approval_coordinator_resolves_pending_request_once() {
        let coordinator = super::ApprovalCoordinator::default();
        let approval_id = uuid::Uuid::new_v4();
        let receiver = coordinator.register(approval_id).await;

        assert!(coordinator.resolve(approval_id, true).await);
        assert!(!coordinator.resolve(approval_id, false).await);
        assert!(receiver.await.expect("approval response"));
    }

    #[test]
    fn agent_identity_includes_short_name() {
        assert!(super::AGENT_IDENTITY_PROMPT.contains("Ева"));
        assert!(super::AGENT_IDENTITY_PROMPT.contains("EvoHime"));
    }

    #[test]
    fn agent_system_prompt_explains_workspace_research_flow() {
        let prompt = super::build_agent_system_prompt(
            std::path::Path::new(r"C:\Projects\demo"),
            &["filesystem.list".into(), "filesystem.read".into()],
        );
        assert!(prompt.contains(r"C:\Projects\demo"));
        assert!(prompt.contains("filesystem.list"));
        assert!(prompt.contains("Не проси пользователя прислать структуру"));
        assert!(prompt.contains("до успешного результата"));
    }

    #[test]
    fn parses_legacy_text_function_calls() {
        let content = r#"
<function_calls>
<invoke name="filesystem.list">
<parameter name="path">.</parameter>
</invoke>
<invoke name="shell.execute">
<parameter name="command">dir /B</parameter>
</invoke>
</function_calls>
"#;
        let calls = super::parse_legacy_function_calls(content, 2);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filesystem.list");
        assert_eq!(calls[0].arguments, r#"{"path":"."}"#);
    }

    #[test]
    fn parses_explicit_natural_filesystem_intent() {
        let call = super::parse_natural_tool_intent(
            "Продолжу изучение. Вызываю filesystem.list для папки `crates`.",
            3,
        )
        .expect("filesystem intent");
        assert_eq!(call.name, "filesystem.list");
        assert_eq!(call.arguments, r#"{"path":"crates"}"#);
        assert!(super::parse_natural_tool_intent("Инструмент filesystem.list доступен.", 3).is_none());
    }

    impl TaskExecutor for NeverExecutor {
        fn execute(
            &self,
            _task_id: String,
            _prompt: String,
            cancellation: CancellationToken,
            _events: tokio::sync::broadcast::Sender<CoreEvent>,
        ) -> BoxFuture<'static, Result<String, AgentRunError>> {
            Box::pin(async move {
                cancellation.cancelled().await;
                Err(AgentRunError::Cancelled)
            })
        }
    }

    #[test]
    fn core_exposes_version() {
        assert!(!CoreVersion::current().is_empty());
    }

    #[tokio::test]
    async fn starts_and_stops_a_task_without_blocking_the_core() {
        let (coordinator, mut events) = TaskCoordinator::new(8);
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-1".into(),
                prompt: "hello".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        assert_eq!(
            events.recv().await.expect("started event"),
            CoreEvent::TaskStarted {
                task_id: "task-1".into(),
                prompt: "hello".into()
            }
        );
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-1".into(),
            })
            .await
            .expect("stop dispatches");
        assert_eq!(
            events.recv().await.expect("stopped event"),
            CoreEvent::TaskStopped {
                task_id: "task-1".into()
            }
        );
    }

    #[test]
    fn strips_legacy_function_blocks_from_user_facing_message() {
        let message = super::strip_legacy_function_blocks(
            "Готово.\n<function_calls><invoke name=\"filesystem.read\" /></function_calls>",
        );
        assert_eq!(message, "Готово.");
    }

    #[test]
    fn detects_delivery_requirements_from_change_request() {
        let requirements = super::DeliveryRequirements::from_prompt(
            "исправь код, проверь cargo test и создай commit",
        );
        assert!(requirements.mutation);
        assert!(requirements.verification);
        assert!(requirements.commit);
        assert_eq!(
            requirements.missing(false, true, false),
            vec!["внести изменение", "создать commit"]
        );
    }

    #[test]
    fn parses_tagged_tool_call_format() {
        let call = super::parse_tagged_tool_call(
            r#"<tool_call>filesystem.read(path="README.md")</tool_call>"#,
            4,
        )
        .expect("tagged tool call");
        assert_eq!(call.name, "filesystem.read");
        assert_eq!(call.arguments, r#"{"path":"README.md"}"#);
        let xml_call = super::parse_tagged_tool_call(
            "<tool_name>filesystem.read</tool_name><tool_input>{\"path\": \"README.md\"}</tool_input>",
            5,
        )
        .expect("structured tool call");
        assert_eq!(xml_call.name, "filesystem.read");
        assert_eq!(xml_call.arguments, r#"{"path":"README.md"}"#);
        let code_call = super::parse_tagged_tool_call(
            r#"<tool_code>filesystem.read(path="README.md")</tool_code>"#,
            6,
        )
        .expect("tool code call");
        assert_eq!(code_call.name, "filesystem.read");
        let plain_call = super::parse_plain_tool_call("filesystem.read\npath: README.md", 7)
            .expect("plain tool call");
        assert_eq!(plain_call.name, "filesystem.read");
        assert_eq!(plain_call.arguments, r#"{"path":"README.md"}"#);
        let xml_named = super::parse_xml_named_tool_call(
            "<filesystem.read><parameter>path>README.md</parameter></filesystem.read>",
            8,
        )
        .expect("xml named tool call");
        assert_eq!(xml_named.name, "filesystem.read");
        assert_eq!(xml_named.arguments, r#"{"path":"README.md"}"#);
    }

    #[tokio::test]
    async fn stop_cancels_an_active_executor() {
        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(NeverExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-cancel".into(),
                prompt: "wait".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-cancel".into(),
            })
            .await
            .expect("stop dispatches");
        assert_eq!(
            events.recv().await.expect("stopped event"),
            CoreEvent::TaskStopped {
                task_id: "task-cancel".into()
            }
        );
    }

    #[tokio::test]
    async fn streams_a_model_response_as_core_events() {
        let gateway = ModelGateway::from_provider(Arc::new(MockProvider::new(
            "mock",
            vec!["hello ".into(), "from core".into()],
        )));
        let agent = ModelAgent::new(Arc::new(gateway));
        let (events, mut receiver) = tokio::sync::broadcast::channel(8);
        let result = agent
            .run_once("task-2", "say hello", &events)
            .await
            .expect("mock model succeeds");
        assert_eq!(result, "hello from core");
        assert_eq!(
            receiver.recv().await.expect("first delta"),
            CoreEvent::AssistantDelta {
                task_id: "task-2".into(),
                content: "hello ".into()
            }
        );
        assert_eq!(
            receiver.recv().await.expect("second delta"),
            CoreEvent::AssistantDelta {
                task_id: "task-2".into(),
                content: "from core".into()
            }
        );
        assert_eq!(
            receiver.recv().await.expect("completed event"),
            CoreEvent::TaskCompleted {
                task_id: "task-2".into(),
                final_message: "hello from core".into()
            }
        );
    }

    #[tokio::test]
    async fn executes_a_safe_filesystem_tool_and_returns_to_the_model() {
        let workspace =
            std::env::temp_dir().join(format!("evohime-core-tool-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&workspace);
        std::fs::write(workspace.join("needle.txt"), "needle in a file").expect("fixture writes");
        let provider = MockProvider::with_tool_call_sequence(
            "mock",
            vec![
                ChatResult {
                    content: String::new(),
                    thinking: None,
                    tool_calls: vec![NativeToolCall {
                        id: "call-1".into(),
                        name: "filesystem.search".into(),
                        arguments: r#"{"query":"needle"}"#.into(),
                    }],
                    usage: None,
                },
                ChatResult {
                    content: "found it".into(),
                    ..ChatResult::default()
                },
            ],
        );
        let agent = ToolAgent::new(
            Arc::new(ModelGateway::from_provider(Arc::new(provider))),
            Arc::new(ToolRegistry::bootstrap()),
        );
        let (events, mut receiver) = tokio::sync::broadcast::channel(16);
        let result = agent
            .run_once("task-tools", "find needle", &workspace, &events)
            .await
            .expect("tool loop succeeds");
        assert_eq!(result, "found it");
        assert!(matches!(
            receiver.recv().await,
            Ok(CoreEvent::ModelContext { workspace_path, .. }) if workspace_path == workspace.display().to_string()
        ));
        assert!(matches!(
            receiver.recv().await,
            Ok(CoreEvent::ToolStarted { .. })
        ));
        assert!(
            matches!(receiver.recv().await, Ok(CoreEvent::ToolOutput { output, .. }) if output.contains("needle"))
        );
        assert!(
            matches!(receiver.recv().await, Ok(CoreEvent::TaskCompleted { final_message, .. }) if final_message == "found it")
        );
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[tokio::test]
    async fn journals_core_events_and_replays_after_a_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-core-journal-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-journal".into(),
                prompt: "persist me".into(),
            })
            .await
            .expect("event records");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-journal".into(),
                final_message: "done".into(),
            })
            .await
            .expect("second event records");
        let replay = journal.replay(first, 10).await.expect("events replay");
        assert_eq!(replay.len(), 1);
        assert_eq!(replay[0].event_type, "task.completed");
        assert_eq!(replay[0].task_id, "task-journal");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn coordinator_journal_captures_lifecycle_events() {
        let path = std::env::temp_dir().join(format!(
            "evohime-core-coordinator-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, mut events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-persisted".into(),
                prompt: "persist lifecycle".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        let _ = events.recv().await.expect("started event");
        coordinator
            .dispatch(CoreCommand::StopTask {
                task_id: "task-persisted".into(),
            })
            .await
            .expect("stop dispatches");
        let _ = events.recv().await.expect("stopped event");
        let mut replay = Vec::new();
        for _ in 0..20 {
            replay = journal.replay(0, 10).await.expect("replay works");
            if replay.len() >= 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].event_type, "task.started");
        assert_eq!(replay[1].event_type, "task.stopped");
        let _ = std::fs::remove_file(path);
    }
}
