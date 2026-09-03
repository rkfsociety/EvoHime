use evohime_core::memory_views_and_adaptive_recall::*;

#[test]
fn retrieval_rejects_scope_escape_before_scoring() {
    let view = MemoryView {
        schema_version: 1,
        id: "view".into(),
        revision: 1,
        owner_scope: "agent".into(),
        scopes: vec![LogicalMemoryScope {
            id: "workspace".into(),
            parent_id: None,
            sensitivity: Sensitivity::Internal,
        }],
        root_scope_ids: vec!["workspace".into()],
        rights: MemoryViewRights {
            read: true,
            write: false,
        },
        max_depth: 2,
        max_results: 4,
    };
    let result = rank_candidates(
        &view,
        vec![RecallCandidate {
            record_id: "memory".into(),
            scope_id: "outside".into(),
            lexical_score: 1000,
            freshness_score: 1000,
            provenance_score: 1000,
        }],
    );
    assert_eq!(result, Err(MemoryViewError::ScopeOutsideView));
}
