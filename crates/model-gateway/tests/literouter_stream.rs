use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use evohime_model_gateway::config::LiteRouterConfig;
use evohime_model_gateway::providers::literouter::LiteRouterProvider;
use evohime_model_gateway::providers::{ChatMessage, ChatRole, ModelProvider};
use futures_util::StreamExt;

#[tokio::test]
async fn literouter_streams_sse_chunks() {
    let server = MockServer::start().await;
    let sse_body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"!\"}}]}\n\n",
        "data: [DONE]\n\n",
    );

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let provider = LiteRouterProvider::new(LiteRouterConfig {
        api_key: "lr_test".to_string(),
        base_url: format!("{}/v1", server.uri()),
        model: "deepseek:free".to_string(),
    })
    .expect("provider");

    let mut stream = provider.stream_chat(&[ChatMessage {
        role: ChatRole::User,
        content: "hi".to_string(),
    }]);

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        output.push_str(&chunk.expect("chunk"));
    }

    assert_eq!(output, "Hello!");
}
