use evohime_model_gateway::mock_gateway;
use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use futures_util::StreamExt;

#[tokio::test]
async fn gateway_streams_tokens_from_mock_provider() {
    let gateway = mock_gateway(vec!["Lite".into(), "Router".into()]);
    let mut stream = gateway.stream_chat(&[ChatMessage {
        role: ChatRole::User,
        content: "ping".to_string(),
    }]);

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk.expect("chunk ok"));
    }

    assert_eq!(output, "LiteRouter");
}
