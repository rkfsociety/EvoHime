use evohime_tool_runtime::{ToolError, ToolResult};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialSource {
    Policy,
    User,
    Escalation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFailureKind {
    NotFound,
    InvalidInput,
    Denied(DenialSource),
    Timeout,
    NonZeroExit,
    Execution,
}

#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub ok: bool,
    pub kind: Option<ToolFailureKind>,
    pub output: String,
    pub structured: Value,
}

pub fn recovery_hint(
    tool_name: &str,
    kind: ToolFailureKind,
    structured: &Value,
    schema: &Value,
    description: &str,
) -> String {
    match kind {
        ToolFailureKind::NotFound => format!(
            "Путь не найден. Используй filesystem.search по фрагменту имени `{tool_name}`; пути должны быть workspace-relative."
        ),
        ToolFailureKind::InvalidInput => {
            if schema.get("additionalProperties") == Some(&Value::Bool(true)) {
                format!("Проверь обязательные поля инструмента {tool_name}: {description}")
            } else {
                format!("Повтори вызов {tool_name} с JSON по схеме: {schema}")
            }
        }
        ToolFailureKind::Denied(DenialSource::Policy) => {
            "Вызов запрещён политикой; не повторяй тот же вызов, выбери разрешённый путь или сообщи хозяину о требуемом permission.".into()
        }
        ToolFailureKind::Denied(DenialSource::User) => {
            "Хозяин отклонил этот вызов; не повторяй его без изменения способа или явного нового разрешения.".into()
        }
        ToolFailureKind::Denied(DenialSource::Escalation) => {
            "Инструмент временно заблокирован после повторных ошибок на 2 шага; используй другой способ, затем вернись к нему.".into()
        }
        ToolFailureKind::Timeout => {
            "Вызов превысил таймаут; сузь объём: укажи конкретный путь вместо `.`, используй меньшую цель вроде `--lib`.".into()
        }
        ToolFailureKind::NonZeroExit => {
            let stderr = structured
                .get("stderr")
                .and_then(Value::as_str)
                .unwrap_or("<stderr отсутствует>");
            format!("Команда завершилась с ошибкой. Первые строки stderr: {}", stderr.lines().take(5).collect::<Vec<_>>().join("\n"))
        }
        ToolFailureKind::Execution => {
            "Инструмент завершился с ошибкой. Исправь аргументы или выбери другой подтверждённый способ и повтори один раз.".into()
        }
    }
}

pub fn canonical_call_signature(name: &str, arguments: &str) -> String {
    let value = serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| Value::String(arguments.into()));
    format!("{name}:{}", canonical_json(&value))
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let sorted = object.iter().map(|(key, value)| (key, canonical_json(value))).collect::<BTreeMap<_, _>>();
            serde_json::to_string(&sorted).unwrap_or_default()
        }
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

pub struct RecentToolCalls {
    capacity: usize,
    order: VecDeque<String>,
    present: HashMap<String, ()>,
}

impl RecentToolCalls {
    pub fn new(capacity: usize) -> Self {
        Self { capacity, order: VecDeque::new(), present: HashMap::new() }
    }

    pub fn remember(&mut self, signature: String) -> bool {
        if self.present.contains_key(&signature) { return false; }
        self.present.insert(signature.clone(), ());
        self.order.push_back(signature);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() { self.present.remove(&old); }
        }
        true
    }

    pub fn forget_reads(&mut self) {
        self.order.retain(|signature| {
            let keep = !(signature.starts_with("filesystem.read:")
                || signature.starts_with("filesystem.list:")
                || signature.starts_with("filesystem.search:"));
            if !keep { self.present.remove(signature); }
            keep
        });
    }
}

#[cfg(test)]
mod recent_tests {
    use super::*;

