use evohime_core::plan_artifact::{
    AcceptanceCriterion, PlanArtifactRuntime, PlanArtifactStatus, PlanArtifactV1, PlanProvenance,
    PlanStep,
};

fn artifact() -> PlanArtifactV1 {
    PlanArtifactV1 {
        schema_version: 1,
        id: "recovery-plan".into(),
        revision: 1,
        version: 1,
        status: PlanArtifactStatus::Draft,
        title: "Recovery plan".into(),
        objective: "bounded execution".into(),
        steps: vec![PlanStep {
            id: "s1".into(),
            description: "bounded step".into(),
            capability_ref: None,
            risk: "low".into(),
        }],
        assumptions: vec!["Core owns authority".into()],
        risks: vec!["external outcome".into()],
        acceptance_criteria: vec![AcceptanceCriterion {
            id: "c1".into(),
            description: "evidence".into(),
            evidence_kind: "TestsPass".into(),
            required: true,
        }],
        references: vec![],
        provenance: PlanProvenance {
            actor: "core".into(),
            request_id: "request".into(),
            correlation_id: "correlation".into(),
        },
        content_hash: String::new(),
    }
}

#[tokio::test]
async fn stale_and_duplicate_transitions_are_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = PlanArtifactRuntime::new(
        evohime_core::EventJournal::open(dir.path().join("recovery.db")).unwrap(),
    );
    let created = runtime.create(&artifact(), "create", 1).await.unwrap();
    let accepted = runtime
        .transition(&created.id, 1, PlanArtifactStatus::Accepted, "accept", 2)
        .await
        .unwrap();
    let duplicate = runtime
        .transition(&accepted.id, 2, PlanArtifactStatus::Executing, "execute", 3)
        .await
        .unwrap();
    assert_eq!(duplicate.status, PlanArtifactStatus::Executing);
    let stale = runtime
        .transition(
            &accepted.id,
            2,
            PlanArtifactStatus::Executing,
            "execute-again",
            4,
        )
        .await
        .unwrap_err();
    assert!(stale.to_string().contains("stale"));
}

#[tokio::test]
async fn uncertain_dispatch_is_a_terminal_visible_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = PlanArtifactRuntime::new(
        evohime_core::EventJournal::open(dir.path().join("unknown.db")).unwrap(),
    );
    let created = runtime.create(&artifact(), "create", 1).await.unwrap();
    let accepted = runtime
        .transition(&created.id, 1, PlanArtifactStatus::Accepted, "accept", 2)
        .await
        .unwrap();
    let running = runtime
        .execute(
            &accepted.id,
            2,
            "policy",
            Some("task"),
            None,
            "correlation",
            "execute",
            3,
        )
        .await
        .unwrap();
    assert_eq!(running.revision, 3);
    let unknown = runtime
        .transition(
            &created.id,
            3,
            PlanArtifactStatus::UnknownOutcome,
            "unknown",
            4,
        )
        .await
        .unwrap();
    assert_eq!(unknown.status, PlanArtifactStatus::UnknownOutcome);
    assert!(runtime
        .transition(
            &created.id,
            4,
            PlanArtifactStatus::Executing,
            "blind-retry",
            5
        )
        .await
        .is_err());
}
