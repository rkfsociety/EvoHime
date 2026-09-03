use evohime_core::policy_aware_tool_result_cache as cache;

#[test]
fn mutating_manifest_defaults_to_never_and_expired_entry_is_not_reused() {
    let metadata = cache::metadata(
        "filesystem.write".into(),
        "1".into(),
        cache::Cacheability::Never,
    )
    .unwrap();
    assert!(cache::cache_key(&metadata, "input", "workspace", "account", "policy").is_err());
    let metadata = cache::metadata(
        "filesystem.read".into(),
        "1".into(),
        cache::Cacheability::ReadOnly,
    )
    .unwrap();
    let key = cache::cache_key(&metadata, "input", "workspace", "account", "policy").unwrap();
    let entry = cache::CacheEntry {
        schema_version: 1,
        key,
        tool_id: "filesystem.read".into(),
        tool_version: "1".into(),
        resource_scope: "workspace".into(),
        authority_scope: "account".into(),
        policy_hash: "policy".into(),
        result_ref: "artifact:read-1".into(),
        observed_at_ms: 1,
        expires_at_ms: 5,
        provenance_ref: "event:read-1".into(),
        status: cache::CacheStatus::Fresh,
    };
    assert!(cache::validate_entry(
        &entry,
        &cache::default_policy(),
        6,
        cache::Freshness::UseCache
    )
    .is_err());
}

#[test]
fn eviction_removes_expired_entries_and_keeps_only_bounded_recent_data() {
    let mut entries = (0..3)
        .map(|i| cache::CacheEntry {
            schema_version: 1,
            key: format!("{i:064x}"),
            tool_id: "read".into(),
            tool_version: "1".into(),
            resource_scope: "workspace".into(),
            authority_scope: "account".into(),
            policy_hash: "policy".into(),
            result_ref: format!("artifact:{i}"),
            observed_at_ms: i,
            expires_at_ms: 100,
            provenance_ref: format!("event:{i}"),
            status: cache::CacheStatus::Fresh,
        })
        .collect();
    let mut policy = cache::default_policy();
    policy.max_entries = 2;
    cache::evict(&mut entries, &policy, 1);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].observed_at_ms, 1);
}
