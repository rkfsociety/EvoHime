// Integration test for distributed worker queue (Stage 7.54).
// Tests that multiple workers can independently claim and complete jobs.

use evohime_storage::{
    claim_next_queued_worker_job, complete_worker_job_claimed, create_worker_job, load_worker_job,
    update_worker_job_heartbeat,
};
use serde_json::json;

#[tokio::test]
#[ignore] // Requires DATABASE_URL
async fn distributed_workers_claim_and_complete_jobs() {
    let pool = evohime_storage::connect_integration_pool()
        .await
        .expect("failed to connect to test database");

    // Submit 3 jobs
    let job1 = create_worker_job(&pool, "echo", &json!({}))
        .await
        .expect("failed to create job 1");
    let job2 = create_worker_job(&pool, "echo", &json!({}))
        .await
        .expect("failed to create job 2");
    let job3 = create_worker_job(&pool, "echo", &json!({}))
        .await
        .expect("failed to create job 3");

    // Worker 1 claims job 1
    let (claimed1, token1) = claim_next_queued_worker_job(&pool)
        .await
        .expect("worker 1 claim failed")
        .expect("worker 1 should get a job");
    assert_eq!(claimed1.id, job1.id);
    assert_eq!(claimed1.status, "running");
    assert!(claimed1.claim_token.is_some());
    assert!(claimed1.claimed_at.is_some());
    assert!(claimed1.heartbeat_at.is_some());

    // Worker 2 claims job 2 (not job 1 again due to FOR UPDATE)
    let (claimed2, token2) = claim_next_queued_worker_job(&pool)
        .await
        .expect("worker 2 claim failed")
        .expect("worker 2 should get a job");
    assert_eq!(claimed2.id, job2.id);
    assert_ne!(token1, token2);

    // Worker 1 sends heartbeat
    let hb1 = update_worker_job_heartbeat(&pool, job1.id, token1)
        .await
        .expect("heartbeat 1 failed");
    assert!(hb1, "heartbeat 1 should succeed with correct claim token");

    // Stale claim fails heartbeat
    let stale_hb = update_worker_job_heartbeat(&pool, job1.id, token2)
        .await
        .expect("stale heartbeat should not error");
    assert!(!stale_hb, "stale claim should fail heartbeat");

    // Worker 1 completes job 1
    let completed1 = complete_worker_job_claimed(
        &pool,
        job1.id,
        token1,
        "completed",
        Some(&json!({"result": "ok"})),
        None,
    )
    .await
    .expect("complete 1 failed")
    .expect("complete 1 should succeed");
    assert_eq!(completed1.status, "completed");

    // Stale worker can't complete again
    let stale_complete =
        complete_worker_job_claimed(&pool, job1.id, token2, "completed", Some(&json!({})), None)
            .await
            .expect("stale complete should not error");
    assert!(
        stale_complete.is_none(),
        "stale claim should not complete already-completed job"
    );

    // Worker 2 completes job 2
    complete_worker_job_claimed(
        &pool,
        job2.id,
        token2,
        "completed",
        Some(&json!({"result": "ok"})),
        None,
    )
    .await
    .expect("complete 2 failed")
    .expect("complete 2 should succeed");

    // Worker 3 claims job 3
    let (claimed3, token3) = claim_next_queued_worker_job(&pool)
        .await
        .expect("worker 3 claim failed")
        .expect("worker 3 should get a job");
    assert_eq!(claimed3.id, job3.id);

    // Worker 3 fails job 3
    complete_worker_job_claimed(
        &pool,
        job3.id,
        token3,
        "failed",
        None,
        Some("intentional test failure"),
    )
    .await
    .expect("complete 3 failed")
    .expect("complete 3 should succeed");

    // Verify all jobs terminal
    for job_id in [job1.id, job2.id, job3.id] {
        let final_job = load_worker_job(&pool, job_id)
            .await
            .expect("load failed")
            .expect("job should exist");
        assert!(
            matches!(final_job.status.as_str(), "completed" | "failed"),
            "job {} status should be terminal, got {}",
            job_id,
            final_job.status
        );
        assert!(
            final_job.completed_at.is_some(),
            "job {} should have completed_at set",
            job_id
        );
    }

    // No more jobs to claim
    let no_more = claim_next_queued_worker_job(&pool)
        .await
        .expect("should not error");
    assert!(no_more.is_none(), "should have no more queued jobs");
}
