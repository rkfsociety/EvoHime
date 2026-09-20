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
