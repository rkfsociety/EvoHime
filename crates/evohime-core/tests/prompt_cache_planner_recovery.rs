use evohime_core::prompt_cache_planner::*;
#[test]
fn secret_and_unbounded_keepalive_fail_closed() {
    let p = ProviderCacheProfile {
        profile_id: "p".into(),
        cache_supported: true,
        min_prefix_tokens: 1,
        max_keepalive_ms: 10,
    };
    let s = segment("s", "secret", true, 1, "policy", "secret").unwrap();
    assert!(build_plan(vec![s], &p, "ctx", "policy", 0).is_err());
    let s = segment("s", "safe", true, 1, "policy", "public").unwrap();
    assert!(build_plan(vec![s], &p, "ctx", "policy", 11).is_err());
}
