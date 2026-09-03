use evohime_core::model_purpose_routing::{
    self, ContextPolicy, ModelCallPurpose, RoutingError, ToolCeiling,
};

#[test]
fn builtin_policy_is_versioned_and_has_all_purposes() {
    let policy = model_purpose_routing::builtin_policy();
    assert_eq!(policy.routes.len(), 13);
    assert_eq!(
        policy.route(ModelCallPurpose::Review).unwrap().profile_ref,
        "default"
    );
    assert_eq!(policy.canonical_hash().unwrap().len(), 64);
}

#[test]
fn unsafe_tool_ceiling_combination_fails_closed() {
    let mut policy = model_purpose_routing::builtin_policy();
    let route = policy.routes.get_mut(&ModelCallPurpose::Review).unwrap();
    route.requirements.tool_ceiling = ToolCeiling::NoTools;
    route.requirements.context_policy = ContextPolicy::Full;
    assert_eq!(
        policy.validate(),
        Err(RoutingError::Invalid("no-tools context"))
    );
}
