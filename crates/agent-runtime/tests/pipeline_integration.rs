//! Agent pipeline integration: tools → completion and approval pause → resume.

use evohime_agent_runtime::{
    run_agent_loop, run_agent_loop_resumed, AgentConfig, AgentError, AgentResumeContext,
};
use evohime_model_gateway::providers::{
    ChatFuture, ChatMessage, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use evohime_model_gateway::{ChatResult, ModelGateway, NativeToolCall, ToolSpec};
use evohime_permissions::{Permission, PermissionEngine, PermissionMode};
use evohime_protocol::{PlanStep, ServerEvent};
use evohime_tool_runtime::{ToolError, ToolRegistry};
use futures_util::stream;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use tokio::sync::mpsc;
use uuid::Uuid;

struct ScriptedProvider {
    calls: Arc<AtomicUsize>,
    responses: Mutex<Vec<Vec<String>>>,
}

impl ScriptedProvider {
    fn new(responses: Vec<Vec<String>>) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            responses: Mutex::new(responses),
        }
    }
}

impl ModelProvider for ScriptedProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Mock
    }

    fn model_name(&self) -> &str {
        "scripted-model"
    }

    fn base_url(&self) -> &str {
        "mock://scripted"
    }

    fn stream_chat(&self, _messages: &[ChatMessage]) -> TokenStream {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let chunks = self
            .responses
            .lock()
            .expect("responses")
            .get(index)
            .cloned()
            .unwrap_or_default();
        Box::pin(stream::iter(chunks.into_iter().map(|chunk| {
            Ok::<_, ProviderError>(evohime_model_gateway::ChatStreamItem::Delta(chunk))
        })))
    }

    fn chat_with_tools(
        &self,
        _model: Option<&str>,
        _messages: &[ChatMessage],
        _tools: &[ToolSpec],
    ) -> ChatFuture {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let response = self
            .responses
            .lock()
            .expect("responses")
            .get(index)
            .and_then(|chunks| chunks.first())
            .cloned()
            .unwrap_or_default();
        let value = serde_json::from_str::<serde_json::Value>(&response).ok();
        let tool_calls = value
            .as_ref()
            .and_then(|value| value.get("tool"))
            .and_then(serde_json::Value::as_str)
            .map(|tool| {
                vec![NativeToolCall {
                    id: format!("call-{index}"),
                    name: tool.replace('.', "_"),
                    arguments: serde_json::to_string(
                        value
                            .as_ref()
                            .and_then(|value| value.get("input"))
                            .unwrap_or(&serde_json::Value::Null),
                    )
                    .unwrap_or_else(|_| "{}".into()),
                }]
            })
            .or_else(|| {
                value
                    .as_ref()
                    .and_then(|value| value.get("message"))
                    .and_then(serde_json::Value::as_str)
                    .map(|message| {
                        vec![NativeToolCall {
                            id: format!("reply-{index}"),
                            name: "assistant_reply".into(),
                            arguments: serde_json::json!({"message": message}).to_string(),
                        }]
                    })
            })
            .unwrap_or_default();
        Box::pin(async move {
            Ok(ChatResult {
                content: String::new(),
                tool_calls,
                usage: None,
            })
        })
    }
}

fn agent_config(temp: &std::path::Path, demo_file: &std::path::Path) -> AgentConfig {
    AgentConfig {
        task_id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
        user_message: "run pipeline".to_string(),
        created_at: chrono::Utc::now(),
        demo_file_path: demo_file.to_path_buf(),
        workspace_root: temp.to_path_buf(),
        model_route: "default".to_string(),
        model: None,
        planning_model_route: "default".to_string(),
        planning_model: None,
        planning_memory_context: None,
        memory_pool: None,
        workspace_key: String::new(),
        is_subagent: false,
        subagent_depth: 0,
        subagent_max_steps: None,
        telemetry: None,
    }
}

