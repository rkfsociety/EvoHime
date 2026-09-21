use super::*;

#[test]
fn revision_insert_is_idempotent_and_artifact_is_immutable() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE TABLE knowledge_collections (collection_id TEXT PRIMARY KEY);")
        .unwrap();
    GroundedResearchStore::install_schema(&connection).unwrap();
    let record = ResearchRevisionRecord {
        revision_id: "revision-1".into(),
        workspace_id: "workspace-1".into(),
        source_id: "source-1".into(),
        revision: 1,
        content_hash: "a".repeat(64),
        origin_snapshot: "{}".into(),
        parser_version: "parser/v1".into(),
        index_profile: "index/v1".into(),
        status: "ready".into(),
        trust: "workspace".into(),
        locator_root: "file:README.md".into(),
    };
    assert!(GroundedResearchStore::insert_revision(&connection, &record).unwrap());
    assert!(!GroundedResearchStore::insert_revision(&connection, &record).unwrap());
    connection
            .execute(
                "INSERT INTO research_artifacts
                 (artifact_id,revision,session_id,content_hash,coverage,claims_json,citations_json,created_at_ms)
                 VALUES ('a',1,'s',?1,'partial','[]','[]',1)",
                ["b".repeat(64)],
            )
            .unwrap();
    assert!(connection
        .execute("UPDATE research_artifacts SET coverage='complete'", [])
        .is_err());
}
