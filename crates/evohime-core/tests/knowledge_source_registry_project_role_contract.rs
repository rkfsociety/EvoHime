use evohime_core::knowledge_source_registry_project_role as knowledge;

#[test]
fn ready_view_carries_only_authorized_sources_and_hit_provenance() {
    let policy = knowledge::default_policy();
    let source = knowledge::KnowledgeSource {
        schema_version: knowledge::SCHEMA_VERSION,
        id: "manual".into(),
        version: 3,
        kind: knowledge::SourceKind::TextDocument,
        display_name: "manual".into(),
        origin_ref: "workspace/manual.txt".into(),
        project_id: Some("p".into()),
        source_fingerprint: "fp".into(),
        sensitivity: knowledge::Sensitivity::Internal,
        trust_class: "repository_reference".into(),
        ingestion_profile_id: "text-v1".into(),
        status: knowledge::SourceStatus::Ready,
        created_by: "user".into(),
        created_at_ms: 1,
        last_indexed_at_ms: Some(2),
        content_hash: "hash".into(),
    };
    let binding = knowledge::KnowledgeBinding {
        source_id: "manual".into(),
        target_kind: knowledge::TargetKind::Project,
        target_id: "p".into(),
        access_mode: knowledge::AccessMode::ReadOnly,
        retrieval_profile_id: Some("keyword".into()),
        priority: 1,
    };
    let view = knowledge::build_view(knowledge::BuildViewInput {
        id: "view".into(),
        run_id: "run".into(),
        sources: &[source],
        bindings: &[binding],
        target_kind: knowledge::TargetKind::Project,
        target_id: "p",
        max_sensitivity: knowledge::Sensitivity::Internal,
        retrieval_profile: "keyword".into(),
        expires_at_ms: None,
        policy: &policy,
    })
    .unwrap();
    let hit = knowledge::KnowledgeHit {
        source_id: "manual".into(),
        source_revision: 3,
        chunk_id: "chunk-1".into(),
        locator: "manual.txt:1".into(),
        excerpt: "reference".into(),
        score: 1,
        match_reasons: vec!["keyword".into()],
        freshness: "current".into(),
        trust_class: "repository_reference".into(),
    };
    assert!(knowledge::validate_hit(&hit, &view, &policy).is_ok());
    assert_eq!(hit.source_revision, 3);
}

#[test]
fn collection_contract_contains_only_bounded_source_references() {
    let collection = knowledge::KnowledgeCollection {
        schema_version: knowledge::SCHEMA_VERSION,
        id: "docs".into(),
        version: 1,
        source_ids: vec!["manual".into()],
        retrieval_profile: "keyword".into(),
        scope: "project:p".into(),
        status: knowledge::CollectionStatus::Ready,
        content_hash: "collection-hash".into(),
    };
    assert!(knowledge::validate_collection(&collection, &knowledge::default_policy()).is_ok());
}
