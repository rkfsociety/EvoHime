use std::collections::HashMap;
use std::sync::Arc;

use evohime_model_gateway::providers::{ChatMessage, ChatRole};
use evohime_model_gateway::{mock_gateway, ModelGateway, ModelGatewayConfig, ModelRouteConfig};
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
        output.push_str(&chunk.expect("chunk ok"));
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
