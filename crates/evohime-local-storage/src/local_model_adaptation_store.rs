//! Persistence for metadata-only Core local-model adaptation jobs.

use rusqlite::{params, Connection, OptionalExtension, Result};

use crate::StorageError;

/// Revision, state, request hash, snapshot hash, immutable request, snapshot.
pub type StoredAdaptationJob = (u64, String, String, String, Vec<u8>, Vec<u8>);
/// Model identity/revision, path, artifact hash/size, phase and journal hash.
pub type StoredPublication = (String, u64, String, String, u64, String, String);
/// Job identity, current revision/state and serialized snapshot.
pub type StoredJobSummary = (String, u64, String, Vec<u8>);

/// Loads the persisted Agent Benchmark Matrix baseline by its stable ID.
pub fn get_benchmark_baseline(
    connection: &Connection,
    baseline_id: &str,
) -> Result<Option<crate::domains::evaluation::StoredBenchmarkBaseline>, StorageError> {
    crate::benchmark_store::get_baseline(connection, baseline_id)
}

/// Loads the frozen benchmark inputs captured before a matrix is dispatched.
pub fn get_benchmark_inputs(
    connection: &Connection,
    job_id: &str,
) -> Result<Option<(String, Vec<u8>)>> {
    connection.query_row(
        "SELECT input_hash,input_json FROM local_model_adaptation_benchmark_inputs WHERE job_id=?1",
        [job_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()
}

/// Returns bytes reserved by other nonterminal adaptation jobs.
pub fn active_disk_reservations(connection: &Connection, except_job_id: &str) -> Result<u64> {
    let bytes: i64 = connection.query_row(
        "SELECT COALESCE(SUM(r.reserved_bytes),0)
         FROM local_model_adaptation_disk_reservations r
         JOIN local_model_adaptation_jobs j ON j.job_id=r.job_id
         WHERE j.job_id<>?1 AND j.state NOT IN ('promoted','rejected','cancelled','failed','interrupted')",
        [except_job_id],
        |row| row.get(0),
    )?;
    u64::try_from(bytes).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, bytes))
}

/// Reserves disk capacity for a durable adaptation job before process dispatch.
pub fn reserve_disk(
    connection: &Connection,
    job_id: &str,
    reserved_bytes: u64,
    now_ms: i64,
) -> Result<bool> {
    let Ok(reserved_bytes) = i64::try_from(reserved_bytes) else {
        return Ok(false);
    };
    if reserved_bytes <= 0 {
        return Ok(false);
    }
    Ok(connection.execute(
        "INSERT INTO local_model_adaptation_disk_reservations(job_id,reserved_bytes,updated_at_ms)
         VALUES(?1,?2,?3)
         ON CONFLICT(job_id) DO UPDATE SET reserved_bytes=excluded.reserved_bytes,updated_at_ms=excluded.updated_at_ms
         WHERE local_model_adaptation_disk_reservations.reserved_bytes=excluded.reserved_bytes",
        params![job_id, reserved_bytes, now_ms],
    )? == 1)
}

/// Moves a still-waiting job to the end of the FIFO retry order.
pub fn defer_waiting_job(connection: &Connection, job_id: &str, now_ms: i64) -> Result<bool> {
    Ok(connection.execute(
        "UPDATE local_model_adaptation_jobs SET updated_at_ms=?2
         WHERE job_id=?1 AND state='waiting_for_resources'",
        params![job_id, now_ms],
    )? == 1)
}

/// Stores immutable benchmark inputs once, before the job is dispatched.
pub fn put_benchmark_inputs(
    connection: &Connection,
    job_id: &str,
    input_hash: &str,
    input_json: &[u8],
    now_ms: i64,
) -> Result<bool> {
    Ok(connection.execute(
        "INSERT OR IGNORE INTO local_model_adaptation_benchmark_inputs(job_id,input_hash,input_json,updated_at_ms) VALUES(?1,?2,?3,?4)",
        params![job_id, input_hash, input_json, now_ms],
    )? == 1)
}

