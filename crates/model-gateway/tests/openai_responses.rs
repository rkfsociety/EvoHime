use evohime_model_gateway::config::LiteRouterConfig;
use evohime_model_gateway::providers::openai_responses::OpenAIResponsesProvider;
use evohime_model_gateway::providers::{ChatMessage, ChatRole, ModelProvider};
use evohime_model_gateway::RetryPolicy;
use futures_util::StreamExt;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn responses_http_error_does_not_include_provider_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .respond_with(ResponseTemplate::new(400).set_body_string(
            "invalid request: https://provider.test/responses?token=secret-provider-body",
        ))
        .mount(&server)
        .await;

    let provider = OpenAIResponsesProvider::with_retry(
        LiteRouterConfig {
            api_key: "sk-test".to_string(),
            base_url: format!("{}/v1", server.uri()),
            model: "responses-test-model".to_string(),
        },
        RetryPolicy::none(),
    )
    .expect("provider");

    let mut stream = provider.stream_chat(&[ChatMessage::text(ChatRole::User, "hi")]);
    let error = stream
        .next()
        .await
        .expect("error item")
        .expect_err("HTTP failure should be returned");

    assert!(error.to_string().contains("400"));
    assert!(!error.to_string().contains("provider.test"));
    assert!(!error.to_string().contains("secret-provider-body"));
}

#[tokio::test]
async fn responses_sends_evohime_identity_headers() {
    use wiremock::matchers::header;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/responses"))
        .and(header("user-agent", evohime_model_gateway::CLIENT_USER_AGENT))
        .and(header("x-title", evohime_model_gateway::CLIENT_X_TITLE))
        .and(header(
            "x-openrouter-title",
            evohime_model_gateway::CLIENT_X_OPENROUTER_TITLE,
        ))
        .and(header(
            "http-referer",
            evohime_model_gateway::CLIENT_HTTP_REFERER,
        ))
        .and(header("referer", evohime_model_gateway::CLIENT_HTTP_REFERER))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n")
                .insert_header("content-type", "text/event-stream"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let provider = OpenAIResponsesProvider::with_retry(
        LiteRouterConfig {
            api_key: "sk-test".to_string(),
            base_url: format!("{}/v1", server.uri()),
            model: "responses-test-model".to_string(),
        },
        RetryPolicy::none(),
    )
    .expect("provider");

    let mut stream = provider.stream_chat(&[ChatMessage::text(ChatRole::User, "hi")]);
    while let Some(item) = stream.next().await {
        item.expect("stream item");
    }
}
