use serde_json::Value;

/// Relative impact tier assigned before a tool call is executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolRiskLevel {
    /// No material side effect is expected.
    None,
    /// Limited side effects with narrow scope.
    Low,
    /// Meaningful changes or external effects may occur.
    Medium,
    /// Arbitrary command execution or similarly broad effects may occur.
    High,
}

impl ToolRiskLevel {
    /// Returns the stable lowercase label used by policy and telemetry.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolRiskLevel::None => "none",
            ToolRiskLevel::Low => "low",
            ToolRiskLevel::Medium => "medium",
            ToolRiskLevel::High => "high",
        }
    }
}

/// Classifies a resolved tool call (name + actual JSON input) by risk tier.
/// This is evaluated after a concrete tool call has been resolved, before
/// execution begins.
pub fn classify_call_risk(tool_name: &str, _input: &Value) -> ToolRiskLevel {
    match tool_name {
        "filesystem_read"
        | "filesystem_search"
        | "filesystem_list"
        | "git_status"
        | "git_diff"
        | "git_log"
        | "git_show"
        | "git_blame"
        | "git_changed_files"
        | "browser_open"
        | "browser_extract"
        | "browser_session_read"
        | "browser_session_screenshot"
        | "memory_search"
        | "goal_create"
        | "goal_verify"
        | "app_list"
        // `analysis_run` лишь считает по уже переданным аргументам: у него нет
        // ни файловых, ни сетевых, ни shell-прав, а объекты остаются внутри
        // task-scoped kernel-сессии.
        | "analysis_run"
        | "http_fetch" => ToolRiskLevel::None,

        "git_pull" | "browser_session_navigate" => ToolRiskLevel::Low,

        "filesystem_write"
        | "filesystem_patch"
        | "git_commit"
        | "mcp_call"
        | "browser_session_click"
        | "browser_session_type"
        | "agent_run"
        // Запуск приложения меняет состояние рабочего стола, но не трогает
        // ни файлы, ни сеть от имени пользователя: это `Medium`, а не `High`
        // уровень `shell_execute`, у которого произвольная командная строка.
        | "app_open" => ToolRiskLevel::Medium,

        // `script_execute` запускает произвольный код из файла воркспейса, поэтому
        // его влияние такое же широкое, как у `shell_execute`.
        "shell_execute" | "git_push" | "process_run" | "eva_workspace_run_command"
        | "script_execute" => ToolRiskLevel::High,

        _ => ToolRiskLevel::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_read_only_tools_as_none() {
        for tool in [
            "filesystem_read",
            "filesystem_search",
            "filesystem_list",
            "git_status",
            "git_diff",
            "browser_open",
            "browser_extract",
            "browser_session_read",
            "browser_session_screenshot",
            "memory_search",
            "analysis_run",
        ] {
            assert_eq!(
                classify_call_risk(tool, &json!({})),
                ToolRiskLevel::None,
                "{tool} should be None risk"
            );
        }
    }

    #[test]
    fn classifies_http_fetch_as_none_because_it_is_get_only() {
        // http_fetch's Input struct has no `method` field — it can never mutate.
        assert_eq!(
            classify_call_risk("http_fetch", &json!({"url": "https://example.com"})),
            ToolRiskLevel::None
        );
    }

    #[test]
    fn classifies_low_risk_tools() {
        for tool in ["git_pull", "browser_session_navigate"] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::Low);
        }
    }

    #[test]
    fn classifies_medium_risk_tools() {
        for tool in [
            "filesystem_write",
            "filesystem_patch",
            "git_commit",
            "mcp_call",
            "browser_session_click",
            "browser_session_type",
            "agent_run",
        ] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::Medium);
        }
    }

    #[test]
    fn classifies_high_risk_tools() {
        for tool in [
            "shell_execute",
            "git_push",
            "process_run",
            "eva_workspace_run_command",
            "script_execute",
        ] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::High);
        }
    }

    #[test]
    fn unknown_tool_defaults_to_medium_not_none() {
        assert_eq!(
            classify_call_risk("some.future.tool", &json!({})),
            ToolRiskLevel::Medium
        );
    }

    #[test]
    fn risk_levels_are_ordered() {
        assert!(ToolRiskLevel::None < ToolRiskLevel::Low);
        assert!(ToolRiskLevel::Low < ToolRiskLevel::Medium);
        assert!(ToolRiskLevel::Medium < ToolRiskLevel::High);
    }

    #[test]
    fn as_str_matches_expected_wire_values() {
        assert_eq!(ToolRiskLevel::None.as_str(), "none");
        assert_eq!(ToolRiskLevel::Low.as_str(), "low");
        assert_eq!(ToolRiskLevel::Medium.as_str(), "medium");
        assert_eq!(ToolRiskLevel::High.as_str(), "high");
    }
}
