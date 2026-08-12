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

pub fn failure_kind_name(kind: ToolFailureKind) -> &'static str {
    match kind {
        ToolFailureKind::NotFound => "not_found",
        ToolFailureKind::InvalidInput => "invalid_input",
        ToolFailureKind::Denied(DenialSource::Policy) => "denied_policy",
        ToolFailureKind::Denied(DenialSource::User) => "denied_user",
        ToolFailureKind::Denied(DenialSource::Escalation) => "denied_escalation",
        ToolFailureKind::Timeout => "timeout",
        ToolFailureKind::NonZeroExit => "non_zero_exit",
        ToolFailureKind::Execution => "execution",
    }
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
    let value = serde_json::from_str::<Value>(arguments)
        .unwrap_or_else(|_| Value::String(arguments.into()));
    format!("{name}:{}", canonical_json(&value))
}

fn canonical_json(value: &Value) -> String {
    match value {
        Value::Object(object) => {
            let sorted = object
                .iter()
                .map(|(key, value)| (key, canonical_json(value)))
                .collect::<BTreeMap<_, _>>();
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
        Self {
            capacity,
            order: VecDeque::new(),
            present: HashMap::new(),
        }
    }

    pub fn remember(&mut self, signature: String) -> bool {
        if self.present.contains_key(&signature) {
            return false;
        }
        self.present.insert(signature.clone(), ());
        self.order.push_back(signature);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.present.remove(&old);
            }
        }
        true
    }

    pub fn forget_reads(&mut self) {
        self.order.retain(|signature| {
            let keep = !(signature.starts_with("filesystem.read:")
                || signature.starts_with("filesystem.list:")
                || signature.starts_with("filesystem.search:"));
            if !keep {
                self.present.remove(signature);
            }
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
            ToolError::PermissionDenied(_) => ToolFailureKind::Denied(DenialSource::Policy),
            ToolError::NeedsApproval { .. } => ToolFailureKind::Denied(DenialSource::Policy),
            ToolError::ApprovalMismatch => ToolFailureKind::Denied(DenialSource::Policy),
            ToolError::ApprovalDenied => ToolFailureKind::Denied(DenialSource::User),
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
    if let Some(ok_flag) = structured.get("ok").and_then(Value::as_bool) {
        if !ok_flag {
            return Some(ToolFailureKind::Execution);
        }
    }
    None
}

/// Классифицирует результат инструмента согласно приоритетной логике.
///
/// Порядок классификации:
/// 1. ToolError::NotFound → ok=false, kind=NotFound
/// 2. ToolError::InvalidInput → ok=false, kind=InvalidInput
/// 3. ToolError::PermissionDenied → ok=false, kind=Denied(Policy)
/// 4. ToolError::TimedOut → ok=false, kind=Timeout
/// 5. exit_code != 0 → ok=false, kind=NonZeroExit
/// 6. structured["ok"] exists → ok = structured["ok"]
/// 7. Иначе → ok=true
pub fn classify_tool_outcome(
    result: Result<ToolResult, ToolError>,
    output: String,
) -> ToolOutcome {
    match result {
        Ok(tool_result) => {
            // Начинаем с успешного результата, затем проверяем семантические ошибки
            let kind = semantic_failure(&tool_result.structured);
            ToolOutcome {
                ok: kind.is_none(),
                kind,
                output,
                structured: tool_result.structured,
            }
        }
        Err(error) => {
            // Приоритетная классификация ошибок
            let kind = match &error {
                // Приоритет 1: NotFound
                ToolError::NotFound { .. } => ToolFailureKind::NotFound,
                // Приоритет 2: InvalidInput
                ToolError::InvalidInput { .. } => ToolFailureKind::InvalidInput,
                // Приоритет 3: PermissionDenied
                ToolError::PermissionDenied(_) => ToolFailureKind::Denied(DenialSource::Policy),
                // Приоритет 4: TimedOut
                ToolError::TimedOut(_) => ToolFailureKind::Timeout,
                // Остальное: Execution или специфичные отказы
                ToolError::NeedsApproval { .. } => ToolFailureKind::Denied(DenialSource::Policy),
                ToolError::ApprovalMismatch => ToolFailureKind::Denied(DenialSource::Policy),
                ToolError::ApprovalDenied => ToolFailureKind::Denied(DenialSource::User),
                ToolError::Execution(_) | ToolError::UnknownTool(_) => ToolFailureKind::Execution,
            };

            ToolOutcome {
                ok: false,
                kind: Some(kind),
                output,
                structured: Value::Null,
            }
        }
    }
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

    // === Тесты для classify_tool_outcome ===

    #[test]
    fn classify_toolerror_notfound_priority_1() {
        let outcome = classify_tool_outcome(
            Err(ToolError::NotFound {
                tool: "filesystem.read".into(),
                path: "missing.txt".into(),
                hint: "файл не найден".into(),
            }),
            "resource not found for filesystem.read: missing.txt".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::NotFound));
    }

    #[test]
    fn classify_toolerror_invalidinput_priority_2() {
        let outcome = classify_tool_outcome(
            Err(ToolError::InvalidInput {
                tool: "shell.execute".into(),
                message: "program is required".into(),
            }),
            "invalid input for shell.execute: program is required".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::InvalidInput));
    }

    #[test]
    fn classify_permissiondenied_priority_3() {
        use evohime_permissions::Permission;
        let outcome = classify_tool_outcome(
            Err(ToolError::PermissionDenied(Permission::ShellExecute)),
            "permission denied: ShellExecute".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(
            outcome.kind,
            Some(ToolFailureKind::Denied(DenialSource::Policy))
        );
    }

    #[test]
    fn classify_shell_timedout_priority_4() {
        let outcome = classify_tool_outcome(
            Err(ToolError::TimedOut(Duration::from_secs(30))),
            "tool timed out after 30s".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::Timeout));
    }

    #[test]
    fn classify_shell_execute_exit_code_nonzero_priority_5() {
        let outcome = classify_tool_outcome(
            Ok(ToolResult {
                output: "command failed".into(),
                structured: json!({
                    "exit_code": 1,
                    "stdout": "out",
                    "stderr": "compilation error"
                }),
            }),
            "command failed".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::NonZeroExit));
    }

    #[test]
    fn classify_filesystem_read_success_ok_true() {
        let outcome = classify_tool_outcome(
            Ok(ToolResult {
                output: "file content here".into(),
                structured: json!({"path": "src/main.rs"}),
            }),
            "file content here".into(),
        );
        assert!(outcome.ok);
        assert_eq!(outcome.kind, None);
    }

    #[test]
    fn classify_git_commit_nothing_to_commit() {
        let outcome = classify_tool_outcome(
            Ok(ToolResult {
                output: "nothing to commit, working tree clean".into(),
                structured: json!({"status": "nothing_to_commit"}),
            }),
            "nothing to commit, working tree clean".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::Execution));
    }

    #[test]
    fn classify_approval_denied_by_user() {
        let outcome = classify_tool_outcome(
            Err(ToolError::ApprovalDenied),
            "approval was denied for this call".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(
            outcome.kind,
            Some(ToolFailureKind::Denied(DenialSource::User))
        );
    }

    #[test]
    fn classify_structured_ok_flag_false() {
        let outcome = classify_tool_outcome(
            Ok(ToolResult {
                output: "operation did not complete as expected".into(),
                structured: json!({"ok": false, "reason": "validation failed"}),
            }),
            "operation did not complete as expected".into(),
        );
        assert!(!outcome.ok);
        assert_eq!(outcome.kind, Some(ToolFailureKind::Execution));
    }

    #[test]
    fn classify_structured_ok_flag_true() {
        let outcome = classify_tool_outcome(
            Ok(ToolResult {
                output: "success".into(),
                structured: json!({"ok": true}),
            }),
            "success".into(),
        );
        assert!(outcome.ok);
        assert_eq!(outcome.kind, None);
    }
}
