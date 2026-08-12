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
- Если пользователь не сформулировал конкретное поручение, не исследуй workspace и не имитируй выполненную работу: задай один короткий уточняющий вопрос и дождись задачи.\n\
- За один ответ вызывай только один инструмент и жди его результата перед следующим вызовом.\n\
- Если пользователь просит изучить, проверить, найти или объяснить проект, сначала вызови filesystem.list с path точкой (.).\n\
- Затем прочитай подходящие manifest-файлы и документацию (например Cargo.toml, *.csproj, package.json, README и архитектурные документы), а для поиска по коду используй filesystem.search.\n\
- Для изучения проекта не используй shell.execute: filesystem.list, filesystem.read и filesystem.search безопаснее и достаточно информативны.\n\
- Не проси пользователя прислать структуру проекта, путь или команды, если workspace уже указан.\n\
- Не утверждай, что изучила файл или выполнила действие, пока соответствующий инструмент не вернул результат.\n\
- Для чтения используй безопасные read-only инструменты. Перед изменениями и опасными действиями учитывай approval.\n\
- Аргументы инструментов всегда передавай полным JSON-объектом: filesystem.list — {{\"path\":\".\"}}, filesystem.read — {{\"path\":\"README.md\"}}, filesystem.search — {{\"query\":\"pattern\",\"path\":\".\"}}; не вызывай их с пустым объектом и не передавай абсолютный путь вместо workspace-relative path.\n\
- Если инструмент вернул ошибку, не завершай задачу: исправь аргументы по тексту ошибки и повтори вызов.\n\
- filesystem.patch принимает только JSON {{\"path\":\"relative/file.rs\",\"patch\":\"--- a/relative/file.rs\\n+++ b/relative/file.rs\\n@@ ...\"}}; поля edits и patches недопустимы.\n\
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
                "patch": { "type": "string", "description": "Required unified diff beginning with --- and +++; do not use edits, patches, JSON operations, or complete file content" }
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

fn audit_log_path() -> PathBuf {
    let data_dir = std::env::var_os("EVOHIME_DATA_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA").map(|path| PathBuf::from(path).join("EvoHime"))
        })
        .unwrap_or_else(|| PathBuf::from(".evohime"));
    data_dir.join("logs").join("audit.jsonl")
}

