use evohime_core::memory_views_and_adaptive_recall::*;

fn view() -> MemoryView {
    MemoryView {
        schema_version: 1,
        id: "view".into(),
        revision: 1,
        owner_scope: "agent".into(),
        scopes: vec![
            LogicalMemoryScope {
                id: "workspace".into(),
                parent_id: None,
                sensitivity: Sensitivity::Internal,
            },
            LogicalMemoryScope {
                id: "workspace/project".into(),
                parent_id: Some("workspace".into()),
                sensitivity: Sensitivity::Private,
            },
        ],
        root_scope_ids: vec!["workspace".into()],
        rights: MemoryViewRights {
            read: true,
            write: false,
        },
        max_depth: 4,
        max_results: 8,
    }
}

#[test]
fn view_contract_is_versioned_scoped_and_read_only() {
    let value = view();
    validate_view(&value).unwrap();
    assert_eq!(canonical_hash(&value).unwrap().len(), 64);
    assert!(authorize_read(&value, "workspace/project").is_ok());
    assert_eq!(
        authorize_write(&value, "workspace/project"),
        Err(MemoryViewError::WriteDenied)
    );
    assert_eq!(
        authorize_read(&value, "workspace/other"),
        Err(MemoryViewError::ScopeOutsideView)
    );
}

#[test]
fn recall_modes_are_bounded_and_explainable() {
    let decision = decide_recall(
        &view(),
        &AdaptiveRecallPolicy {
            schema_version: 1,
            shallow_depth: 1,
            deep_depth: 8,
            auto_composite_depth: 3,
            max_results: 16,
        },
        RecallMode::Auto,
        QueryComplexity::Composite,
        "multi hop",
        42,
    )
    .unwrap();
    assert_eq!(decision.effective_depth, 3);
    assert_eq!(decision.read_barrier_generation, 42);
    assert_eq!(
        decision.score_components,
        vec!["lexical", "freshness", "provenance"]
    );
}