#[tokio::test]
async fn task_start_tool_events_then_completion() {
    let temp = tempfile::tempdir().expect("tempdir");
    let demo_file = temp.path().join("notes.md");
    std::fs::write(&demo_file, "hello notes").expect("write");

    let provider = ScriptedProvider::new(vec![
        vec![r#"{"tool":"filesystem.read","input":{"path":"notes.md"}}"#.into()],
        vec![r#"{"message":"ok-done"}"#.into()],
    ]);
    let gateway = ModelGateway::from_provider(Arc::new(provider));
    let tools = ToolRegistry::bootstrap();
    let (tx, mut rx) = mpsc::unbounded_channel();

    let result = run_agent_loop(
        agent_config(temp.path(), &demo_file),
        &gateway,
        &tools,
        vec![],
        vec![],
        tx,
    )
    .await
    .expect("agent completes");
    assert!(result.final_message.contains("ok"));

    let mut saw_tool_started = false;
    let mut saw_tool_completed = false;
    let mut saw_task_completed = false;
    while let Some(event) = rx.recv().await {
        match event {
            ServerEvent::ToolStarted {
                tool_name: ref name,
                ..
            } if name == "filesystem.read" => saw_tool_started = true,
            ServerEvent::ToolCompleted {
                tool_name: ref name,
                success: true,
                ..
            } if name == "filesystem.read" => saw_tool_completed = true,
            ServerEvent::TaskCompleted { .. } => saw_task_completed = true,
            _ => {}
        }
    }
    assert!(saw_tool_started, "expected tool.started");
    assert!(saw_tool_completed, "expected tool.completed");
    assert!(saw_task_completed, "expected task.completed");
}

#[tokio::test]
async fn approval_pauses_write_then_resume_completes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let demo_file = temp.path().join("context.md");
    std::fs::write(&demo_file, "# ctx").expect("write");

    let permissions = PermissionEngine::new();
    permissions
        .set_mode(Permission::FilesystemWrite, PermissionMode::Ask)
        .await;
    let tools = ToolRegistry::bootstrap_with_permissions(permissions.clone());

    let write_desc = "Update `out.txt` with:\n```text\napproved\n```";
    let provider = ScriptedProvider::new(vec![vec![
        r#"{"tool":"filesystem.write","input":{"path":"out.txt","content":"approved"}}"#.into(),
    ]]);
    let gateway = ModelGateway::from_provider(Arc::new(provider));
    let (tx, _rx) = mpsc::unbounded_channel();
    let config = agent_config(temp.path(), &demo_file);

    let err = run_agent_loop(config.clone(), &gateway, &tools, vec![], vec![], tx)
        .await
        .expect_err("write should require approval");

    let approval_id = match err {
        AgentError::Tool(ToolError::NeedsApproval {
            approval_id, tool, ..
        }) => {
            assert_eq!(tool, "filesystem.write");
            approval_id
        }
        other => panic!("unexpected error: {other}"),
    };

    assert!(
        !temp.path().join("out.txt").exists(),
        "file must not be written before approval"
    );

    let resolved = permissions
        .resolve(approval_id, true)
        .await
        .expect("grant approval");
    assert_eq!(resolved, evohime_permissions::ApprovalState::Granted);
    permissions
        .set_mode(Permission::FilesystemWrite, PermissionMode::Allow)
        .await;

    let resume_provider = ScriptedProvider::new(vec![
        vec![
            r#"{"tool":"filesystem.write","input":{"path":"out.txt","content":"approved"}}"#.into(),
        ],
        vec![r#"{"message":"wrote file"}"#.into()],
    ]);
    let resume_gateway = ModelGateway::from_provider(Arc::new(resume_provider));
    let (resume_tx, mut resume_rx) = mpsc::unbounded_channel();

    let result = run_agent_loop_resumed(
        config,
        &resume_gateway,
        &tools,
        vec![],
        vec![],
        resume_tx,
        AgentResumeContext {
            workspace_context: Some("# ctx".into()),
            plan: Some(vec![PlanStep {
                id: "step-1".into(),
                tool_name: "filesystem.write".into(),
                description: write_desc.into(),
                depends_on: vec![],
            }]),
            completed_step_ids: vec![],
            tool_results: vec![],
            pause_reason: Some("approval_required".into()),
        },
    )
    .await
    .expect("resume after approval");

    assert!(result.final_message.contains("wrote"));
    let content = std::fs::read_to_string(temp.path().join("out.txt")).expect("out.txt");
    assert!(content.contains("approved"));

    let mut saw_write = false;
    while let Some(event) = resume_rx.recv().await {
        if let ServerEvent::ToolCompleted {
            tool_name,
            success: true,
            ..
        } = event
        {
            if tool_name == "filesystem.write" {
                saw_write = true;
            }
        }
    }
    assert!(
        saw_write,
        "expected filesystem.write completed after resume"
    );
}
