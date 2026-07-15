use evohime_agent_runtime::{run_agent_loop, AgentConfig};
use evohime_model_gateway::mock_gateway;
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_protocol::ServerEvent;
use evohime_tool_runtime::ToolRegistry;
use tokio::sync::mpsc;
use uuid::Uuid;

#[tokio::test]
async fn agent_loop_streams_model_tokens() {
    let temp = tempfile::tempdir().expect("tempdir");
    let demo_file = temp.path().join("context.md");
    std::fs::write(&demo_file, "# Demo\nHello from workspace.").expect("write");

    let (tx, mut rx) = mpsc::unbounded_channel();
    let gateway = mock_gateway(vec!["Evo".into(), "Hime".into()]);
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
