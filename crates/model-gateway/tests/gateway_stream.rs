use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_model_gateway::{
    mock_gateway, ChatStreamItem, ModelGateway, ModelGatewayConfig, ModelRouteConfig,
};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;

fn push_delta(output: &mut String, item: ChatStreamItem) {
    if let ChatStreamItem::Delta(text) = item {
        output.push_str(&text);
    }
}

#[tokio::test]
async fn gateway_streams_tokens_from_mock_provider() {
    let gateway = mock_gateway(vec!["Lite".into(), "Router".into()]);
    let mut stream = gateway.stream_chat(&[ChatMessage {
        role: ChatRole::User,
        content: "ping".to_string(),
    }]);

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
        .stream_chat_for_route(
            "planner",
            &[ChatMessage {
                role: ChatRole::User,
                content: "ping".to_string(),
            }],
        )
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
