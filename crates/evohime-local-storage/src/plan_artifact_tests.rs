use super::*;
fn artifact() -> PlanArtifactV1 {
    PlanArtifactV1 {
        schema_version: 1,
        id: "a".into(),
        revision: 1,
        version: 1,
        status: PlanArtifactStatus::Draft,
        title: "T".into(),
        objective: "O".into(),
        steps: vec![PlanStep {
            id: "s".into(),
            description: "do".into(),
            capability_ref: None,
            risk: "low".into(),
        }],
        assumptions: vec![],
        risks: vec![],
        acceptance_criteria: vec![AcceptanceCriterion {
            id: "c".into(),
            description: "pass".into(),
            evidence_kind: "TestsPass".into(),
            required: true,
        }],
        references: vec![],
        provenance: PlanProvenance {
            actor: "core".into(),
            request_id: "r".into(),
            correlation_id: "c".into(),
        },
        content_hash: String::new(),
    }
}
#[test]
fn seal_hash_and_transitions() {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE plan_artifact_revisions (artifact_id TEXT,revision INTEGER,version INTEGER,status TEXT,content_hash TEXT,artifact_json BLOB,idempotency_key TEXT,created_at_ms INTEGER,PRIMARY KEY(artifact_id,revision),UNIQUE(artifact_id,idempotency_key))").unwrap();
    let s = PlanArtifactStore::new(&db);
    let a = s.create(&artifact(), "i", 0).unwrap();
    assert!(!a.content_hash.is_empty());
    let b = s
        .transition("a", 1, PlanArtifactStatus::Accepted, "j", 1)
        .unwrap();
    assert_eq!(b.version, 2);
    assert!(matches!(
        s.transition("a", 1, PlanArtifactStatus::Executing, "k", 2),
        Err(PlanArtifactError::Stale { .. })
    ));
}
