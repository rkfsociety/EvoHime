use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use evohime_model_gateway::config::LiteRouterConfig;
use evohime_model_gateway::providers::literouter::LiteRouterProvider;
use evohime_model_gateway::providers::{ChatMessage, ChatRole, ModelProvider};
use evohime_model_gateway::{ChatStreamItem, LlmUsage, RetryPolicy};
use futures_util::StreamExt;

fn push_delta(output: &mut String, item: ChatStreamItem) {
    if let ChatStreamItem::Delta(text) = item {
        output.push_str(&text);
    }
}

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
        push_delta(&mut output, chunk.expect("chunk"));
    }

    assert_eq!(output, "Hello!");
}

#[tokio::test]
async fn literouter_streams_usage_chunk() {
    let server = MockServer::start().await;
    let sse_body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
        "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
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
    let mut usage = None;
    while let Some(chunk) = stream.next().await {
        match chunk.expect("chunk") {
            ChatStreamItem::Delta(text) => output.push_str(&text),
            ChatStreamItem::Usage(value) => usage = Some(value),
        }
    }

    assert_eq!(output, "Hi");
    assert_eq!(
        usage,
        Some(LlmUsage {
            prompt_tokens: 3,
            completion_tokens: 1,
            total_tokens: 4,
        })
    );
}

#[tokio::test]
async fn literouter_retries_after_429_then_streams() {
    let server = MockServer::start().await;
    let sse_body = concat!(
        "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
        "data: [DONE]\n\n",
    );

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_string("slow down"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(sse_body)
                .insert_header("content-type", "text/event-stream"),
        )
        .mount(&server)
        .await;

    let provider = LiteRouterProvider::with_retry(
        LiteRouterConfig {
            api_key: "lr_test".to_string(),
            base_url: format!("{}/v1", server.uri()),
            model: "deepseek:free".to_string(),
        },
        RetryPolicy::for_tests(2),
    )
    .expect("provider");

    let mut stream = provider.stream_chat(&[ChatMessage {
        role: ChatRole::User,
        content: "hi".to_string(),
    }]);

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        push_delta(&mut output, chunk.expect("chunk"));
    }
    assert_eq!(output, "ok");
}

#[tokio::test]
async fn literouter_does_not_retry_client_errors() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_string("bad request"))
        .expect(1)
        .mount(&server)
        .await;

    let provider = LiteRouterProvider::with_retry(
        LiteRouterConfig {
            api_key: "lr_test".to_string(),
            base_url: format!("{}/v1", server.uri()),
            model: "deepseek:free".to_string(),
        },
        RetryPolicy::for_tests(3),
    )
    .expect("provider");

    let mut stream = provider.stream_chat(&[ChatMessage {
        role: ChatRole::User,
        content: "hi".to_string(),
    }]);

    let err = stream
        .next()
        .await
        .expect("error item")
        .expect_err("should fail");
    assert!(err.to_string().contains("400"));
    assert!(stream.next().await.is_none());
}
