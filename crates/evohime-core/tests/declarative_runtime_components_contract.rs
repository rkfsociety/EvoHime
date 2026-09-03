use evohime_core::declarative_agent_component_registry::{
    ComponentProvider, ComponentType, Registry, TrustStatus,
};
use evohime_core::declarative_runtime_components::*;
use std::collections::{BTreeMap, BTreeSet};

fn fixture() -> (ComponentConfig, Registry) {
    let mut providers = BTreeMap::new();
    providers.insert(
        "agent-provider".into(),
        ComponentProvider {
            provider_id: "agent-provider".into(),
            component_type: ComponentType::AgentRole,
            current_version: 1,
            trust: TrustStatus::BuiltInTrusted,
            schema_hash: "schema".into(),
        },
    );
    let registry = Registry {
        schema_version: 1,
        revision: 1,
        providers,
        components: BTreeMap::new(),
        content_hash: "registry".into(),
    };
    let mut config = ComponentConfig {
        schema_version: 1,
        component_id: "component-1".into(),
        component_type: ComponentType::AgentRole,
        provider_id: "agent-provider".into(),
        provider_version: 1,
        definition_config: serde_json::json!({"mode":"bounded"}),
        runtime_state: RuntimeState::Defined,
        secret_bindings: vec![SecretBinding {
            name: "provider_key".into(),
            credential_ref: "cred-ref".into(),
        }],
        capability_refs: vec!["memory.read".into()],
        policy_version: "policy-1".into(),
        provenance_ref: "event-1".into(),
        revision: 1,
        content_hash: String::new(),
    };
    config.content_hash = canonical_hash(&config).unwrap();
    (config, registry)
}

#[test]
fn valid_config_is_rehydrated_only_against_current_policy() {
    let (config, registry) = fixture();
    let policy = PolicySnapshot {
        policy_version: "policy-1".into(),
        allowed_capabilities: ["memory.read".into()].into_iter().collect(),
    };
    assert!(rehydrate(&config, &registry, &policy).is_ok());
    let denied = PolicySnapshot {
        policy_version: "policy-2".into(),
        allowed_capabilities: BTreeSet::new(),
    };
    assert_eq!(
        rehydrate(&config, &registry, &denied),
        Err(ComponentError::PolicyDenied)
    );
}
