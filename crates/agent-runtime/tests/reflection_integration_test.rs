use evohime_agent_runtime::reflection::{ReflectionEngine, ToolOutputContext};
use evohime_protocol::ReflectionAction;

#[test]
fn test_reflection_analyzes_tool_success() {
    let context = ToolOutputContext {
        tool_name: "filesystem.read".to_string(),
        tool_input: serde_json::json!({}),
        tool_output: "file contents".to_string(),
        tool_error: None,
        expected_outcome: Some("file contents".to_string()),
    };

    let (analysis, action) = ReflectionEngine::analyze_tool_output(&context, vec![]);

    assert!(analysis.success_score > 0.8, "Success score should be high");
    assert_eq!(
        action,
        ReflectionAction::Proceed,
        "Should proceed on success"
    );
}

#[test]
fn test_reflection_detects_tool_error() {
    let context = ToolOutputContext {
        tool_name: "filesystem.read".to_string(),
        tool_input: serde_json::json!({}),
        tool_output: String::new(),
        tool_error: Some("Permission denied".to_string()),
        expected_outcome: None,
    };

    let (analysis, action) = ReflectionEngine::analyze_tool_output(
        &context,
        vec![("E_PERM".to_string(), "Permission denied".to_string(), 0.85)],
    );

    assert_eq!(analysis.success_score, 0.0, "Error score should be 0");
    assert!(
        !analysis.error_patterns.is_empty(),
        "Should detect error patterns"
    );
    assert!(
        matches!(action, ReflectionAction::RetryTool),
        "Should retry on error"
    );
}

#[test]
fn test_reflection_matches_failure_patterns() {
    let patterns = vec![
        ("P001".to_string(), "connection refused".to_string(), 0.9),
        ("P002".to_string(), "timeout".to_string(), 0.85),
    ];

    let context = ToolOutputContext {
        tool_name: "shell.execute".to_string(),
        tool_input: serde_json::json!({}),
        tool_output: "Error: connection refused on port 3000".to_string(),
        tool_error: None,
        expected_outcome: Some("Server started".to_string()),
    };

    let (analysis, _) = ReflectionEngine::analyze_tool_output(&context, patterns);

    assert!(
        analysis
            .error_patterns
            .iter()
            .any(|p| p.pattern_name.contains("connection")),
        "Should match connection refused pattern"
    );
}

#[test]
fn test_reflection_action_depends_on_score() {
    // High score → Proceed
    let ctx1 = ToolOutputContext {
        tool_name: "git.status".to_string(),
        tool_input: serde_json::json!({}),
        tool_output: "On branch main\nnothing to commit".to_string(),
        tool_error: None,
        expected_outcome: None,
    };
    let (_, action1) = ReflectionEngine::analyze_tool_output(&ctx1, vec![]);
    assert_eq!(action1, ReflectionAction::Proceed);

    // Low score → RetryTool
    let ctx2 = ToolOutputContext {
        tool_name: "filesystem.read".to_string(),
        tool_input: serde_json::json!({}),
        tool_output: String::new(),
        tool_error: Some("Not found".to_string()),
        expected_outcome: None,
    };
    let (_, action2) = ReflectionEngine::analyze_tool_output(&ctx2, vec![]);
    assert!(
        matches!(action2, ReflectionAction::RetryTool),
        "Low score should suggest retry"
    );
}