/// Inserts revision one or compare-and-set advances exactly one job revision.
// The fields map one-to-one to the persisted CAS row and are intentionally explicit.
#[allow(clippy::too_many_arguments)]
pub fn put_job(
    connection: &Connection,
    job_id: &str,
    revision: u64,
    state: &str,
    idempotency_key: &str,
    request_hash: &str,
    content_hash: &str,
    request_json: &[u8],
    snapshot_json: &[u8],
    now_ms: i64,
) -> Result<bool> {
    if revision == 1 {
        return Ok(connection.execute(
            "INSERT OR IGNORE INTO local_model_adaptation_jobs
             (job_id,revision,state,idempotency_key,request_hash,content_hash,request_json,snapshot_json,created_at_ms,updated_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
            params![job_id, revision, state, idempotency_key, request_hash, content_hash, request_json, snapshot_json, now_ms],
        )? == 1);
    }
    Ok(connection.execute(
        "UPDATE local_model_adaptation_jobs SET revision=?2,state=?3,content_hash=?4,
         snapshot_json=?5,updated_at_ms=?6 WHERE job_id=?1 AND revision=?2-1 AND request_hash=?7",
        params![
            job_id,
            revision,
            state,
            content_hash,
            snapshot_json,
            now_ms,
            request_hash
        ],
    )? == 1)
}

/// Loads one job by its immutable idempotency key.
pub fn get_job_by_idempotency_key(
    connection: &Connection,
    idempotency_key: &str,
) -> Result<Option<StoredAdaptationJob>> {
    connection
        .query_row(
            "SELECT revision,state,request_hash,content_hash,request_json,snapshot_json
             FROM local_model_adaptation_jobs WHERE idempotency_key=?1",
            [idempotency_key],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
}

/// Loads one bounded adaptation job snapshot and its immutable request.
pub fn get_job(connection: &Connection, job_id: &str) -> Result<Option<StoredAdaptationJob>> {
    connection
        .query_row(
            "SELECT revision,state,request_hash,content_hash,request_json,snapshot_json
         FROM local_model_adaptation_jobs WHERE job_id=?1",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
}

/// Writes or compare-and-set updates the filesystem publication journal.
// The fields map one-to-one to the persisted publication journal row.
#[allow(clippy::too_many_arguments)]
pub fn put_publication(
    connection: &Connection,
    job_id: &str,
    model_id: &str,
    model_revision: u64,
    artifact_relative_path: &str,
    artifact_hash: &str,
    artifact_size_bytes: u64,
    registry_state: &str,
    content_hash: &str,
    now_ms: i64,
) -> Result<bool> {
    Ok(connection.execute(
        "INSERT INTO local_model_adaptation_publications
         (job_id,model_id,model_revision,artifact_relative_path,artifact_hash,artifact_size_bytes,registry_state,content_hash,updated_at_ms)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(job_id) DO UPDATE SET model_id=excluded.model_id,
         model_revision=excluded.model_revision,artifact_relative_path=excluded.artifact_relative_path,
         artifact_hash=excluded.artifact_hash,artifact_size_bytes=excluded.artifact_size_bytes,
         registry_state=excluded.registry_state,content_hash=excluded.content_hash,
         updated_at_ms=excluded.updated_at_ms
         WHERE local_model_adaptation_publications.content_hash=?10",
        params![job_id, model_id, model_revision, artifact_relative_path, artifact_hash,
            artifact_size_bytes, registry_state, content_hash, now_ms, content_hash],
    )? == 1)
}

/// Loads one filesystem publication journal entry by adaptation job ID.
pub fn get_publication(connection: &Connection, job_id: &str) -> Result<Option<StoredPublication>> {
    connection
        .query_row(
            "SELECT model_id,model_revision,artifact_relative_path,artifact_hash,
         artifact_size_bytes,registry_state,content_hash
         FROM local_model_adaptation_publications WHERE job_id=?1",
            [job_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
}

/// Lists jobs in oldest-update order for bounded restart reconciliation.
pub fn list_jobs(connection: &Connection, limit: u32) -> Result<Vec<StoredJobSummary>> {
    let mut statement = connection.prepare(
        "SELECT job_id,revision,state,snapshot_json FROM local_model_adaptation_jobs
         ORDER BY updated_at_ms,job_id LIMIT ?1",
    )?;
    let rows = statement.query_map([i64::from(limit.min(256))], |row| {
        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn schema() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        crate::benchmark_store::install_schema(&connection).unwrap();
        let transaction = connection.transaction().unwrap();
        crate::migrations::v178::apply(&transaction, 177).unwrap();
        transaction.commit().unwrap();
        connection
    }

    #[test]
    fn baseline_approval_is_durably_attributed_and_idempotently_retrievable() {
        let connection = schema();
        assert!(crate::benchmark_store::put_baseline(
            &connection,
            "baseline-1",
            "suite-v1",
            "challenge-1",
            "model-hash",
            "agent-hash",
            "{}",
            "source",
            1,
            10,
        )
        .unwrap());
        assert!(crate::benchmark_store::put_baseline_approval(
            &connection,
            "baseline-1",
            "benchmark-owner",
            "run-1",
            &"a".repeat(64),
            "approval-key",
            11,
        )
        .unwrap());
        assert_eq!(
            crate::benchmark_store::get_baseline_approval_by_key(&connection, "approval-key")
                .unwrap(),
            Some((
                "baseline-1".into(),
                "benchmark-owner".into(),
                "run-1".into(),
                "a".repeat(64)
            )),
        );
        assert_eq!(
            crate::benchmark_store::get_baseline_approval(&connection, "baseline-1").unwrap(),
            Some(("benchmark-owner".into(), "run-1".into(), "a".repeat(64))),
        );
        assert!(!crate::benchmark_store::put_baseline_approval(
            &connection,
            "baseline-1",
            "benchmark-owner",
            "run-1",
            &"a".repeat(64),
            "approval-key",
            12,
        )
        .unwrap());
    }

    #[test]
    fn job_revision_is_compare_and_set_and_publication_is_journaled() {
        let connection = schema();
        assert!(put_job(
            &connection,
            "job-a",
            1,
            "created",
            "idempotency-1",
            "request-hash",
            "hash",
            b"{}",
            b"{}",
            1
        )
        .unwrap());
        assert_eq!(
            get_job_by_idempotency_key(&connection, "idempotency-1")
                .unwrap()
                .unwrap()
                .1,
            "created"
        );
        assert!(!put_job(
            &connection,
            "job-b",
            1,
            "created",
            "idempotency-1",
            "another-request",
            "other-hash",
            b"{}",
            b"{}",
            2
        )
        .unwrap());
        assert!(!put_job(
            &connection,
            "job-a",
            3,
            "running",
            "idempotency-1",
            "request-hash",
            "hash",
            b"{}",
            b"{}",
            2
        )
        .unwrap());
        assert!(!put_job(
            &connection,
            "job-a",
            2,
            "running",
            "idempotency-1",
            "wrong-request",
            "hash",
            b"{}",
            b"{}",
            2
        )
        .unwrap());
        assert!(put_job(
            &connection,
            "job-a",
            2,
            "running",
            "idempotency-1",
            "request-hash",
            "hash-2",
            b"{}",
            b"{}",
            3
        )
        .unwrap());
        assert!(put_publication(
            &connection,
            "job-a",
            "model-a",
            1,
            "models/model-a.gguf",
            &"a".repeat(64),
            42,
            "staged",
            "publication-hash",
            4
        )
        .unwrap());
        assert!(!put_publication(
            &connection,
            "job-a",
            "model-a",
            1,
            "models/model-a.gguf",
            &"b".repeat(64),
            42,
            "published",
            "other-hash",
            5
        )
        .unwrap());
        let job = get_job(&connection, "job-a").unwrap().unwrap();
        assert_eq!(job.0, 2);
        assert_eq!(job.1, "running");
        assert_eq!(job.2, "request-hash");
    }

    #[test]
    fn benchmark_inputs_are_immutable_after_first_dispatch() {
        let connection = schema();
        assert!(put_job(
            &connection,
            "job-b",
            1,
            "created",
            "key-b",
            "request",
            "hash",
            b"{}",
            b"{}",
            1
        )
        .unwrap());
        assert!(
            put_benchmark_inputs(&connection, "job-b", "suite-hash", b"frozen-input", 2).unwrap()
        );
        assert!(
            !put_benchmark_inputs(&connection, "job-b", "different-hash", b"changed-input", 3)
                .unwrap()
        );
        assert_eq!(
            get_benchmark_inputs(&connection, "job-b").unwrap(),
            Some(("suite-hash".into(), b"frozen-input".to_vec()))
        );
    }

    #[test]
    fn disk_reservations_are_accounted_until_the_job_is_terminal() {
        let connection = schema();
        for id in ["job-a", "job-b"] {
            assert!(put_job(
                &connection,
                id,
                1,
                "created",
                id,
                "request",
                "hash",
                b"{}",
                b"{}",
                1
            )
            .unwrap());
        }
        assert!(reserve_disk(&connection, "job-a", 1024, 2).unwrap());
        assert!(reserve_disk(&connection, "job-b", 2048, 2).unwrap());
        assert_eq!(
            active_disk_reservations(&connection, "job-a").unwrap(),
            2048
        );
        assert_eq!(
            active_disk_reservations(&connection, "job-b").unwrap(),
            1024
        );
        assert!(!reserve_disk(&connection, "job-a", 4096, 3).unwrap());
        assert!(put_job(
            &connection,
            "job-a",
            2,
            "failed",
            "job-a",
            "request",
            "hash-2",
            b"{}",
            b"{}",
            4
        )
        .unwrap());
        assert_eq!(active_disk_reservations(&connection, "job-b").unwrap(), 0);
    }

    #[test]
    fn deferred_waiting_job_moves_behind_older_eligible_work() {
        let connection = schema();
        for (id, now) in [("job-a", 1), ("job-b", 2)] {
            assert!(put_job(
                &connection,
                id,
                1,
                "waiting_for_resources",
                id,
                "request",
                "hash",
                b"{}",
                b"{}",
                now
            )
            .unwrap());
        }
        assert!(defer_waiting_job(&connection, "job-a", 3).unwrap());
        let jobs = list_jobs(&connection, 10).unwrap();
        assert_eq!(jobs[0].0, "job-b");
        assert_eq!(jobs[1].0, "job-a");
    }
}