fn append_audit_line(line: &str) {
    let path = audit_log_path();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
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
    let supported_names = [
        "filesystem.list",
        "filesystem.read",
        "filesystem.search",
        "filesystem.write",
        "filesystem.patch",
        "shell.execute",
        "git.status",
        "git.diff",
        "git.commit",
    ];

    let mut json_cursor = 0;
    while let Some(relative_start) = content[json_cursor..].find("<function_calls>") {
        let start = json_cursor + relative_start + "<function_calls>".len();
        let Some(relative_end) = content[start..].find("</function_calls>") else {
            break;
        };
        let end = start + relative_end;
        let body = content[start..end].trim();
        if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(body) {
            for item in items {
                let Some(name) = item
                    .get("tool_name")
                    .or_else(|| item.get("name"))
                    .and_then(serde_json::Value::as_str)
                else {
                    continue;
                };
                if !supported_names.contains(&name) {
                    continue;
                }
                let arguments = item
                    .get("arguments")
                    .or_else(|| item.get("parameters"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                calls.push(NativeToolCall {
                    id: format!("legacy-json-{iteration}-{}", calls.len()),
                    name: name.to_string(),
                    arguments: arguments.to_string(),
                });
            }
        }
        json_cursor = end + "</function_calls>".len();
    }

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
            "filesystem.list" | "filesystem.read" | "filesystem.search"
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

    if let Some(json_body) = content
        .split("```json")
        .nth(1)
        .and_then(|body| body.split("```").next())
        .and_then(|body| serde_json::from_str::<serde_json::Value>(body.trim()).ok())
    {
        let arguments = if name == "filesystem.search" {
            json_body
        } else if json_body.get("path").is_some() {
            json_body
        } else {
            serde_json::json!({ "path": "." })
        };
        return Some(NativeToolCall {
            id: format!("natural-{iteration}"),
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
    }

    if !explicit_action {
        return None;
    }

    let path = content
        .split('`')
        .nth(1)
        .filter(|value| !value.contains('.'))
        .unwrap_or(if name == "filesystem.list" { "." } else { "" });
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
        let wrapped = format!(
            "<tool_call>{}</tool_call>",
            content[body_start..body_end].trim()
        );
        return parse_tagged_tool_call(&wrapped, iteration);
    }
    if let (Some(name_start), Some(input_start)) =
        (content.find("<tool_name>"), content.find("<tool_input>"))
    {
        let name_start = name_start + "<tool_name>".len();
        let name_end = content[name_start..].find("</tool_name>")? + name_start;
        let input_start = input_start + "<tool_input>".len();
        let input_end = content[input_start..].find("</tool_input>")? + input_start;
        let name = content[name_start..name_end].trim();
        if !matches!(
            name,
            "filesystem.list" | "filesystem.read" | "filesystem.search" | "git.status" | "git.diff"
        ) {
            return None;
        }
        let arguments =
            serde_json::from_str::<serde_json::Value>(content[input_start..input_end].trim())
                .ok()?;
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
            arguments.insert(
                key.trim().to_string(),
                serde_json::Value::String(value.to_string()),
            );
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
    let name = [
        "filesystem.list",
        "filesystem.read",
        "filesystem.search",
        "git.status",
        "git.diff",
    ]
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
    let name = [
        "filesystem.list",
        "filesystem.read",
        "filesystem.search",
        "git.status",
        "git.diff",
    ]
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
    research: bool,
    mutation: bool,
    verification: bool,
    diff_check: bool,
    commit: bool,
}

impl DeliveryRequirements {
    fn from_prompt(prompt: &str) -> Self {
        let prompt = prompt.to_lowercase();
        Self {
            research: ["изучи", "исслед", "ознаком", "найди", "объясни"]
                .iter()
                .any(|marker| prompt.contains(marker)),
            mutation: [
                "исправ",
                "измен",
                "добав",
                "реализ",
                "сделай",
                "улучш",
                "удал",
                "убер",
            ]
            .iter()
            .any(|marker| prompt.contains(marker)),
            verification: ["проверь", "провер", "тест", "test", "собери", "запусти"]
                .iter()
                .any(|marker| prompt.contains(marker)),
            diff_check: prompt.contains("git diff --check"),
            commit: prompt.contains("коммит") || prompt.contains("commit"),
        }
    }

    fn missing(
        self,
        research_done: bool,
        mutation_done: bool,
        verification_done: bool,
        commit_done: bool,
    ) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.research && !research_done {
            missing.push("изучить workspace и подготовить отчёт");
        }
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

fn delivery_next_step(
    requirements: DeliveryRequirements,
    research_done: bool,
    mutation_done: bool,
    verification_done: bool,
    commit_done: bool,
) -> &'static str {
    if requirements.research && !research_done {
        "НЕМЕДЛЕННО вызови следующий нужный read-only инструмент с полным JSON и продолжи исследование. Не пиши отчёт."
    } else if !mutation_done && requirements.mutation {
        "НЕМЕДЛЕННО вызови filesystem.patch или filesystem.write и внеси требуемое изменение. Не вызывай read/search и не пиши отчёт."
    } else if !verification_done && requirements.verification {
        "НЕМЕДЛЕННО вызови shell.execute с требуемым тестом/проверкой. Не пиши отчёт."
    } else if !commit_done && requirements.commit {
        "НЕМЕДЛЕННО вызови git.commit с task-only сообщением. Не пиши отчёт."
    } else {
        "НЕМЕДЛЕННО вызови следующий нужный read-only инструмент с полным JSON и продолжи исследование. Не пиши отчёт."
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
    EventRecord, ImportedTask, LocalDatabase, ProjectPolicyRecord, RecoveryState,
    RunCheckpointRecord, RunEffectRecord, RunRecord, RunRecoveryRecord, StorageError,
    WorkItemRecord,
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

pub mod audit;
pub mod build;
pub mod capability_registry;
pub mod child_roles;
pub mod child_runtime;
pub mod doctor;
pub mod memory_api;
pub mod memory_domain;
pub mod observability;
pub mod permission_rules;
pub mod plan;
pub mod prd;
pub mod research;
pub mod research_fetch;
pub mod research_pipeline;
pub mod scope;
pub mod workflow;
pub mod workflow_runner;
pub mod workspace;

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
    ImportPrd {
        client_id: String,
        request_id: String,
        command_hash: String,
        import_id: String,
        project_id: String,
        origin: String,
        version: String,
        source_text: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskHistory {
        task_id: String,
        limit: usize,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskContext {
        project_id: String,
        task_id: String,
        max_chars: usize,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskPlanSpec {
        project_id: String,
        task_id: String,
        max_chars: usize,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetTaskSnapshot {
        project_id: String,
        task_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    RestoreTaskSnapshot {
        project_id: String,
        task_id: String,
        snapshot_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    GetBuildPolicy {
        project_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    SaveBuildPolicy {
        project_id: String,
        policy_json: Vec<u8>,
        expected_version: i64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    ApplyApprovedBuild {
        project_id: String,
        run_id: String,
        task_id: String,
        approved_build_json: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    PrepareBuild {
        project_id: String,
        proposal_json: Vec<u8>,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Bounded, read-only Core Doctor diagnostic. `project_id` is optional;
    /// when set, the permissions probe is grounded in that project's real
    /// workspace path. `protocol_major`/`expected_protocol_major` and
    /// `provider`/`approval_required` are supplied by the IPC layer, which
    /// is where that state actually lives.
    RunDoctor {
        project_id: String,
        protocol_major: Option<u32>,
        expected_protocol_major: u32,
        provider: crate::doctor::ProviderProbe,
        approval_required: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Captures one bounded, redacted piece of offline research evidence and
    /// persists it against the real `research_evidence` table, tied to
    /// `work_item_id` via `provenance_link`. Redaction and validation happen
    /// in `research::ResearchEvidence::capture` before anything is stored.
    SaveResearchEvidence {
        work_item_id: String,
        source_kind: String,
        source_ref: String,
        title: String,
        publisher: String,
        content_type: String,
        raw_excerpt: String,
        retrieved_at_ms: u64,
        ttl_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists previously saved research evidence for a work item.
    ListResearchEvidence {
        work_item_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Performs a real, policy-gated, SSRF-protected HTTP GET against `url`,
    /// driving `research_fetch::run_research_fetch` through the real
    /// `research_pipeline` state machine, then persists the resulting
    /// `ResearchEvidence` the same way `SaveResearchEvidence` does. `title`
    /// is caller-supplied; content-type/publisher are derived from the
    /// response and URL. No search-engine integration and no LLM-based
    /// summarization happen here (see `research_fetch` module docs).
    RunResearchFetch {
        work_item_id: String,
        url: String,
        title: String,
        allowed_domains: Vec<String>,
        max_bytes: u64,
        max_latency_ms: u64,
        max_cost_micros: u64,
        ttl_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Creates one bounded Memory v1 record. `memory_domain::MemoryDomain`
    /// runs validation, TTL expansion and content redaction server-side
    /// (its in-memory storage is not used: the real `memory_entries` table,
    /// via `memory_store`, is the sole source of truth); `id` and
    /// `created_at_ms` are computed here, never trusted from the caller.
    CreateMemory {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        title: String,
        content: String,
        provenance_kind: String,
        provenance_id: String,
        provenance_locator: String,
        privacy: String,
        ttl_ms: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists non-forgotten Memory v1 records for one exact scope.
    ListMemory {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        include_archived: bool,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lexical, deterministic search over Memory v1 records for one exact
    /// scope.
    SearchMemory {
        scope_kind: String,
        project_id: String,
        secondary_id: String,
        query: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Archives a memory record. Per the Memory v1 plan, this requires an
    /// out-of-band approval token (`approval_id`), validated the same way
    /// `memory_api::Approval` validates it: mirrors the `ApplyApprovedBuild`
    /// trust model, where the client presents proof that the operation was
    /// already approved before this command is sent.
    ArchiveMemory {
        id: String,
        approval_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Permanently erases a memory record's title/content. Also requires an
    /// out-of-band approval token; see `ArchiveMemory`.
    ForgetMemory {
        id: String,
        approval_id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Installs (or, when a manifest of the same name already exists,
    /// updates) one bounded capability manifest into the local catalog.
    /// `manifest_json` is validated via
    /// `capability_registry::CapabilityManifest`'s own bounds plus
    /// `validate_registry`/`validate_update` against the manifests already
    /// persisted, before anything is written. `local_archive` carries only
    /// an audit path. `https_archive` treats `source_path` as an HTTPS URL,
    /// downloads it through the shared SSRF guard, and requires the trusted
    /// out-of-band SHA-256 in `expected_content_hash` to match before any
    /// catalog write.
    InstallCapability {
        manifest_json: String,
        install_source: String,
        source_path: String,
        expected_content_hash: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists installed capability manifests, newest-first.
    ListCapabilities {
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Deterministic intent/tool/domain match against the installed
    /// catalog, via `capability_registry::match_capabilities`.
    MatchCapabilities {
        intent: String,
        required_tools: Vec<String>,
        required_domains: Vec<String>,
        requested_risk: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Removes one installed capability manifest by id (manifest name).
    RemoveCapability {
        id: String,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates and persists one bounded, redacted task handoff between
    /// child roles (`child_roles::HandoffEnvelope::new`). This only records
    /// the handoff; it does not deliver or act on it for any real child
    /// agent -- runtime wiring remains a later, dedicated task per
    /// `child_roles.rs`'s own scope note.
    RequestChildHandoff {
        handoff_id: String,
        task_id: String,
        kind: String,
        from_role: String,
        from_name: String,
        to_role: String,
        to_name: String,
        purpose: String,
        payload: std::collections::HashMap<String, String>,
        sequence: u64,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Lists persisted child handoffs for a task, in sequence order.
    ListChildHandoffs {
        task_id: String,
        limit: u32,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates (`child_runtime::ChildTaskRequest::validate`) and persists
    /// one bounded, read-only child task request. Rejects any request with
    /// a non-read-only `requested_capabilities` entry, any nested child
    /// (`parent_is_child = true`), or oversized context/output -- the same
    /// pure contract used by the unit tests, enforced end-to-end here. Core
    /// does not act on an accepted request: it is stored as a durable
    /// record of an approved read-only child task descriptor for whatever
    /// later spawns it (out of scope for this task).
    SubmitChildRequest {
        child_task_id: String,
        parent_task_id: String,
        role: String,
        kind: String,
        reduced_context: Vec<String>,
        max_output_bytes: u32,
        requested_capabilities: Vec<String>,
        parent_is_child: bool,
        reply: oneshot::Sender<Result<Vec<u8>, String>>,
    },
    /// Validates (`child_runtime::accept_report`, against the matching
    /// stored `SubmitChildRequest`) and persists one child report. Rejects
    /// a task-id mismatch, secret-like content, duplicate sources, or a
    /// missing/invalid matching request.
    SubmitChildReport {
        child_task_id: String,
        status: String,
        summary: String,
        findings: Vec<String>,
        sources: Vec<String>,
        confidence_percent: u32,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DurableReplayBatch {
    pub events: Vec<EventRecord>,
    pub gap_detected: bool,
    pub first_available_sequence: Option<i64>,
    pub last_sequence: i64,
}

fn default_build_policy() -> crate::scope::BuildScope {
    crate::scope::BuildScope {
        allowed_paths: Vec::new(),
        allowed_operations: vec!["write".into(), "create".into()],
        expected_outputs: Vec::new(),
        protected_paths: vec![".git".into(), ".evohime".into()],
        allowed_file_types: Vec::new(),
        max_files_changed: 20,
        max_bytes_changed: 2 * 1024 * 1024,
        allow_create: true,
        allow_delete: false,
        allow_rename: false,
        baseline_snapshot_id: None,
        acceptance_criteria: String::new(),
        risk_class: "medium".into(),
        timeout_ms: 30_000,
    }
}

fn harden_build_policy(mut policy: crate::scope::BuildScope) -> crate::scope::BuildScope {
    for required in [".git", ".evohime"] {
        if !policy.protected_paths.iter().any(|path| path == required) {
            policy.protected_paths.push(required.into());
        }
    }
    policy
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

    pub async fn replay_bounded(
        &self,
        after_sequence: i64,
        limit: usize,
    ) -> Result<DurableReplayBatch, StorageError> {
        const MAX_DURABLE_REPLAY_EVENTS: usize = 512;
        let records = {
            let database = self.database.lock().await;
            database.read_events_after(after_sequence, limit.min(MAX_DURABLE_REPLAY_EVENTS))?
        };
        let first_available_sequence = records.first().map(|record| record.sequence_id);
        let gap_detected =
            first_available_sequence.is_some_and(|first| after_sequence.saturating_add(1) < first);
        let last_sequence = records
            .last()
            .map(|record| record.sequence_id)
            .unwrap_or(after_sequence);
        Ok(DurableReplayBatch {
            events: records,
            gap_detected,
            first_available_sequence,
            last_sequence,
        })
    }

    /// Bounded, read-only storage facts for diagnostics (Core Doctor).
    pub async fn storage_snapshot(&self) -> Result<(PathBuf, u32), StorageError> {
        let database = self.database.lock().await;
        Ok((database.path().to_path_buf(), database.schema_version()?))
    }

    /// Bounded, read-only recovery facts for diagnostics (Core Doctor). This
    /// only performs SELECTs and never mutates run/effect state.
    pub async fn recovery_probe(&self) -> Result<crate::doctor::RecoveryProbe, StorageError> {
        let database = self.database.lock().await;
        let health = database.read_recovery_health()?;
        let state = if health.unknown_effects > 0 || health.lease_expired {
            "BLOCKED"
        } else if health.resumable_runs > 0 {
            "RESUMABLE"
        } else {
            "CLEAN"
        };
        Ok(crate::doctor::RecoveryProbe {
            state: state.into(),
            unknown_effects: health.unknown_effects.max(0) as u32,
            lease_expired: health.lease_expired,
            resumable_runs: health.resumable_runs.max(0) as u32,
        })
    }

    pub async fn transition_recovery(
        &self,
        run_id: &str,
        state: RecoveryState,
        effect_id: &str,
        idempotency_key: &str,
        verifier: &str,
        evidence_json: &[u8],
        decision: &str,
    ) -> Result<RunRecoveryRecord, StorageError> {
        let database = self.database.lock().await;
        database.transition_recovery(
            run_id,
            state,
            effect_id,
            idempotency_key,
            verifier,
            evidence_json,
            decision,
        )
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

    pub async fn get_project(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::ProjectRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_project(id)
    }

    /// Persists one redacted, bounded research evidence record against the
    /// real `research_evidence` table (SCHEMA_VERSION 8).
    pub async fn save_research_evidence(
        &self,
        record: &evohime_local_storage::research_store::ResearchEvidenceRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::research_store::ResearchEvidenceSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists research evidence records tied to a work item, oldest id first.
    pub async fn list_research_evidence(
        &self,
        work_item_id: &str,
    ) -> Result<Vec<evohime_local_storage::research_store::ResearchEvidenceRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::research_store::ResearchEvidenceSql::list_by_provenance(
            database.connection(),
            work_item_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one bounded, redacted Memory v1 record against the real
    /// `memory_entries` table (SCHEMA_VERSION 8).
    pub async fn save_memory(
        &self,
        record: &evohime_local_storage::memory_store::MemoryRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::insert(database.connection(), record)
            .map_err(|error| error.to_string())
    }

    /// Lists non-forgotten Memory v1 records for one exact scope.
    pub async fn list_memory(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
        include_archived: bool,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::list(
            database.connection(),
            scope,
            scope_id,
            include_archived,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Lexical, deterministic search over Memory v1 records for one exact
    /// scope.
    pub async fn search_memory(
        &self,
        scope: evohime_local_storage::memory_store::MemoryScope,
        scope_id: &str,
        query: &str,
        now: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::memory_store::MemoryRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::search(
            database.connection(),
            scope,
            scope_id,
            query,
            now,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Archives a memory record. Returns `false` if no matching, non-forgotten
    /// record was found.
    pub async fn archive_memory(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::archive(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Forgets (erases title/content of) a memory record. Returns `false` if
    /// no matching row was found.
    pub async fn forget_memory(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::memory_store::MemoryStoreSql::forget(database.connection(), id)
            .map_err(|error| error.to_string())
    }

    /// Installs (inserts) or updates (replaces by id) one bounded capability
    /// manifest against the real `capability_manifests` table.
    pub async fn save_capability_manifest(
        &self,
        record: &evohime_local_storage::capability_store::CapabilityManifestRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::insert(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists installed capability manifests, newest-first.
    pub async fn list_capability_manifests(
        &self,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::capability_store::CapabilityManifestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::list(
            database.connection(),
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches one installed capability manifest by id (manifest name).
    pub async fn get_capability_manifest(
        &self,
        id: &str,
    ) -> Result<Option<evohime_local_storage::capability_store::CapabilityManifestRecord>, String>
    {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::get_by_id(
            database.connection(),
            id,
        )
        .map_err(|error| error.to_string())
    }

    /// Removes one installed capability manifest by id. Returns `false` if
    /// no matching row was found.
    pub async fn remove_capability_manifest(&self, id: &str) -> Result<bool, String> {
        let database = self.database.lock().await;
        evohime_local_storage::capability_store::CapabilityStoreSql::delete_by_id(
            database.connection(),
            id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one validated child handoff envelope.
    pub async fn save_child_handoff(
        &self,
        record: &evohime_local_storage::child_store::HandoffRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::insert_handoff(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Lists persisted child handoffs for a task, in sequence order.
    pub async fn list_child_handoffs(
        &self,
        task_id: &str,
        limit: u32,
    ) -> Result<Vec<evohime_local_storage::child_store::HandoffRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::list_handoffs_by_task(
            database.connection(),
            task_id,
            limit,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one validated, read-only child task request.
    pub async fn save_child_task_request(
        &self,
        record: &evohime_local_storage::child_store::ChildTaskRequestRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::insert_child_task_request(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    /// Fetches one persisted child task request by its child_task_id.
    pub async fn get_child_task_request(
        &self,
        child_task_id: &str,
    ) -> Result<Option<evohime_local_storage::child_store::ChildTaskRequestRecord>, String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::get_child_task_request(
            database.connection(),
            child_task_id,
        )
        .map_err(|error| error.to_string())
    }

    /// Persists one accepted child report.
    pub async fn save_child_report(
        &self,
        record: &evohime_local_storage::child_store::ChildReportRecord,
    ) -> Result<(), String> {
        let database = self.database.lock().await;
        evohime_local_storage::child_store::ChildStoreSql::insert_child_report(
            database.connection(),
            record,
        )
        .map_err(|error| error.to_string())
    }

    pub async fn get_or_create_build_policy(
        &self,
        project_id: &str,
        default_policy: &crate::scope::BuildScope,
    ) -> Result<crate::scope::BuildScope, String> {
        let database = self.database.lock().await;
        if let Some(record) = database
            .get_project_policy(project_id)
            .map_err(|error| error.to_string())?
        {
            return serde_json::from_slice(&record.policy_json)
                .map(harden_build_policy)
                .map_err(|error| format!("invalid persisted build policy: {error}"));
        }
        let policy_json = serde_json::to_vec(default_policy).map_err(|error| error.to_string())?;
        database
            .upsert_project_policy(project_id, &policy_json, None)
            .map_err(|error| error.to_string())?;
        Ok(harden_build_policy(default_policy.clone()))
    }

    pub async fn get_build_policy(
        &self,
        project_id: &str,
        default_policy: &crate::scope::BuildScope,
    ) -> Result<(crate::scope::BuildScope, i64), String> {
        let database = self.database.lock().await;
        let record = match database
            .get_project_policy(project_id)
            .map_err(|error| error.to_string())?
        {
            Some(record) => record,
            None => {
                let policy_json =
                    serde_json::to_vec(default_policy).map_err(|error| error.to_string())?;
                database
                    .upsert_project_policy(project_id, &policy_json, None)
                    .map_err(|error| error.to_string())?
            }
        };
        let policy = serde_json::from_slice(&record.policy_json)
            .map(harden_build_policy)
            .map_err(|error| format!("invalid persisted build policy: {error}"))?;
        Ok((policy, record.version))
    }

    pub async fn save_build_policy(
        &self,
        project_id: &str,
        policy: &crate::scope::BuildScope,
        expected_version: Option<i64>,
    ) -> Result<ProjectPolicyRecord, String> {
        let policy_json = serde_json::to_vec(policy).map_err(|error| error.to_string())?;
        let database = self.database.lock().await;
        database
            .upsert_project_policy(project_id, &policy_json, expected_version)
            .map_err(|error| error.to_string())
    }

    pub async fn get_work_item(&self, id: &str) -> Result<Option<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_work_item(id)
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

    pub async fn import_prd(
        &self,
        provenance_id: &str,
        project_id: &str,
        origin: &str,
        version: &str,
        source_text: &str,
        tasks: &[ImportedTask],
    ) -> Result<Vec<WorkItemRecord>, StorageError> {
        let database = self.database.lock().await;
        database.import_prd(
            provenance_id,
            project_id,
            origin,
            version,
            source_text,
            tasks,
        )
    }

    pub async fn save_snapshot(
        &self,
        id: &str,
        run_id: &str,
        workspace_hash: &str,
        payload: &[u8],
    ) -> Result<evohime_local_storage::SnapshotRecord, StorageError> {
        let database = self.database.lock().await;
        database.save_snapshot(id, run_id, workspace_hash, payload)
    }

    pub async fn latest_snapshot_for_task(
        &self,
        task_id: &str,
    ) -> Result<Option<evohime_local_storage::SnapshotRecord>, StorageError> {
        let database = self.database.lock().await;
        database.latest_snapshot_for_task(task_id)
    }

    pub async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<evohime_local_storage::SnapshotRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_snapshot(snapshot_id)
    }

    pub async fn get_run(
        &self,
        run_id: &str,
    ) -> Result<Option<evohime_local_storage::RunRecord>, StorageError> {
        let database = self.database.lock().await;
        database.get_run(run_id)
    }

    pub async fn begin_build_effect(
        &self,
        run_id: &str,
        task_id: &str,
        intent_hash: &str,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect_id = format!("effect-{run_id}");
        let checkpoint = RunCheckpointRecord {
            run_id: run_id.into(),
            checkpoint_id: format!("checkpoint-{run_id}"),
            stage: "build".into(),
            node_id: "bounded-build".into(),
            attempt: 1,
            input_hash: intent_hash.into(),
            state_json: serde_json::to_vec(&serde_json::json!({
                "stage": "build", "intent_hash": intent_hash
            }))?,
            pending_effects_json: serde_json::to_vec(&vec![effect_id.clone()])?,
            committed_at: String::new(),
        };
        let effect = RunEffectRecord {
            effect_id: effect_id.clone(),
            run_id: run_id.into(),
            node_id: "bounded-build".into(),
            kind: "bounded_build".into(),
            idempotency_key: format!("{run_id}:bounded-build"),
            immutable_intent_hash: intent_hash.into(),
            state: "prepared".into(),
            started_at: None,
            completed_at: None,
            result_hash: None,
        };
        let run = RunRecord {
            id: run_id.into(),
            work_item_id: task_id.into(),
            status: "running".into(),
            policy_snapshot: Vec::new(),
            role_snapshot: Vec::new(),
            skill_snapshot: Vec::new(),
            model_route_snapshot: Vec::new(),
        };
        let stored = database.prepare_run_effect(&run, &checkpoint, &effect)?;
        if stored.immutable_intent_hash != intent_hash {
            return Err(StorageError::InvalidRunEffect(
                "intent hash conflict".into(),
            ));
        }
        match stored.state.as_str() {
            "prepared" => {
                database.acquire_run_lease(run_id, &format!("lease-{run_id}"), "core", 1, 30)?;
                database.mark_effect_executing(&effect_id)
            }
            "executing" => Err(StorageError::InvalidRunEffect(
                "effect is already executing".into(),
            )),
            "completed_success" | "completed_failure" | "unknown" => Err(
                StorageError::InvalidRunEffect(format!("effect is already {}", stored.state)),
            ),
            _ => Err(StorageError::InvalidRunEffect(format!(
                "unsupported state {}",
                stored.state
            ))),
        }
    }

    pub async fn complete_build_effect(
        &self,
        run_id: &str,
        success: bool,
        result_hash: Option<&str>,
    ) -> Result<RunEffectRecord, StorageError> {
        let database = self.database.lock().await;
        let effect =
            database.complete_run_effect(&format!("effect-{run_id}"), success, result_hash)?;
        database.update_run_status(run_id, if success { "completed" } else { "failed" })?;
        database.release_run_lease(run_id, &format!("lease-{run_id}"), "core", 1)?;
        Ok(effect)
    }

    pub async fn reconcile_build_effect(
        &self,
        run_id: &str,
        success: bool,
        evidence: &serde_json::Value,
    ) -> Result<evohime_local_storage::RunReconciliationRecord, StorageError> {
        let database = self.database.lock().await;
        let record = database.reconcile_run_effect(
            &format!("effect-{run_id}"),
            success,
            "bounded_build_snapshot",
            &serde_json::to_vec(evidence)?,
        )?;
        if success {
            database.update_run_status(run_id, "completed")?;
        }
        Ok(record)
    }

    pub async fn recover_after_restart(
        &self,
    ) -> Result<Vec<evohime_local_storage::RecoveredRunRecord>, StorageError> {
        let database = self.database.lock().await;
        database.recover_unknown_effects()
    }

    pub async fn recover_and_reconcile_after_restart(
        &self,
    ) -> Result<Vec<evohime_local_storage::RunReconciliationRecord>, StorageError> {
        let database = self.database.lock().await;
        let recovered = database.recover_unknown_effects()?;
        let mut reconciliations = Vec::with_capacity(recovered.len());
        for record in recovered {
            let snapshot = database.latest_snapshot_for_task(&record.work_item_id)?;
            let success = snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.run_id == record.run_id);
            let evidence = serde_json::json!({
                "run_id": record.run_id,
                "effect_id": record.effect_id,
                "snapshot_id": success.then(|| snapshot.as_ref().expect("successful reconciliation has snapshot").id.clone()),
                "decision": if success { "applied" } else { "blocked" },
            });
            let reconciliation = database.reconcile_run_effect(
                &record.effect_id,
                success,
                "bounded_build_snapshot",
                &serde_json::to_vec(&evidence)?,
            )?;
            if success {
                database.update_run_status(&record.run_id, "completed")?;
            }
            database.append_event(
                &record.work_item_id,
                if success {
                    "run.reconciliation.completed"
                } else {
                    "run.recovery.blocked"
                },
                &serde_json::to_vec(&evidence)?,
            )?;
            reconciliations.push(reconciliation);
        }
        Ok(reconciliations)
    }

    pub async fn record_audit(
        &self,
        subject_id: &str,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, StorageError> {
        let database = self.database.lock().await;
        database.append_event(subject_id, event_type, payload)
    }

    pub async fn task_history(
        &self,
        task_id: &str,
        limit: usize,
    ) -> Result<Vec<EventRecord>, StorageError> {
        let database = self.database.lock().await;
        database.read_task_events(task_id, limit)
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
            max_iterations: 16,
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
                ToolSpec::function(name, tool.description, tool_parameters(tool.name))
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
        let mut verification_test_passed = false;
        let mut diff_check_passed = false;
        let mut research_observations = 0usize;
        let mut research_has_overview = false;
        let mut research_has_content = false;
        let mut research_has_search = false;
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
                                .and_then(|value| {
                                    value
                                        .get("path")
                                        .and_then(|path| path.as_str())
                                        .map(str::to_string)
                                })
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
            let mut duplicate_tool_call = None;
            tool_calls.retain(|call| {
                let is_new = seen_tool_calls.insert(format!("{}:{}", call.name, call.arguments));
                if !is_new && duplicate_tool_call.is_none() {
                    duplicate_tool_call = Some(call.name.clone());
                }
                is_new
            });
            if let Some(tool_name) = duplicate_tool_call {
                messages.push(ChatMessage::text(
                    ChatRole::User,
                    format!(
                        "Ты уже выполняла точно такой вызов {tool_name}. Его повтор удалён Core. Самостоятельно выбери следующий новый шаг: используй другой подтверждённый путь или filesystem.search, затем продолжи исследование/реализацию. Не повторяй последний вызов и не завершай задачу отчётом."
                    ),
                ));
            }
            if tool_calls.is_empty() {
                let research_done = !delivery_requirements.research
                    || (research_observations >= 5
                        && research_has_overview
                        && research_has_content
                        && research_has_search);
                let missing = delivery_requirements.missing(
                    research_done,
                    mutation_done,
                    verification_done,
                    commit_done,
                );
                if !missing.is_empty() && iteration + 1 < self.max_iterations {
                    let next_step = delivery_next_step(
                        delivery_requirements,
                        research_done,
                        mutation_done,
                        verification_done,
                        commit_done,
                    );
                    let continuation = format!(
                        "Задача ещё не завершена. Не выполнены: {}. {next_step}",
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
                if delivery_requirements.research {
                    research_observations += 1;
                    research_has_overview |= call.name == "filesystem.list";
                    research_has_content |=
                        matches!(call.name.as_str(), "filesystem.read" | "filesystem.search");
                    research_has_search |= call.name == "filesystem.search";
                }
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
                let commit_blocked = call.name == "git.commit"
                    && delivery_requirements.commit
                    && (!verification_test_passed
                        || (delivery_requirements.diff_check && !diff_check_passed));
                let output = if commit_blocked {
                    "git.commit blocked: сначала успешно выполни обязательную проверку и git diff --check".to_string()
                } else {
                    match tokio::select! {
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
                                "approval denied: mutation not performed".to_string()
                            } else {
                                match self
                                    .tools
                                    .execute_after_approval(
                                        &context,
                                        &tool,
                                        input,
                                        approval_id,
                                        cancellation.clone(),
                                    )
                                    .await
                                {
                                    Ok(result) => result.output,
                                    Err(error) => error.to_string(),
                                }
                            }
                        }
                        Err(error) => error.to_string(),
                    }
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
                let failed = tool_output_failed(&output);
                mutation_done |= !failed
                    && !output.to_lowercase().contains("approval denied")
                    && matches!(call.name.as_str(), "filesystem.write" | "filesystem.patch");
                commit_done |= !failed && call.name == "git.commit";
                if call.name == "shell.execute" && !failed {
                    let arguments = call.arguments.to_lowercase();
                    if arguments.contains("diff") && arguments.contains("check") {
                        diff_check_passed = true;
                    } else if arguments.contains("test")
                        || arguments.contains("check")
                        || arguments.contains("build")
                        || arguments.contains("собер")
                    {
                        verification_test_passed = true;
                    }
                }
                if call.name == "shell.execute" && failed {
                    let arguments = call.arguments.to_lowercase();
                    if arguments.contains("diff") && arguments.contains("check") {
                        diff_check_passed = false;
                    } else if arguments.contains("test")
                        || arguments.contains("check")
                        || arguments.contains("build")
                        || arguments.contains("собер")
                    {
                        verification_test_passed = false;
                    }
                }
                verification_done = verification_test_passed
                    && (!delivery_requirements.diff_check || diff_check_passed);
                let patch_context_mismatch = output.to_lowercase().contains("patch context mismatch");
                messages.push(ChatMessage::tool_observation(call.id, output));
                if failed {
                    let recovery = if patch_context_mismatch {
                        " Для patch context mismatch сначала вызови git.diff или filesystem.read для актуального файла, затем сформируй новый patch по фактическому содержимому; старый patch не повторяй."
                    } else {
                        ""
                    };
                    messages.push(ChatMessage::text(
                        ChatRole::User,
                        format!(
                            "Инструмент {} завершился ошибкой. Не завершай задачу и не повторяй пустые аргументы.{} Повтори один вызов с полным workspace-relative JSON: filesystem.list={{\"path\":\".\"}}; filesystem.read={{\"path\":\"README.md\"}}; filesystem.search={{\"query\":\"нужный текст\",\"path\":\".\"}}. Для другого инструмента укажи все его обязательные поля.",
                            call.name, recovery
                        ),
                    ));
                }
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

fn tool_output_failed(output: &str) -> bool {
    let lower = output.to_lowercase();
    if lower.contains("exit_code:") {
        return !lower.contains("exit_code: 0");
    }
    lower.contains("failed")
        || lower.contains("approval denied")
        || lower.contains("ошиб")
        || lower.contains("не удалось")
        || lower.contains("blocked")
        || lower.contains("exit_code: 1")
        || lower.contains("exit_code: 101")
        || lower.contains("error:")
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
    state: Arc<Mutex<CoordinatorState>>,
}

struct CoordinatorState {
    tasks: HashMap<String, CancellationToken>,
    events: broadcast::Sender<CoreEvent>,
    executor: Option<Arc<dyn TaskExecutor>>,
    journal: Option<EventJournal>,
    audit: crate::audit::AuditTrail,
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
            audit: crate::audit::AuditTrail::default(),
        }));
        if let Some(journal) = journal {
            let mut journal_receiver = events.subscribe();
            tokio::spawn(async move {
                while let Ok(event) = journal_receiver.recv().await {
                    let _ = journal.record(&event).await;
                }
            });
        }
        let audit_state = Arc::clone(&state);
        let mut audit_receiver = events.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = audit_receiver.recv().await {
                Self::record_audit_for_event(&audit_state, &event).await;
            }
        });
        let worker_state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(command) = command_rx.recv().await {
                Self::handle_command(Arc::clone(&worker_state), command).await;
            }
        });
        (Self { commands, state }, event_rx)
    }

    pub async fn dispatch(
        &self,
        command: CoreCommand,
    ) -> Result<(), mpsc::error::SendError<CoreCommand>> {
        self.commands.send(command).await
    }

    /// Appends a bounded, durable audit record. Failures to append (bounds
    /// exceeded, invalid fields) are non-fatal to the caller: audit logging
    /// must never block or fail a live command.
    async fn record_audit(
        state: &Arc<Mutex<CoordinatorState>>,
        kind: crate::audit::AuditKind,
        actor: impl Into<String>,
        event_id: impl Into<String>,
        fields: impl IntoIterator<Item = (String, String)>,
    ) {
        let mut state_guard = state.lock().await;
        let sequence = state_guard.audit.records().len() as u64;
        let record = match crate::audit::AuditRecord::new(sequence, event_id, kind, actor, fields) {
            Ok(record) => record,
            Err(_) => return,
        };
        let Ok(line) = record.to_json_line() else {
            return;
        };
        if state_guard.audit.append(record).is_ok() {
            drop(state_guard);
            append_audit_line(&line);
        }
    }

    async fn record_audit_for_event(state: &Arc<Mutex<CoordinatorState>>, event: &CoreEvent) {
        match event {
            CoreEvent::ApprovalRequired {
                task_id,
                approval_id,
                tool_name,
                permission,
                scope,
                ..
            } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::Approval,
                    task_id.to_string(),
                    "approval.required",
                    [
                        ("approval_id".to_owned(), approval_id.to_string()),
                        ("tool_name".to_owned(), tool_name.to_string()),
                        ("permission".to_owned(), permission.to_string()),
                        ("scope".to_owned(), scope.to_string()),
                    ],
                )
                .await;
            }
            CoreEvent::ToolStarted { task_id, tool_name } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::ToolCall,
                    task_id.to_string(),
                    "tool.started",
                    [("tool_name".to_owned(), tool_name.to_string())],
                )
                .await;
            }
            CoreEvent::TaskFailed { task_id, error } => {
                Self::record_audit(
                    state,
                    crate::audit::AuditKind::Failure,
                    task_id.to_string(),
                    "task.failed",
                    [("error".to_owned(), error.to_string())],
                )
                .await;
            }
            _ => {}
        }
    }

    /// Returns the current in-memory audit trail as JSONL, primarily for
    /// tests and diagnostics. The durable copy lives on disk at
    /// `<data_dir>/logs/audit.jsonl`.
    pub async fn audit_jsonl(&self) -> String {
        self.state.lock().await.audit.as_jsonl().unwrap_or_default()
    }

    /// Returns a snapshot of the current in-memory audit records, primarily
    /// for tests and diagnostics.
    pub async fn audit_records(&self) -> Vec<crate::audit::AuditRecord> {
        self.state.lock().await.audit.records().to_vec()
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
                                workspace_root
                                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default()),
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
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
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
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
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
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
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
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
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
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
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
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
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
            CoreCommand::ImportPrd {
                client_id,
                request_id,
                command_hash,
                import_id,
                project_id,
                origin,
                version,
                source_text,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if let Some(replay) = journal
                        .record_deduplicated(&client_id, &request_id, &command_hash, b"")
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        return Ok(replay);
                    }
                    let parsed = crate::prd::parse_markdown_prd(&source_text, &origin, &version);
                    if !parsed.diagnostics.is_empty() {
                        let diagnostics = serde_json::to_string(&parsed.diagnostics)
                            .map_err(|error| error.to_string())?;
                        return Err(format!("PRD contains diagnostics: {diagnostics}"));
                    }
                    let document = parsed.document.ok_or_else(|| "PRD is empty".to_string())?;
                    let tasks = document
                        .tasks
                        .iter()
                        .enumerate()
                        .map(|(index, task)| ImportedTask {
                            id: format!("{project_id}:{import_id}:{index}"),
                            title: task.title.clone(),
                            description: task.description.clone(),
                            source_ref: task.source_ref.clone(),
                            acceptance_criteria: task.acceptance_criteria.join("\n"),
                        })
                        .collect::<Vec<_>>();
                    let imported = journal
                        .import_prd(
                            &import_id,
                            &project_id,
                            &origin,
                            &version,
                            &source_text,
                            &tasks,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    let result = serde_json::to_vec(&serde_json::json!({
                        "import_id": import_id,
                        "project_id": project_id,
                        "task_ids": imported.into_iter().map(|task| task.id).collect::<Vec<_>>(),
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
            CoreCommand::GetTaskHistory {
                task_id,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let events = journal
                        .task_history(&task_id, limit.min(100))
                        .await
                        .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "events": events.into_iter().map(|event| serde_json::json!({
                            "sequence_id": event.sequence_id,
                            "event_type": event.event_type,
                            "created_at": event.created_at,
                            "payload": serde_json::from_slice::<serde_json::Value>(&event.payload)
                                .unwrap_or_else(|_| serde_json::json!({"raw_bytes": event.payload})),
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskContext {
                project_id,
                task_id,
                max_chars,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let manifest = crate::workspace::build_manifest(
                        &project.workspace_path,
                        500,
                        2 * 1024 * 1024,
                    )
                    .map_err(|error| error.to_string())?;
                    let references = manifest
                        .entries
                        .iter()
                        .map(|entry| entry.relative_path.clone())
                        .collect::<Vec<_>>();
                    let context = crate::workspace::assemble_context(
                        crate::workspace::ContextInput {
                            title: &task.title,
                            description: &task.description,
                            acceptance_criteria: &task.acceptance_criteria,
                            non_goals: &task.non_goals,
                            references: &references,
                        },
                        max_chars.min(32 * 1024),
                    );
                    serde_json::to_vec(&serde_json::json!({
                        "project_id": project_id,
                        "task_id": task_id,
                        "workspace_hash": manifest.workspace_hash,
                        "manifest": manifest,
                        "context": context,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskPlanSpec {
                project_id,
                task_id,
                max_chars,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let plan = crate::plan::build_task_plan_spec(
                        &task.title,
                        &task.description,
                        &task.acceptance_criteria,
                        &task.non_goals,
                        "offline context; research не выполняется",
                        max_chars.min(32 * 1024),
                    );
                    serde_json::to_vec(&plan).map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetTaskSnapshot {
                project_id,
                task_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let snapshot = journal
                        .latest_snapshot_for_task(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "snapshot not found".to_string())?;
                    let snapshot_json =
                        serde_json::from_slice::<serde_json::Value>(&snapshot.payload)
                            .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({
                        "id": snapshot.id,
                        "run_id": snapshot.run_id,
                        "workspace_hash": snapshot.workspace_hash,
                        "created_at": snapshot.created_at,
                        "snapshot": snapshot_json,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RestoreTaskSnapshot {
                project_id,
                task_id,
                snapshot_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let task = journal
                        .get_work_item(&task_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "task not found".to_string())?;
                    if task.project_id != project_id {
                        return Err("task does not belong to project".to_string());
                    }
                    let snapshot = journal
                        .get_snapshot(&snapshot_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "snapshot not found".to_string())?;
                    let run = journal
                        .get_run(&snapshot.run_id)
                        .await
                        .map_err(|error| error.to_string())?;
                    if run.as_ref().map(|run| run.work_item_id.as_str()) != Some(task_id.as_str()) {
                        return Err("snapshot ownership could not be verified".to_string());
                    }
                    let run_id = snapshot.run_id.clone();
                    let workspace_snapshot = serde_json::from_slice::<
                        crate::build::WorkspaceSnapshot,
                    >(&snapshot.payload)
                    .map_err(|error| format!("invalid snapshot: {error}"))?;
                    crate::build::restore_snapshot(&project.workspace_path, &workspace_snapshot)
                        .map_err(|error| error.to_string())?;
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "snapshot_id": snapshot_id,
                        "run_id": run_id,
                        "operation": "workspace_restore",
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&task_id, "snapshot.rollback.applied", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        task_id.clone(),
                        "snapshot.rollback.applied",
                        [
                            ("snapshot_id".to_owned(), snapshot_id.clone()),
                            ("run_id".to_owned(), run_id.clone()),
                            ("operation".to_owned(), "workspace_restore".to_owned()),
                        ],
                    )
                    .await;
                    Ok(serde_json::to_vec(&serde_json::json!({
                        "snapshot_id": snapshot_id,
                        "restored": true,
                    }))
                    .map_err(|error| error.to_string())?)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::GetBuildPolicy { project_id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal.get_project(&project_id).await.map_err(|error| error.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    let (policy, version) = journal.get_build_policy(&project.id, &default_build_policy()).await?;
                    serde_json::to_vec(&serde_json::json!({ "project_id": project_id, "version": version, "policy": policy })).map_err(|error| error.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::SaveBuildPolicy {
                project_id,
                policy_json,
                expected_version,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal = journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    journal.get_project(&project_id).await.map_err(|error| error.to_string())?.ok_or_else(|| "project not found".to_string())?;
                    let policy = harden_build_policy(serde_json::from_slice::<crate::scope::BuildScope>(&policy_json).map_err(|error| format!("invalid build policy: {error}"))?);
                    if let Some(violation) = crate::scope::validate_build_scope(&policy, &[]).first() { return Err(format!("invalid build policy: {}", violation.reason)); }
                    let saved = journal.save_build_policy(&project_id, &policy, Some(expected_version)).await?;
                    serde_json::to_vec(&serde_json::json!({ "project_id": project_id, "version": saved.version, "policy": policy })).map_err(|error| error.to_string())
                }.await;
                let _ = reply.send(result);
            }
            CoreCommand::ApplyApprovedBuild {
                project_id,
                run_id,
                task_id,
                approved_build_json,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let approved =
                        serde_json::from_slice::<crate::build::ApprovedBuild>(&approved_build_json)
                            .map_err(|error| format!("invalid approved build: {error}"))?;
                    let _effect = journal
                        .begin_build_effect(&run_id, &task_id, &approved.intent_hash)
                        .await
                        .map_err(|error| error.to_string())?;
                    let snapshot = match crate::build::apply_approved_build(
                        &project.workspace_path,
                        &approved,
                    ) {
                        Ok(snapshot) => snapshot,
                        Err(error) => {
                            let _ = journal.complete_build_effect(&run_id, false, None).await;
                            Self::record_audit(
                                &state,
                                crate::audit::AuditKind::Failure,
                                if task_id.is_empty() {
                                    run_id.clone()
                                } else {
                                    task_id.clone()
                                },
                                "build.apply_failed",
                                [
                                    ("run_id".to_owned(), run_id.clone()),
                                    ("task_id".to_owned(), task_id.clone()),
                                    ("intent_hash".to_owned(), approved.intent_hash.clone()),
                                    ("error".to_owned(), error.to_string()),
                                ],
                            )
                            .await;
                            return Err(error.to_string());
                        }
                    };
                    let payload =
                        serde_json::to_vec(&snapshot).map_err(|error| error.to_string())?;
                    journal
                        .save_snapshot(
                            &snapshot.id,
                            &run_id,
                            &snapshot.baseline_workspace_hash,
                            &payload,
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "run_id": run_id,
                        "snapshot_id": snapshot.id,
                        "intent_hash": approved.intent_hash,
                        "effective_permissions_hash": approved.effective_permissions_hash,
                        "workspace_hash": snapshot.baseline_workspace_hash,
                        "diff_count": snapshot.diff.len(),
                        "diff": &snapshot.diff,
                    }))
                    .map_err(|error| error.to_string())?;
                    let audit_subject = if task_id.is_empty() {
                        &run_id
                    } else {
                        &task_id
                    };
                    journal
                        .record_audit(audit_subject, "build.applied", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    journal
                        .complete_build_effect(&run_id, true, Some(&snapshot.id))
                        .await
                        .map_err(|error| error.to_string())?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Diff,
                        audit_subject.to_string(),
                        "build.applied",
                        [
                            ("run_id".to_owned(), run_id.clone()),
                            ("task_id".to_owned(), task_id.clone()),
                            ("snapshot_id".to_owned(), snapshot.id.clone()),
                            ("intent_hash".to_owned(), approved.intent_hash.clone()),
                            ("diff_count".to_owned(), snapshot.diff.len().to_string()),
                        ],
                    )
                    .await;
                    Ok(payload)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::PrepareBuild {
                project_id,
                proposal_json,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let project = journal
                        .get_project(&project_id)
                        .await
                        .map_err(|error| error.to_string())?
                        .ok_or_else(|| "project not found".to_string())?;
                    let proposal =
                        serde_json::from_slice::<crate::build::BuildProposal>(&proposal_json)
                            .map_err(|error| format!("invalid build proposal: {error}"))?;
                    let policy = journal
                        .get_or_create_build_policy(&project_id, &default_build_policy())
                        .await?;
                    let effective_scope =
                        crate::scope::restrict_to_policy(&policy, &proposal.scope).map_err(
                            |violations| {
                                serde_json::to_string(&violations)
                                    .unwrap_or_else(|_| "build policy violation".into())
                            },
                        )?;
                    let effective_proposal = crate::build::BuildProposal {
                        scope: effective_scope,
                        changes: proposal.changes,
                    };
                    let approved =
                        crate::build::prepare_build(&project.workspace_path, &effective_proposal)
                            .map_err(|error| error.to_string())?;
                    let payload =
                        serde_json::to_vec(&approved).map_err(|error| error.to_string())?;
                    let audit_subject = format!("proposal-{}", approved.intent_hash);
                    let audit_payload = serde_json::to_vec(&serde_json::json!({
                        "intent_hash": approved.intent_hash,
                        "effective_permissions_hash": approved.effective_permissions_hash,
                        "expected_workspace_hash": approved.expected_workspace_hash,
                        "change_count": approved.changes.len(),
                    }))
                    .map_err(|error| error.to_string())?;
                    journal
                        .record_audit(&audit_subject, "build.approval_prepared", &audit_payload)
                        .await
                        .map_err(|error| error.to_string())?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Budget,
                        project_id.clone(),
                        "build.approval_prepared",
                        [
                            ("intent_hash".to_owned(), approved.intent_hash.clone()),
                            (
                                "change_count".to_owned(),
                                approved.changes.len().to_string(),
                            ),
                            (
                                "max_files_changed".to_owned(),
                                policy.max_files_changed.to_string(),
                            ),
                            (
                                "max_bytes_changed".to_owned(),
                                policy.max_bytes_changed.to_string(),
                            ),
                        ],
                    )
                    .await;
                    Ok(payload)
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RunDoctor {
                project_id,
                protocol_major,
                expected_protocol_major,
                provider,
                approval_required,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let storage = match &journal {
                        Some(journal) => {
                            let (path, schema_version) = journal
                                .storage_snapshot()
                                .await
                                .map_err(|error| error.to_string())?;
                            let exists = path.exists();
                            let writable = exists
                                && std::fs::metadata(&path)
                                    .map(|meta| !meta.permissions().readonly())
                                    .unwrap_or(false);
                            crate::doctor::StorageProbe {
                                path_label: path.display().to_string(),
                                exists,
                                writable,
                                schema_version: Some(schema_version),
                                expected_schema_version: evohime_local_storage::SCHEMA_VERSION,
                            }
                        }
                        None => crate::doctor::StorageProbe {
                            path_label: "not-configured".into(),
                            exists: false,
                            writable: false,
                            schema_version: None,
                            expected_schema_version: evohime_local_storage::SCHEMA_VERSION,
                        },
                    };

                    let pipe = crate::doctor::PipeProbe {
                        pipe_label: "desktop-ipc".into(),
                        reachable: true,
                        protocol_major,
                        expected_protocol_major,
                    };

                    let recovery = match &journal {
                        Some(journal) => journal
                            .recovery_probe()
                            .await
                            .map_err(|error| error.to_string())?,
                        None => crate::doctor::RecoveryProbe {
                            state: "NOT_CONFIGURED".into(),
                            unknown_effects: 0,
                            lease_expired: false,
                            resumable_runs: 0,
                        },
                    };

                    let permissions = match (&journal, project_id.is_empty()) {
                        (Some(journal), false) => {
                            match journal
                                .get_project(&project_id)
                                .await
                                .map_err(|error| error.to_string())?
                            {
                                Some(project) => {
                                    let workspace = std::path::Path::new(&project.workspace_path);
                                    let workspace_readable = workspace.is_dir();
                                    let workspace_writable = workspace_readable
                                        && std::fs::metadata(workspace)
                                            .map(|meta| !meta.permissions().readonly())
                                            .unwrap_or(false);
                                    let protected_paths_intact = [".git", ".evohime"]
                                        .iter()
                                        .all(|segment| workspace.join(segment).exists());
                                    crate::doctor::PermissionsProbe {
                                        workspace_readable,
                                        workspace_writable,
                                        protected_paths_intact,
                                        approval_required,
                                    }
                                }
                                None => unresolved_permissions_probe(approval_required),
                            }
                        }
                        _ => unresolved_permissions_probe(approval_required),
                    };

                    let snapshot = crate::doctor::DoctorSnapshot {
                        storage,
                        pipe,
                        provider,
                        recovery,
                        permissions,
                    };
                    let report = crate::doctor::DoctorReport::from_snapshot(&snapshot)
                        .map_err(|error| format!("{error:?}"))?;
                    Ok(report.to_bounded_json().into_bytes())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SaveResearchEvidence {
                work_item_id,
                source_kind,
                source_ref,
                title,
                publisher,
                content_type,
                raw_excerpt,
                retrieved_at_ms,
                ttl_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if work_item_id.trim().is_empty() {
                        return Err("work_item_id must not be empty".to_string());
                    }
                    let source = crate::research::SourceMetadata::new(
                        source_ref,
                        title,
                        publisher,
                        content_type,
                        retrieved_at_ms,
                    )
                    .map_err(|error| error.to_string())?;
                    let captured_at_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let evidence = crate::research::ResearchEvidence::capture(
                        source,
                        raw_excerpt,
                        captured_at_ms,
                        ttl_ms,
                    )
                    .map_err(|error| error.to_string())?;
                    let id = uuid::Uuid::new_v4().to_string();
                    let record = evohime_local_storage::research_store::ResearchEvidenceRecord {
                        id: id.clone(),
                        source_kind: source_kind.clone(),
                        source_ref: evidence.source.url.clone(),
                        redacted_excerpt: evidence.excerpt.clone(),
                        source_hash: evidence.excerpt_sha256.clone(),
                        fetched_at: evidence.captured_at_ms.to_string(),
                        ttl_seconds: evidence.ttl_ms.div_ceil(1_000),
                        provenance_link: Some(work_item_id.clone()),
                    };
                    journal.save_research_evidence(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        work_item_id.clone(),
                        "research.evidence.saved",
                        [
                            ("evidence_id".to_owned(), id.clone()),
                            ("source_kind".to_owned(), source_kind),
                            ("source_hash".to_owned(), evidence.excerpt_sha256.clone()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({
                        "id": id,
                        "work_item_id": work_item_id,
                        "evidence": evidence,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListResearchEvidence {
                work_item_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_research_evidence(&work_item_id).await?;
                    serde_json::to_vec(&serde_json::json!({
                        "work_item_id": work_item_id,
                        "records": records,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RunResearchFetch {
                work_item_id,
                url,
                title,
                allowed_domains,
                max_bytes,
                max_latency_ms,
                max_cost_micros,
                ttl_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if work_item_id.trim().is_empty() {
                        return Err("work_item_id must not be empty".to_string());
                    }
                    let policy = crate::research_pipeline::ResearchPolicy {
                        network_allowed: true,
                        allowed_domains,
                        max_bytes,
                        max_latency_ms,
                        max_cost_micros,
                    };
                    let fetch_result = crate::research_fetch::run_research_fetch(
                        &work_item_id,
                        &url,
                        &title,
                        &policy,
                        ttl_ms,
                        false,
                    )
                    .await;
                    match fetch_result {
                        Ok(outcome) => {
                            let id = uuid::Uuid::new_v4().to_string();
                            let record =
                                evohime_local_storage::research_store::ResearchEvidenceRecord {
                                    id: id.clone(),
                                    source_kind: "url".to_string(),
                                    source_ref: outcome.evidence.source.url.clone(),
                                    redacted_excerpt: outcome.evidence.excerpt.clone(),
                                    source_hash: outcome.evidence.excerpt_sha256.clone(),
                                    fetched_at: outcome.evidence.captured_at_ms.to_string(),
                                    ttl_seconds: outcome.evidence.ttl_ms.div_ceil(1_000),
                                    provenance_link: Some(work_item_id.clone()),
                                };
                            journal.save_research_evidence(&record).await?;
                            Self::record_audit(
                                &state,
                                crate::audit::AuditKind::Evidence,
                                work_item_id.clone(),
                                "research.fetch.completed",
                                [
                                    ("evidence_id".to_owned(), id.clone()),
                                    ("url".to_owned(), outcome.citation.url.clone()),
                                    (
                                        "source_hash".to_owned(),
                                        outcome.citation.source_hash.clone(),
                                    ),
                                ],
                            )
                            .await;
                            serde_json::to_vec(&serde_json::json!({
                                "id": id,
                                "work_item_id": work_item_id,
                                "state": outcome.state,
                                "evidence": outcome.evidence,
                                "citation": outcome.citation,
                            }))
                            .map_err(|error| error.to_string())
                        }
                        Err(error) => {
                            Self::record_audit(
                                &state,
                                crate::audit::AuditKind::Failure,
                                work_item_id.clone(),
                                "research.fetch.failed",
                                [
                                    ("url".to_owned(), url.clone()),
                                    ("state".to_owned(), format!("{:?}", error.state)),
                                    ("error".to_owned(), error.message.clone()),
                                ],
                            )
                            .await;
                            Err(error.message)
                        }
                    }
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::CreateMemory {
                scope_kind,
                project_id,
                secondary_id,
                title,
                content,
                provenance_kind,
                provenance_id,
                provenance_locator,
                privacy,
                ttl_ms,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let domain_scope =
                        memory_domain_scope(&scope_kind, &project_id, &secondary_id)?;
                    let provenance = crate::memory_domain::ProvenanceRef::new(
                        provenance_kind,
                        provenance_id,
                        (!provenance_locator.trim().is_empty()).then_some(provenance_locator),
                    )
                    .map_err(|error| error.to_string())?;
                    let privacy_label = parse_memory_privacy(&privacy)?;
                    let created_at_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let id = uuid::Uuid::new_v4().to_string();
                    let record = crate::memory_domain::MemoryDomain::new()
                        .create(crate::memory_domain::CreateMemory {
                            id: id.clone(),
                            scope: domain_scope,
                            title,
                            content,
                            provenance,
                            privacy: privacy_label,
                            created_at_ms,
                            ttl_ms,
                        })
                        .map_err(|error| error.to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let store_privacy = memory_store_privacy(record.privacy)?;
                    let provenance_json = serde_json::to_string(&record.provenance)
                        .map_err(|error| error.to_string())?;
                    let store_record = evohime_local_storage::memory_store::MemoryRecord::new(
                        record.id.clone(),
                        store_scope,
                        encode_memory_scope_id(&project_id, &secondary_id),
                        record.title.clone(),
                        record.content.clone(),
                        provenance_json,
                        store_privacy,
                        record.created_at_ms.to_string(),
                        Some(record.expires_at_ms.to_string()),
                    )
                    .map_err(|error| error.to_string())?;
                    journal.save_memory(&store_record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        project_id.clone(),
                        "memory.created",
                        [
                            ("memory_id".to_owned(), record.id.clone()),
                            ("scope_kind".to_owned(), scope_kind),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "record": record }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListMemory {
                scope_kind,
                project_id,
                secondary_id,
                include_archived,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let scope_id = encode_memory_scope_id(&project_id, &secondary_id);
                    let records = journal
                        .list_memory(store_scope, &scope_id, include_archived, limit)
                        .await?;
                    let records = records
                        .iter()
                        .map(memory_record_to_json)
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({ "records": records }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SearchMemory {
                scope_kind,
                project_id,
                secondary_id,
                query,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let store_scope = memory_store_scope(&scope_kind)?;
                    let scope_id = encode_memory_scope_id(&project_id, &secondary_id);
                    let now_ms = SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64;
                    let records = journal
                        .search_memory(store_scope, &scope_id, &query, &now_ms.to_string(), limit)
                        .await?;
                    let records = records
                        .iter()
                        .map(memory_record_to_json)
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({ "records": records }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ArchiveMemory {
                id,
                approval_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    crate::memory_api::Approval::new(
                        approval_id.clone(),
                        crate::memory_api::MemoryOperation::Archive,
                    )
                    .map_err(|error| error.to_string())?;
                    let changed = journal.archive_memory(&id).await?;
                    if !changed {
                        return Err(
                            "memory record was not found or is already archived/forgotten"
                                .to_string(),
                        );
                    }
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "memory.archived",
                        [
                            ("memory_id".to_owned(), id.clone()),
                            ("approval_id".to_owned(), approval_id),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "id": id, "archived": true }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ForgetMemory {
                id,
                approval_id,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    crate::memory_api::Approval::new(
                        approval_id.clone(),
                        crate::memory_api::MemoryOperation::Forget,
                    )
                    .map_err(|error| error.to_string())?;
                    let changed = journal.forget_memory(&id).await?;
                    if !changed {
                        return Err("memory record was not found".to_string());
                    }
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "memory.forgotten",
                        [
                            ("memory_id".to_owned(), id.clone()),
                            ("approval_id".to_owned(), approval_id),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "id": id, "forgotten": true }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::InstallCapability {
                manifest_json,
                install_source,
                source_path,
                expected_content_hash,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    if install_source != "local_archive" && install_source != "https_archive" {
                        return Err(format!(
                            "unsupported capability install source: {install_source}"
                        ));
                    }
                    let candidate: crate::capability_registry::CapabilityManifest =
                        serde_json::from_str(&manifest_json).map_err(|error| error.to_string())?;
                    candidate.validate().map_err(|error| error.to_string())?;
                    let expected_manifest_source = if install_source == "https_archive" {
                        crate::capability_registry::InstallSource::HttpsArchive
                    } else {
                        crate::capability_registry::InstallSource::LocalArchive
                    };
                    if candidate.install.source != expected_manifest_source {
                        return Err(
                            "manifest install source does not match the requested installer"
                                .to_string(),
                        );
                    }
                    if install_source == "https_archive" {
                        verify_https_capability_archive(&source_path, &expected_content_hash)
                            .await?;
                    }
                    let existing_records = journal
                        .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                        .await?;
                    let mut existing_manifests = Vec::with_capacity(existing_records.len());
                    for record in &existing_records {
                        let manifest: crate::capability_registry::CapabilityManifest =
                            serde_json::from_str(&record.manifest_json)
                                .map_err(|error| error.to_string())?;
                        existing_manifests.push(manifest);
                    }
                    if let Some(current) = existing_manifests
                        .iter()
                        .find(|manifest| manifest.name == candidate.name)
                    {
                        crate::capability_registry::validate_update(current, &candidate)
                            .map_err(|error| error.to_string())?;
                    } else {
                        let mut proposed = existing_manifests.clone();
                        proposed.push(candidate.clone());
                        crate::capability_registry::validate_registry(&proposed)
                            .map_err(|error| error.to_string())?;
                    }
                    let store_record =
                        evohime_local_storage::capability_store::CapabilityManifestRecord {
                            id: candidate.name.clone(),
                            kind: capability_manifest_kind(&candidate),
                            version: candidate.version.clone(),
                            risk_class: capability_risk_class_str(candidate.risk_class).to_string(),
                            content_hash: candidate.content_hash.clone(),
                            manifest_json: serde_json::to_string(&candidate)
                                .map_err(|error| error.to_string())?,
                        };
                    journal.save_capability_manifest(&store_record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        candidate.name.clone(),
                        "capability.installed",
                        [
                            ("manifest_id".to_owned(), candidate.name.clone()),
                            ("version".to_owned(), candidate.version.clone()),
                            ("install_source".to_owned(), install_source),
                            ("source_path".to_owned(), source_path),
                            (
                                "expected_content_hash".to_owned(),
                                if expected_content_hash.is_empty() {
                                    "not_provided".to_owned()
                                } else {
                                    expected_content_hash
                                },
                            ),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "manifest": candidate }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListCapabilities { limit, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_capability_manifests(limit).await?;
                    let manifests = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                                &record.manifest_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({ "manifests": manifests }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::MatchCapabilities {
                intent,
                required_tools,
                required_domains,
                requested_risk,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let requested_risk = parse_capability_risk_class(&requested_risk)?;
                    let records = journal
                        .list_capability_manifests(crate::capability_registry::MAX_MANIFESTS as u32)
                        .await?;
                    let manifests = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::capability_registry::CapabilityManifest>(
                                &record.manifest_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let query = crate::capability_registry::MatchQuery {
                        intent,
                        required_tools,
                        required_domains,
                        requested_risk,
                    };
                    let matches =
                        crate::capability_registry::match_capabilities(&manifests, &query)
                            .map_err(|error| error.to_string())?;
                    serde_json::to_vec(&serde_json::json!({ "matches": matches }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RemoveCapability { id, reply } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let removed = journal.remove_capability_manifest(&id).await?;
                    if !removed {
                        return Err("capability manifest was not found".to_string());
                    }
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Approval,
                        id.clone(),
                        "capability.removed",
                        [("manifest_id".to_owned(), id.clone())],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "id": id, "removed": true }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::RequestChildHandoff {
                handoff_id,
                task_id,
                kind,
                from_role,
                from_name,
                to_role,
                to_name,
                purpose,
                payload,
                sequence,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let parsed_kind = handoff_kind_from_str(&kind)?;
                    let from = role_identity_from_parts(&from_role, &from_name)?;
                    let to = role_identity_from_parts(&to_role, &to_name)?;
                    let handoff_payload = crate::child_roles::HandoffPayload::new(payload)
                        .map_err(|error| error.to_string())?;
                    let envelope = crate::child_roles::HandoffEnvelope::new(
                        handoff_id.clone(),
                        task_id.clone(),
                        parsed_kind,
                        from.clone(),
                        to.clone(),
                        purpose,
                        handoff_payload,
                        sequence,
                    )
                    .map_err(|error| error.to_string())?;
                    let record = evohime_local_storage::child_store::HandoffRecord {
                        handoff_id: envelope.handoff_id.clone(),
                        task_id: envelope.task_id.clone(),
                        kind: handoff_kind_str(envelope.kind).to_string(),
                        status: handoff_status_str(envelope.status).to_string(),
                        from_role: role_identity_display(&from),
                        to_role: role_identity_display(&to),
                        sequence: envelope.sequence,
                        envelope_json: envelope.to_deterministic_json(),
                    };
                    journal.save_child_handoff(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        task_id.clone(),
                        "child.handoff.requested",
                        [
                            ("handoff_id".to_owned(), envelope.handoff_id.clone()),
                            ("task_id".to_owned(), task_id),
                            ("from_role".to_owned(), record.from_role.clone()),
                            ("to_role".to_owned(), record.to_role.clone()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "handoff": envelope }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::ListChildHandoffs {
                task_id,
                limit,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let records = journal.list_child_handoffs(&task_id, limit).await?;
                    let handoffs = records
                        .iter()
                        .map(|record| {
                            serde_json::from_str::<crate::child_roles::HandoffEnvelope>(
                                &record.envelope_json,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    serde_json::to_vec(&serde_json::json!({
                        "task_id": task_id,
                        "handoffs": handoffs,
                    }))
                    .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SubmitChildRequest {
                child_task_id,
                parent_task_id,
                role,
                kind,
                reduced_context,
                max_output_bytes,
                requested_capabilities,
                parent_is_child,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let parsed_kind = child_task_kind_from_str(&kind)?;
                    let request = crate::child_runtime::ChildTaskRequest {
                        child_task_id: child_task_id.clone(),
                        parent_task_id: parent_task_id.clone(),
                        role: role.clone(),
                        kind: parsed_kind,
                        reduced_context,
                        max_output_bytes: max_output_bytes as usize,
                        requested_capabilities,
                        parent_is_child,
                    };
                    // The real bounded contract runs here: rejects nested
                    // children, any non-read-only requested capability, and
                    // oversized context/output. This is the same
                    // `ChildTaskRequest::validate` used by the pure unit
                    // tests, now enforced on the live IPC path.
                    request.validate().map_err(|error| error.to_string())?;
                    let request_json =
                        serde_json::to_string(&request).map_err(|error| error.to_string())?;
                    let record = evohime_local_storage::child_store::ChildTaskRequestRecord {
                        child_task_id: request.child_task_id.clone(),
                        parent_task_id: request.parent_task_id.clone(),
                        role: request.role.clone(),
                        kind: child_task_kind_str(request.kind).to_string(),
                        request_json,
                    };
                    journal.save_child_task_request(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        parent_task_id.clone(),
                        "child.request.submitted",
                        [
                            ("child_task_id".to_owned(), request.child_task_id.clone()),
                            ("parent_task_id".to_owned(), parent_task_id),
                            ("role".to_owned(), request.role.clone()),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "request": request }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
            CoreCommand::SubmitChildReport {
                child_task_id,
                status,
                summary,
                findings,
                sources,
                confidence_percent,
                reply,
            } => {
                let journal = state.lock().await.journal.clone();
                let result = async {
                    let journal =
                        journal.ok_or_else(|| "storage journal is not configured".to_string())?;
                    let parsed_status = child_report_status_from_str(&status)?;
                    let confidence_percent: u8 = u8::try_from(confidence_percent)
                        .map_err(|_| "confidence_percent must be between 0 and 255".to_string())?;
                    let report = crate::child_runtime::ChildReport {
                        child_task_id: child_task_id.clone(),
                        status: parsed_status,
                        summary,
                        findings,
                        sources,
                        confidence_percent,
                    };
                    let stored_request = journal
                        .get_child_task_request(&child_task_id)
                        .await?
                        .ok_or_else(|| {
                            "no matching child task request found for child_task_id".to_string()
                        })?;
                    let request: crate::child_runtime::ChildTaskRequest =
                        serde_json::from_str(&stored_request.request_json)
                            .map_err(|error| error.to_string())?;
                    // The real bounded contract runs here: re-validates the
                    // request, validates the report's own bounds, rejects
                    // secret-like content and duplicate sources, and
                    // rejects a child_task_id mismatch -- the same
                    // `accept_report` used by the pure unit tests, now
                    // enforced on the live IPC path.
                    let accepted = crate::child_runtime::accept_report(&request, &report)
                        .map_err(|error| error.to_string())?;
                    let report_json =
                        serde_json::to_string(&accepted).map_err(|error| error.to_string())?;
                    let record = evohime_local_storage::child_store::ChildReportRecord {
                        child_task_id: accepted.child_task_id.clone(),
                        parent_task_id: stored_request.parent_task_id.clone(),
                        status: child_report_status_str(accepted.status).to_string(),
                        confidence_percent: accepted.confidence_percent,
                        report_json,
                    };
                    journal.save_child_report(&record).await?;
                    Self::record_audit(
                        &state,
                        crate::audit::AuditKind::Evidence,
                        stored_request.parent_task_id.clone(),
                        "child.report.accepted",
                        [
                            ("child_task_id".to_owned(), accepted.child_task_id.clone()),
                            (
                                "parent_task_id".to_owned(),
                                stored_request.parent_task_id.clone(),
                            ),
                            (
                                "confidence_percent".to_owned(),
                                accepted.confidence_percent.to_string(),
                            ),
                        ],
                    )
                    .await;
                    serde_json::to_vec(&serde_json::json!({ "report": accepted }))
                        .map_err(|error| error.to_string())
                }
                .await;
                let _ = reply.send(result);
            }
        }
    }
}

/// Maps an IPC-layer scope kind + project/secondary id pair into the
/// `memory_domain::MemoryScope` used for validation and redaction.
fn memory_domain_scope(
    kind: &str,
    project_id: &str,
    secondary_id: &str,
) -> Result<crate::memory_domain::MemoryScope, String> {
    match kind {
        "project" => crate::memory_domain::MemoryScope::project(project_id)
            .map_err(|error| error.to_string()),
        "task" => crate::memory_domain::MemoryScope::task(project_id, secondary_id)
            .map_err(|error| error.to_string()),
        "workspace" => crate::memory_domain::MemoryScope::workspace(project_id, secondary_id)
            .map_err(|error| error.to_string()),
        other => Err(format!("unsupported memory scope kind: {other}")),
    }
}

/// Maps an IPC-layer scope kind into the `memory_store::MemoryScope` used by
/// the real `memory_entries` table.
fn memory_store_scope(
    kind: &str,
) -> Result<evohime_local_storage::memory_store::MemoryScope, String> {
    match kind {
        "project" => Ok(evohime_local_storage::memory_store::MemoryScope::Project),
        "task" => Ok(evohime_local_storage::memory_store::MemoryScope::Task),
        "workspace" => Ok(evohime_local_storage::memory_store::MemoryScope::Workspace),
        other => Err(format!("unsupported memory scope kind: {other}")),
    }
}

fn parse_memory_privacy(value: &str) -> Result<crate::memory_domain::PrivacyLabel, String> {
    match value {
        "public" => Ok(crate::memory_domain::PrivacyLabel::Public),
        "internal" | "" => Ok(crate::memory_domain::PrivacyLabel::Internal),
        "private" => Ok(crate::memory_domain::PrivacyLabel::Private),
        other => Err(format!(
            "unsupported memory privacy label: {other} (secret is not supported by persistent storage)"
        )),
    }
}

/// The persistent `memory_entries` table has no `secret` privacy label; the
/// domain-level `PrivacyLabel::Secret` is rejected before it ever reaches
/// storage (callers must not be able to persist a value they cannot express).
fn memory_store_privacy(
    label: crate::memory_domain::PrivacyLabel,
) -> Result<evohime_local_storage::memory_store::MemoryPrivacy, String> {
    match label {
        crate::memory_domain::PrivacyLabel::Public => {
            Ok(evohime_local_storage::memory_store::MemoryPrivacy::Public)
        }
        crate::memory_domain::PrivacyLabel::Internal => {
            Ok(evohime_local_storage::memory_store::MemoryPrivacy::Internal)
        }
        crate::memory_domain::PrivacyLabel::Private => {
            Ok(evohime_local_storage::memory_store::MemoryPrivacy::Private)
        }
        crate::memory_domain::PrivacyLabel::Secret => {
            Err("secret privacy is not supported by persistent memory storage".to_string())
        }
    }
}

/// Encodes a project/secondary id pair into the single `scope_id` column the
/// `memory_entries` table stores. Project scope uses the project id alone;
/// task/workspace scope appends the secondary id after a `:` separator so
/// list/search can still target one exact scope.
fn encode_memory_scope_id(project_id: &str, secondary_id: &str) -> String {
    if secondary_id.trim().is_empty() {
        project_id.to_string()
    } else {
        format!("{project_id}:{secondary_id}")
    }
}

fn decode_memory_scope_id(scope_id: &str) -> (String, String) {
    match scope_id.split_once(':') {
        Some((project_id, secondary_id)) => (project_id.to_string(), secondary_id.to_string()),
        None => (scope_id.to_string(), String::new()),
    }
}

/// Renders a stored `memory_store::MemoryRecord` back into the JSON shape
/// returned over IPC, decoding the scope id and parsing the provenance JSON
/// that was serialized at create time.
fn memory_record_to_json(
    record: &evohime_local_storage::memory_store::MemoryRecord,
) -> Result<serde_json::Value, String> {
    let (project_id, secondary_id) = decode_memory_scope_id(&record.scope_id);
    let provenance: serde_json::Value = if record.provenance.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&record.provenance).unwrap_or(serde_json::Value::Null)
    };
    let scope_kind = match record.scope {
        evohime_local_storage::memory_store::MemoryScope::Project => "project",
        evohime_local_storage::memory_store::MemoryScope::Task => "task",
        evohime_local_storage::memory_store::MemoryScope::Workspace => "workspace",
    };
    let privacy = match record.privacy {
        evohime_local_storage::memory_store::MemoryPrivacy::Public => "public",
        evohime_local_storage::memory_store::MemoryPrivacy::Internal => "internal",
        evohime_local_storage::memory_store::MemoryPrivacy::Private => "private",
    };
    Ok(serde_json::json!({
        "id": record.id,
        "scope_kind": scope_kind,
        "project_id": project_id,
        "secondary_id": secondary_id,
        "title": record.title,
        "content": record.content,
        "provenance": provenance,
        "privacy": privacy,
        "created_at_ms": record.created_at,
        "expires_at_ms": record.expires_at,
        "archived": record.archived,
        "forgotten": record.forgotten,
    }))
}

/// Cheap listing classification derived from which of a manifest's
/// `roles`/`skills` lists are non-empty; see
/// `capability_store::ManifestKind` for why this is store-layer only.
fn capability_manifest_kind(
    manifest: &crate::capability_registry::CapabilityManifest,
) -> evohime_local_storage::capability_store::ManifestKind {
    match (!manifest.roles.is_empty(), !manifest.skills.is_empty()) {
        (true, false) => evohime_local_storage::capability_store::ManifestKind::Role,
        (false, true) => evohime_local_storage::capability_store::ManifestKind::Skill,
        _ => evohime_local_storage::capability_store::ManifestKind::Mixed,
    }
}

const MAX_CAPABILITY_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const CAPABILITY_ARCHIVE_TIMEOUT_MS: u64 = 30_000;

/// Downloads one capability archive into bounded memory solely for integrity
/// verification. The archive is deliberately not persisted by this command;
/// the catalog write below records only the already-validated manifest.
async fn verify_https_capability_archive(
    source_url: &str,
    expected_content_hash: &str,
) -> Result<(), String> {
    if expected_content_hash.len() != 64
        || !expected_content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("expected_content_hash must be a 64-character SHA-256 digest".to_string());
    }
    let url = reqwest::Url::parse(source_url).map_err(|error| error.to_string())?;
    if url.scheme() != "https" {
        return Err("https_archive source_path must use HTTPS".to_string());
    }
    evohime_tool_runtime::assert_safe_http_url(&url)
        .map_err(|message| format!("ssrf blocked capability archive: {message}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(
            CAPABILITY_ARCHIVE_TIMEOUT_MS,
        ))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.url().scheme() == "https"
                && evohime_tool_runtime::assert_safe_http_url(attempt.url()).is_ok()
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .user_agent("EvoHime/0.1 capability-installer")
        .build()
        .map_err(|error| format!("capability archive client setup failed: {error}"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("capability archive download failed: {error}"))?;
    if response.url().scheme() != "https" {
        return Err("capability archive redirect left HTTPS".to_string());
    }
    evohime_tool_runtime::assert_safe_http_url(response.url())
        .map_err(|message| format!("ssrf blocked capability archive redirect: {message}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "capability archive endpoint returned {}",
            response.status()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CAPABILITY_ARCHIVE_BYTES)
    {
        return Err(format!(
            "capability archive exceeds {MAX_CAPABILITY_ARCHIVE_BYTES} byte limit"
        ));
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| format!("failed to read capability archive: {error}"))?;
        body.extend_from_slice(&chunk);
        if body.len() as u64 > MAX_CAPABILITY_ARCHIVE_BYTES {
            return Err(format!(
                "capability archive exceeds {MAX_CAPABILITY_ARCHIVE_BYTES} byte limit"
            ));
        }
    }
    verify_capability_archive_hash(&body, expected_content_hash)
}

fn verify_capability_archive_hash(bytes: &[u8], expected_content_hash: &str) -> Result<(), String> {
    let observed = crate::research::sha256_hex(bytes);
    if !observed.eq_ignore_ascii_case(expected_content_hash) {
        return Err(format!(
            "capability archive SHA-256 mismatch: expected {expected_content_hash}, observed {observed}"
        ));
    }
    Ok(())
}

fn capability_risk_class_str(risk: crate::capability_registry::RiskClass) -> &'static str {
    match risk {
        crate::capability_registry::RiskClass::Low => "low",
        crate::capability_registry::RiskClass::Medium => "medium",
        crate::capability_registry::RiskClass::High => "high",
    }
}

fn parse_capability_risk_class(
    value: &str,
) -> Result<crate::capability_registry::RiskClass, String> {
    match value {
        "low" => Ok(crate::capability_registry::RiskClass::Low),
        "medium" | "" => Ok(crate::capability_registry::RiskClass::Medium),
        "high" => Ok(crate::capability_registry::RiskClass::High),
        other => Err(format!("unsupported requested_risk: {other}")),
    }
}

fn handoff_kind_from_str(value: &str) -> Result<crate::child_roles::HandoffKind, String> {
    match value {
        "delegate" => Ok(crate::child_roles::HandoffKind::Delegate),
        "return_result" => Ok(crate::child_roles::HandoffKind::ReturnResult),
        "request_review" => Ok(crate::child_roles::HandoffKind::RequestReview),
        "request_retry" => Ok(crate::child_roles::HandoffKind::RequestRetry),
        other => Err(format!("unsupported handoff kind: {other}")),
    }
}

fn handoff_kind_str(kind: crate::child_roles::HandoffKind) -> &'static str {
    match kind {
        crate::child_roles::HandoffKind::Delegate => "delegate",
        crate::child_roles::HandoffKind::ReturnResult => "return_result",
        crate::child_roles::HandoffKind::RequestReview => "request_review",
        crate::child_roles::HandoffKind::RequestRetry => "request_retry",
    }
}

fn handoff_status_str(status: crate::child_roles::HandoffStatus) -> &'static str {
    match status {
        crate::child_roles::HandoffStatus::Pending => "pending",
        crate::child_roles::HandoffStatus::Accepted => "accepted",
        crate::child_roles::HandoffStatus::Rejected => "rejected",
        crate::child_roles::HandoffStatus::Completed => "completed",
    }
}

fn child_role_from_str(value: &str) -> Result<crate::child_roles::ChildRole, String> {
    match value {
        "coordinator" => Ok(crate::child_roles::ChildRole::Coordinator),
        "researcher" => Ok(crate::child_roles::ChildRole::Researcher),
        "planner" => Ok(crate::child_roles::ChildRole::Planner),
        "implementer" => Ok(crate::child_roles::ChildRole::Implementer),
        "reviewer" => Ok(crate::child_roles::ChildRole::Reviewer),
        "tester" => Ok(crate::child_roles::ChildRole::Tester),
        "custom" => Ok(crate::child_roles::ChildRole::Custom),
        other => Err(format!("unsupported child role: {other}")),
    }
}

/// Builds a `RoleIdentity` from the wire's separate role/name fields. A
/// "custom" role requires a bounded, validated name; a built-in role
/// carries no name.
fn role_identity_from_parts(
    role: &str,
    name: &str,
) -> Result<crate::child_roles::RoleIdentity, String> {
    let parsed_role = child_role_from_str(role)?;
    if parsed_role == crate::child_roles::ChildRole::Custom {
        crate::child_roles::RoleIdentity::custom(name).map_err(|error| error.to_string())
    } else {
        Ok(crate::child_roles::RoleIdentity::builtin(parsed_role))
    }
}

/// Cheap display form of a `RoleIdentity` for the store's denormalized
/// listing columns only; the full identity survives in the envelope JSON.
fn role_identity_display(identity: &crate::child_roles::RoleIdentity) -> String {
    match &identity.name {
        Some(name) => format!("custom:{name}"),
        None => format!("{:?}", identity.role).to_ascii_lowercase(),
    }
}

fn child_task_kind_from_str(value: &str) -> Result<crate::child_runtime::ChildTaskKind, String> {
    match value {
        "code_search" => Ok(crate::child_runtime::ChildTaskKind::CodeSearch),
        "threat_model_review" => Ok(crate::child_runtime::ChildTaskKind::ThreatModelReview),
        "test_plan_review" => Ok(crate::child_runtime::ChildTaskKind::TestPlanReview),
        "documentation" => Ok(crate::child_runtime::ChildTaskKind::Documentation),
        "onboarding" => Ok(crate::child_runtime::ChildTaskKind::Onboarding),
        other => Err(format!("unsupported child task kind: {other}")),
    }
}

fn child_task_kind_str(kind: crate::child_runtime::ChildTaskKind) -> &'static str {
    match kind {
        crate::child_runtime::ChildTaskKind::CodeSearch => "code_search",
        crate::child_runtime::ChildTaskKind::ThreatModelReview => "threat_model_review",
        crate::child_runtime::ChildTaskKind::TestPlanReview => "test_plan_review",
        crate::child_runtime::ChildTaskKind::Documentation => "documentation",
        crate::child_runtime::ChildTaskKind::Onboarding => "onboarding",
    }
}

fn child_report_status_from_str(
    value: &str,
) -> Result<crate::child_runtime::ChildReportStatus, String> {
    match value {
        "complete" => Ok(crate::child_runtime::ChildReportStatus::Complete),
        "partial" => Ok(crate::child_runtime::ChildReportStatus::Partial),
        "rejected" => Ok(crate::child_runtime::ChildReportStatus::Rejected),
        other => Err(format!("unsupported child report status: {other}")),
    }
}

fn child_report_status_str(status: crate::child_runtime::ChildReportStatus) -> &'static str {
    match status {
        crate::child_runtime::ChildReportStatus::Complete => "complete",
        crate::child_runtime::ChildReportStatus::Partial => "partial",
        crate::child_runtime::ChildReportStatus::Rejected => "rejected",
    }
}

/// Fail-closed permissions probe used when the doctor cannot ground its
/// permissions check in a real, resolved workspace (no project supplied or
/// the project was not found). This intentionally does not claim health.
fn unresolved_permissions_probe(approval_required: bool) -> crate::doctor::PermissionsProbe {
    crate::doctor::PermissionsProbe {
        workspace_readable: false,
        workspace_writable: false,
        protected_paths_intact: false,
        approval_required,
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
    fn denied_approval_output_is_not_a_successful_mutation() {
        assert!(super::tool_output_failed(
            "approval denied: mutation not performed"
        ));
    }

    #[test]
    fn agent_identity_includes_short_name() {
        assert!(super::AGENT_IDENTITY_PROMPT.contains("Ева"));
        assert!(super::AGENT_IDENTITY_PROMPT.contains("EvoHime"));
    }

    #[test]
    fn capability_archive_hash_mismatch_is_rejected_before_install() {
        let error = super::verify_capability_archive_hash(b"trusted archive", &"0".repeat(64))
            .expect_err("tampered archive must be rejected");
        assert!(error.contains("SHA-256 mismatch"));
    }

    #[test]
    fn agent_system_prompt_explains_workspace_research_flow() {
        let prompt = super::build_agent_system_prompt(
            std::path::Path::new(r"C:\Projects\demo"),
            &["filesystem.list".into(), "filesystem.read".into()],
        );
        assert!(prompt.contains(r"C:\Projects\demo"));
        assert!(prompt.contains("filesystem.list"));
        assert!(prompt.contains("не сформулировал конкретное поручение"));
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
    fn parses_json_function_call_blocks_for_mutating_tools() {
        let content = r#"
<function_calls>
[{"tool_name":"filesystem.patch","arguments":{"path":"tests/a.rs","patch":"--- a/tests/a.rs\n+++ b/tests/a.rs\n@@"}}]
</function_calls>
"#;
        let calls = super::parse_legacy_function_calls(content, 4);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "filesystem.patch");
        assert!(calls[0].arguments.contains("tests/a.rs"));
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
        assert!(
            super::parse_natural_tool_intent("Инструмент filesystem.list доступен.", 3).is_none()
        );
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

    struct ToolCallingExecutor;

    impl TaskExecutor for ToolCallingExecutor {
        fn execute(
            &self,
            task_id: String,
            _prompt: String,
            _cancellation: CancellationToken,
            events: tokio::sync::broadcast::Sender<CoreEvent>,
        ) -> BoxFuture<'static, Result<String, AgentRunError>> {
            Box::pin(async move {
                let _ = events.send(CoreEvent::ToolStarted {
                    task_id: task_id.clone(),
                    tool_name: "filesystem.list".into(),
                });
                Ok("done".into())
            })
        }
    }

    #[tokio::test]
    async fn tool_started_event_appends_a_real_audit_record() {
        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(ToolCallingExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-audit-tool".into(),
                prompt: "list files".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::ToolStarted { .. })
        ));

        let mut records = Vec::new();
        for _ in 0..50 {
            records = coordinator.audit_records().await;
            if records
                .iter()
                .any(|record| record.kind == super::audit::AuditKind::ToolCall)
            {
                break;
            }
            tokio::task::yield_now().await;
        }

        let tool_call = records
            .iter()
            .find(|record| record.kind == super::audit::AuditKind::ToolCall)
            .expect("tool call audit record is appended");
        assert_eq!(tool_call.actor, "task-audit-tool");
        assert_eq!(tool_call.event_id, "tool.started");
        assert_eq!(
            tool_call.fields.get("tool_name").map(String::as_str),
            Some("filesystem.list")
        );

        let jsonl = coordinator.audit_jsonl().await;
        assert!(jsonl.contains("\"kind\":\"tool_call\""));
        assert!(jsonl.contains("filesystem.list"));
    }

    #[tokio::test]
    async fn task_failed_event_appends_a_failure_audit_record() {
        struct FailingExecutor;
        impl TaskExecutor for FailingExecutor {
            fn execute(
                &self,
                _task_id: String,
                _prompt: String,
                _cancellation: CancellationToken,
                _events: tokio::sync::broadcast::Sender<CoreEvent>,
            ) -> BoxFuture<'static, Result<String, AgentRunError>> {
                Box::pin(async move { Err(AgentRunError::Timeout(1)) })
            }
        }

        let (coordinator, mut events) =
            TaskCoordinator::new_with_executor(8, Some(Arc::new(FailingExecutor)));
        coordinator
            .dispatch(CoreCommand::StartTask {
                task_id: "task-audit-failure".into(),
                prompt: "fail please".into(),
                workspace_root: None,
            })
            .await
            .expect("start dispatches");
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskStarted { .. })
        ));
        assert!(matches!(
            events.recv().await,
            Ok(CoreEvent::TaskFailed { .. })
        ));

        let mut records = Vec::new();
        for _ in 0..50 {
            records = coordinator.audit_records().await;
            if records
                .iter()
                .any(|record| record.kind == super::audit::AuditKind::Failure)
            {
                break;
            }
            tokio::task::yield_now().await;
        }

        let failure = records
            .iter()
            .find(|record| record.kind == super::audit::AuditKind::Failure)
            .expect("failure audit record is appended");
        assert_eq!(failure.actor, "task-audit-failure");
        assert_eq!(failure.event_id, "task.failed");
        assert!(failure.fields.get("error").is_some());
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
        assert!(!requirements.diff_check);
        assert_eq!(
            requirements.missing(false, false, true, false),
            vec!["внести изменение", "создать commit"]
        );
    }

    #[test]
    fn detects_diff_check_as_a_commit_prerequisite() {
        let requirements = super::DeliveryRequirements::from_prompt(
            "добавь тест, выполни cargo test, git diff --check и создай commit",
        );
        assert!(requirements.verification);
        assert!(requirements.diff_check);
        assert!(requirements.commit);
    }

    #[test]
    fn detects_research_requirement_and_keeps_it_open_until_observed() {
        let requirements = super::DeliveryRequirements::from_prompt("изучи проект");
        assert!(requirements.research);
        assert_eq!(
            requirements.missing(false, false, false, false),
            vec!["изучить workspace и подготовить отчёт"]
        );
        assert!(super::DeliveryRequirements::from_prompt("привет").research == false);
    }

    #[test]
    fn delivery_gate_finishes_research_before_mutation() {
        let requirements = super::DeliveryRequirements {
            research: true,
            mutation: true,
            verification: true,
            diff_check: true,
            commit: true,
        };
        assert!(super::delivery_next_step(requirements, false, false, false, false)
            .contains("read-only"));
        assert!(super::delivery_next_step(requirements, true, false, false, false)
            .contains("filesystem.patch"));
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
        journal
            .record_audit(
                "run-journal",
                "build.applied",
                br#"{"snapshot_id":"snap-1"}"#,
            )
            .await
            .expect("audit records");
        let audit = journal
            .task_history("run-journal", 10)
            .await
            .expect("audit reads");
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].event_type, "build.applied");
        assert_eq!(audit[0].payload, br#"{"snapshot_id":"snap-1"}"#);
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
