use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_model_gateway::{
    fetch_model_catalog, mock_gateway, ChatStreamItem, ModelGateway, ModelGatewayConfig,
    ModelRouteConfig, MAX_MODEL_CATALOG_BYTES, MAX_MODEL_CATALOG_ENTRIES,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn push_delta(output: &mut String, item: ChatStreamItem) {
    if let ChatStreamItem::Delta(text) = item {
        output.push_str(&text);
    }
}

#[tokio::test]
async fn gateway_streams_tokens_from_mock_provider() {
    let gateway = mock_gateway(vec!["Lite".into(), "Router".into()]);
    let mut stream = gateway.stream_chat(&[ChatMessage::text(ChatRole::User, "ping")]);

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        push_delta(&mut output, chunk.expect("chunk ok"));
    }

    assert_eq!(output, "LiteRouter");
}

#[tokio::test]
async fn gateway_streams_tokens_from_named_route() {
    let gateway = ModelGateway::from_routes(
        "default",
        HashMap::from([
            (
                "default".to_string(),
                Arc::new(evohime_model_gateway::providers::mock::MockProvider::new(
                    "default-model",
                    vec!["default".into()],
                )) as Arc<dyn evohime_model_gateway::providers::ModelProvider>,
            ),
            (
                "planner".to_string(),
                Arc::new(evohime_model_gateway::providers::mock::MockProvider::new(
                    "planner-model",
                    vec!["planner".into()],
                )) as Arc<dyn evohime_model_gateway::providers::ModelProvider>,
            ),
        ]),
    );

    let mut stream = gateway
        .stream_chat_for_route("planner", &[ChatMessage::text(ChatRole::User, "ping")])
        .expect("named route exists");

    let mut output = String::new();
    while let Some(chunk) = stream.next().await {
        push_delta(&mut output, chunk.expect("chunk ok"));
    }

    assert_eq!(output, "planner");
    assert_eq!(gateway.model_name(), "default-model");
}

#[test]
fn config_response_lists_routes() {
    let config = ModelGatewayConfig {
        default_route: "default".to_string(),
        routes: HashMap::from([
            (
                "default".to_string(),
                ModelRouteConfig::literouter(
                    "lr_default",
                    "https://api.literouter.com/v1",
                    "deepseek:free",
                ),
            ),
            (
                "planner".to_string(),
                ModelRouteConfig::literouter(
                    "lr_planner",
                    "https://api.literouter.com/v1",
                    "mistral:free",
                ),
            ),
        ]),
    };

    let response = ModelGateway::config_response(&config);

    assert_eq!(response.default_route, "default");
    assert_eq!(response.routes.len(), 2);
    assert!(response.routes.iter().any(|route| route.name == "planner"));
    assert_eq!(response.routes[0].name, "default");
}

#[test]
fn config_response_uses_provider_model_catalog() {
    let config = ModelGatewayConfig {
        default_route: "default".to_string(),
        routes: HashMap::from([(
            "default".to_string(),
            ModelRouteConfig::literouter(
                "lr_default",
                "https://api.literouter.com/v1",
                "deepseek-v3.2:free",
            ),
        )]),
    };
    let available_models = HashMap::from([(
        "default".to_string(),
        vec![
            "deepseek-v3.2:free".to_string(),
            "gpt-oss-20b:free".to_string(),
        ],
    )]);

    let response = ModelGateway::config_response_with_models(&config, &available_models);

    assert_eq!(response.available_models, available_models["default"]);
    assert_eq!(
        response.routes[0].available_models,
        available_models["default"]
    );
}

#[tokio::test]
async fn model_catalog_is_sorted_deduplicated_and_trimmed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"data":[{"id":" z "},{"id":"a"},{"id":"a"},{"id":""}]}"#),
        )
        .mount(&server)
        .await;

    let entries = fetch_model_catalog(&ModelRouteConfig::openai_compatible(
        "provider-secret",
        format!("{}/v1", server.uri()),
        "a",
    ))
    .await
    .expect("catalog");

    assert_eq!(
        entries
            .into_iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>(),
        vec!["a", "z"]
    );
}

#[tokio::test]
async fn model_catalog_rejects_oversized_and_overfull_responses_without_body() {
    let server = MockServer::start().await;
    let route =
        ModelRouteConfig::openai_compatible("provider-secret", format!("{}/v1", server.uri()), "a");
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "{{\"data\":[],\"padding\":\"{}\"}}",
            "x".repeat(MAX_MODEL_CATALOG_BYTES)
        )))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let error = fetch_model_catalog(&route).await.expect_err("size bound");
    assert!(error.to_string().contains("size limit"));
    assert!(!error.to_string().contains("provider-secret"));

    let entries = (0..=MAX_MODEL_CATALOG_ENTRIES)
        .map(|index| serde_json::json!({"id": format!("model-{index}")}))
        .collect::<Vec<_>>();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": entries
        })))
        .mount(&server)
        .await;
    let error = fetch_model_catalog(&route).await.expect_err("entry bound");
    assert!(error.to_string().contains("too many entries"));
}

#[tokio::test]
async fn model_catalog_http_errors_do_not_include_provider_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(401).set_body_string("provider-secret-response-body"))
        .mount(&server)
        .await;

    let error = fetch_model_catalog(&ModelRouteConfig::openai_compatible(
        "provider-secret",
        format!("{}/v1", server.uri()),
        "a",
    ))
    .await
    .expect_err("unauthorized catalog");
    assert!(error.to_string().contains("HTTP 401"));
    assert!(!error.to_string().contains("provider-secret-response-body"));
}