    #[test]
    fn canonical_signatures_ignore_object_spacing_and_key_order() {
        assert_eq!(
            canonical_call_signature("filesystem.read", r#"{"path": "a", "line": 1}"#),
            canonical_call_signature("filesystem.read", r#"{"line":1,"path":"a"}"#)
        );
    }

    #[test]
    fn recent_window_allows_rework_after_eviction_and_forgets_reads() {
        let mut recent = RecentToolCalls::new(2);
        assert!(recent.remember("filesystem.read:{\"path\":\"a\"}".into()));
        assert!(!recent.remember("filesystem.read:{\"path\":\"a\"}".into()));
        recent.remember("git.status:null".into());
        recent.forget_reads();
        assert!(recent.remember("filesystem.read:{\"path\":\"a\"}".into()));
    }
}

impl ToolOutcome {
    pub fn success(result: ToolResult) -> Self {
        let kind = semantic_failure(&result.structured);
        Self {
            ok: kind.is_none(),
            kind,
            output: result.output,
            structured: result.structured,
        }
    }

    pub fn from_result(result: Result<ToolResult, ToolError>) -> Self {
        match result {
            Ok(result) => Self::success(result),
            Err(error) => Self::from_error(error),
        }
    }

    pub fn from_error(error: ToolError) -> Self {
        let kind = match &error {
            ToolError::NotFound { .. } => ToolFailureKind::NotFound,
            ToolError::InvalidInput { .. } => ToolFailureKind::InvalidInput,
            ToolError::PermissionDenied(_)
            | ToolError::NeedsApproval { .. }
            | ToolError::ApprovalMismatch
            | ToolError::ApprovalDenied => {
                ToolFailureKind::Denied(DenialSource::Policy)
            }
            ToolError::TimedOut(_) => ToolFailureKind::Timeout,
            ToolError::Execution(_) | ToolError::UnknownTool(_) => ToolFailureKind::Execution,
        };
        Self {
            ok: false,
            kind: Some(kind),
            output: error.to_string(),
            structured: Value::Null,
        }
    }

    pub fn denied_by_user(output: impl Into<String>) -> Self {
        Self {
            ok: false,
            kind: Some(ToolFailureKind::Denied(DenialSource::User)),
            output: output.into(),
            structured: Value::Null,
        }
    }
}

fn semantic_failure(structured: &Value) -> Option<ToolFailureKind> {
    if structured.get("timed_out").and_then(Value::as_bool) == Some(true) {
        return Some(ToolFailureKind::Timeout);
    }
    if let Some(exit_code) = structured.get("exit_code") {
        if exit_code.as_i64() != Some(0) {
            return Some(ToolFailureKind::NonZeroExit);
        }
    }
    if structured.get("status").and_then(Value::as_str) == Some("nothing_to_commit") {
        return Some(ToolFailureKind::Execution);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn text_is_not_used_to_detect_success() {
        let outcome = ToolOutcome::success(ToolResult {
            output: "error: this is valid file content".into(),
            structured: json!({"path":"notes.txt"}),
        });
        assert!(outcome.ok);
        assert_eq!(outcome.kind, None);
    }

    #[test]
    fn shell_exit_and_timeout_are_typed() {
        assert_eq!(
            ToolOutcome::success(ToolResult {
                output: String::new(),
                structured: json!({"exit_code": 1, "timed_out": false}),
            })
            .kind,
            Some(ToolFailureKind::NonZeroExit)
        );
        assert_eq!(
            ToolOutcome::success(ToolResult {
                output: String::new(),
                structured: json!({"exit_code": null, "timed_out": true}),
            })
            .kind,
            Some(ToolFailureKind::Timeout)
        );
    }

    #[test]
    fn errors_keep_their_typed_category() {
        assert_eq!(
            ToolOutcome::from_error(ToolError::NotFound {
                tool: "filesystem.read".into(),
                path: "missing.txt".into(),
                hint: String::new(),
            })
            .kind,
            Some(ToolFailureKind::NotFound)
        );
        assert_eq!(
            ToolOutcome::from_error(ToolError::TimedOut(Duration::from_secs(1))).kind,
            Some(ToolFailureKind::Timeout)
        );
        assert_eq!(
            ToolOutcome::from_error(ToolError::ApprovalMismatch).kind,
            Some(ToolFailureKind::Denied(DenialSource::Policy))
        );
    }

    #[test]
    fn nothing_to_commit_is_not_a_successful_commit() {
        let outcome = ToolOutcome::success(ToolResult {
            output: "Изменений для коммита нет".into(),
            structured: json!({"status": "nothing_to_commit"}),
        });
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::Execution));
    }
}
