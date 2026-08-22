use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolRiskLevel {
    None,
    Low,
    Medium,
    High,
}

impl ToolRiskLevel {
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
        "filesystem.read"
        | "filesystem.search"
        | "filesystem.list"
        | "git.status"
        | "git.diff"
        | "git.log"
        | "git.show"
        | "git.blame"
        | "git.changed_files"
        | "browser.open"
        | "browser.extract"
        | "browser.session.read"
        | "browser.session.screenshot"
        | "memory.search"
        | "app.list"
        | "http.fetch" => ToolRiskLevel::None,

        "git.pull" | "browser.session.navigate" => ToolRiskLevel::Low,

        "filesystem.write"
        | "filesystem.patch"
        | "git.commit"
        | "mcp.call"
        | "browser.session.click"
        | "browser.session.type"
        | "agent.run"
        // Запуск приложения меняет состояние рабочего стола, но не трогает
        // ни файлы, ни сеть от имени пользователя: это `Medium`, а не `High`
        // уровень `shell.execute`, у которого произвольная командная строка.
        | "app.open" => ToolRiskLevel::Medium,

        "shell.execute" | "git.push" => ToolRiskLevel::High,

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
            "filesystem.read",
            "filesystem.search",
            "filesystem.list",
            "git.status",
            "git.diff",
            "browser.open",
            "browser.extract",
            "browser.session.read",
            "browser.session.screenshot",
            "memory.search",
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
        // http.fetch's Input struct has no `method` field — it can never mutate.
        assert_eq!(
            classify_call_risk("http.fetch", &json!({"url": "https://example.com"})),
            ToolRiskLevel::None
        );
    }

    #[test]
    fn classifies_low_risk_tools() {
        for tool in ["git.pull", "browser.session.navigate"] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::Low);
        }
    }

    #[test]
    fn classifies_medium_risk_tools() {
        for tool in [
            "filesystem.write",
            "filesystem.patch",
            "git.commit",
            "mcp.call",
            "browser.session.click",
            "browser.session.type",
            "agent.run",
        ] {
            assert_eq!(classify_call_risk(tool, &json!({})), ToolRiskLevel::Medium);
        }
    }

    #[test]
    fn classifies_high_risk_tools() {
        for tool in ["shell.execute", "git.push"] {
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
