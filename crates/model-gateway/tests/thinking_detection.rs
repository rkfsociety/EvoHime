//! Wave 3B provider thinking capability detection tests.

use evohime_model_gateway::providers::ProviderKind;
use evohime_model_gateway::ModelRouteConfig;

#[test]
fn literouter_supports_thinking() {
    assert!(ProviderKind::LiteRouter.supports_thinking());
}

#[test]
fn openai_compatible_no_thinking() {
    assert!(!ProviderKind::OpenAICompatible.supports_thinking());
}

#[test]
fn mock_supports_thinking() {
    assert!(ProviderKind::Mock.supports_thinking());
}

#[test]
fn model_route_literouter_config() {
    let route = ModelRouteConfig::literouter("key", "https://api.example.com", "claude-3");
    assert_eq!(route.provider, ProviderKind::LiteRouter);
    assert!(route.supports_thinking);
}

#[test]
fn model_route_openai_config() {
    let route = ModelRouteConfig::openai_compatible("key", "https://api.openai.com/v1", "gpt-4");
    assert_eq!(route.provider, ProviderKind::OpenAICompatible);
    assert!(!route.supports_thinking);
}

#[test]
fn model_route_mock_config() {
    let route = ModelRouteConfig::mock("test-model");
    assert_eq!(route.provider, ProviderKind::Mock);
    assert!(route.supports_thinking);
}

#[test]
fn provider_kind_as_str() {
    assert_eq!(ProviderKind::LiteRouter.as_str(), "literouter");
    assert_eq!(ProviderKind::OpenAICompatible.as_str(), "openai_compatible");
    assert_eq!(ProviderKind::Mock.as_str(), "mock");
}

#[test]
fn provider_kind_parse() {
    assert_eq!(
        ProviderKind::parse("literouter"),
        Some(ProviderKind::LiteRouter)
    );
    assert_eq!(
        ProviderKind::parse("lite-router"),
        Some(ProviderKind::LiteRouter)
    );
    assert_eq!(
        ProviderKind::parse("openai"),
        Some(ProviderKind::OpenAICompatible)
    );
    assert_eq!(ProviderKind::parse("mock"), Some(ProviderKind::Mock));
    assert_eq!(ProviderKind::parse("unknown"), None);
}
