use evohime_agent_runtime::{run_agent_loop, AgentConfig};
use evohime_model_gateway::providers::{
    ChatMessage, ChatRole, ModelProvider, ProviderError, ProviderKind, TokenStream,
};
use evohime_model_gateway::ModelGateway;
use evohime_protocol::ServerEvent;
use evohime_tool_runtime::ToolRegistry;
use futures_util::stream;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use uuid::Uuid;

struct TwoPhaseProvider {
    calls: Arc<AtomicUsize>,
}

impl ModelProvider for TwoPhaseProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Mock
    }
    fn model_name(&self) -> &str {
        "test-model"
    }
    fn base_url(&self) -> &str {
        "mock://test"
    }
    fn stream_chat(&self, _messages: &[ChatMessage]) -> TokenStream {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let chunks = if call == 0 {
            vec![r#"[{"id":"step-1","tool_name":"assistant.reply","description":"Respond","depends_on":[]}]"#.to_string()]
        } else {
            vec!["Evo".to_string(), "Hime".to_string()]
        };
        Box::pin(stream::iter(chunks.into_iter().map(|chunk| {
            Ok::<_, ProviderError>(evohime_model_gateway::ChatStreamItem::Delta(chunk))
        })))
    }
}

#[tokio::test]
async fn agent_loop_streams_model_tokens() {
    let temp = tempfile::tempdir().expect("tempdir");
    let demo_file = temp.path().join("context.md");
    std::fs::write(&demo_file, "# Demo\nHello from workspace.").expect("write");

    let (tx, mut rx) = mpsc::unbounded_channel();
    let gateway = ModelGateway::from_provider(Arc::new(TwoPhaseProvider {
        calls: Arc::new(AtomicUsize::new(0)),
    }));
    let tools = ToolRegistry::bootstrap();

    let result = run_agent_loop(
        AgentConfig {
            task_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            user_message: "Explain the file".to_string(),
            created_at: chrono::Utc::now(),
            demo_file_path: demo_file.clone(),
            workspace_root: temp.path().to_path_buf(),
            model_route: "default".to_string(),
            model: None,
            planning_model_route: "default".to_string(),
            planning_model: None,
            memory_pool: None,
            workspace_key: String::new(),
            is_subagent: false,
            subagent_depth: 0,
            subagent_max_steps: None,
            telemetry: None,
        },
        &gateway,
        &tools,
        vec![ChatMessage {
            role: ChatRole::User,
            content: "previous".to_string(),
        }],
        vec![],
        tx,
    )
    .await
    .expect("agent completes");

    assert_eq!(result.final_message, "EvoHime");

    let mut deltas = Vec::new();
    while let Some(event) = rx.recv().await {
        if let ServerEvent::AgentMessageDelta { delta, .. } = event {
            deltas.push(delta);
        }
    }

    assert_eq!(deltas.join(""), "EvoHime");
}
