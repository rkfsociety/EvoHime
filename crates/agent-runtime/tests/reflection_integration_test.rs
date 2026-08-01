use evohime_agent_runtime::reflection::{ReflectionEngine, ToolOutputContext};
use evohime_agent_runtime::{ReflectionStage, ReflectionStageInput};
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

/// End-to-end reflection stage against a real database: a remembered
/// `failure_pattern` must reach the verdict and the verdict must be persisted.
#[tokio::test]
async fn reflection_stage_uses_experience_memory_and_persists() {
    let Some(pool) = evohime_storage::connect_integration_pool().await else {
        eprintln!("skipping reflection persistence test: database unavailable");
        return;
    };

    let operator_id = evohime_storage::operators::BOOTSTRAP_OWNER_ID;
    let scope_key = evohime_storage::LOCAL_OPERATOR_SCOPE_KEY;
    let mut lesson = evohime_storage::NewMemoryItem::candidate_fact(
        evohime_storage::MemoryScope::Experience,
        scope_key,
        "shell command assumed bash on a windows host",
    );
    lesson.kind = evohime_storage::MemoryKind::FailurePattern;
    lesson.status = evohime_storage::MemoryStatus::Active;
    lesson.confidence = 0.8;
    lesson.importance = 0.9;
    let lesson = evohime_storage::insert_memory_item(&pool, &lesson)
        .await
        .expect("insert lesson");

    let session = evohime_storage::create_session(&pool)
        .await
        .expect("create session");
    let task = evohime_storage::create_task(&pool, session.id, "reflect", None, None, None)
        .await
        .expect("create task");

    let output = ReflectionStage::execute(
        Some(&pool),
        ReflectionStageInput {
            task_id: task.id,
            operator_id,
            tool_call_id: "call_reflection_it".to_string(),
            tool_name: "shell.execute".to_string(),
            tool_input: serde_json::json!({"program": "bash"}),
            tool_output: String::new(),
            tool_error: Some(
                "shell command assumed bash on a windows host: program not found".to_string(),
            ),
            consecutive_failures: 0,
        },
    )
    .await;

    assert_eq!(output.analysis.success_score, 0.0);
    assert!(
        output
            .analysis
            .error_patterns
            .iter()
            .any(|pattern| pattern.pattern_id == lesson.id.to_string()),
        "the remembered lesson must be matched: {:?}",
        output.analysis.error_patterns
    );

    let stored = evohime_storage::ReflectionEventDAO::new(pool.clone())
        .get_reflection_events_by_task(task.id)
        .await
        .expect("load reflection events");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].reflection_action, "retry_tool");
    assert_eq!(stored[0].reflection_type, "post_tool_execution");
    assert!(stored[0].recommendation.is_some());

    let _ = evohime_storage::delete_memory_item(&pool, lesson.id).await;
}
