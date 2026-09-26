use super::*;
use crate::LocalDatabase;

fn database(name: &str) -> LocalDatabase {
    let path = std::env::temp_dir().join(format!(
        "evohime-artifact-{name}-{}-{:?}.db",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    LocalDatabase::open(&path).expect("database opens")
}

const KIND: &str = "tool_result";

#[test]
fn binary_artifact_round_trip_never_uses_a_host_path() {
    let database = database("binary");
    let store = ArtifactStore::new(database.connection());
    let bytes = [0u8, 1, 2, 255];
    let result = store
        .offload_bytes(
            "browser_screenshot",
            "task",
            "task",
            &bytes,
            Privacy::Workspace,
            1_000,
        )
        .expect("binary offload succeeds");
    assert!(result.reference.locator.starts_with("artifact://"));
    assert_eq!(
        store
            .read_bytes(
                &result.reference.locator,
                "task",
                &[],
                "browser_screenshot",
                2_000
            )
            .unwrap(),
        bytes
    );
    assert!(!result.reference.locator.contains(":\\"));
}

#[test]
fn bounded_binary_read_rejects_reference_and_actual_oversize() {
    let database = database("binary-bounded-read");
    let store = ArtifactStore::new(database.connection());
    let bytes = [0u8, 1, 2, 255];
    let result = store
        .offload_bytes(
            "image/png",
            "task",
            "task",
            &bytes,
            Privacy::Workspace,
            1_000,
        )
        .expect("binary offload succeeds");

    assert!(store
        .read_bytes_bounded(
            &result.reference.locator,
            "task",
            &[],
            "image/png",
            2_000,
            3
        )
        .is_err());
    database
        .connection()
        .execute(
            "UPDATE task_artifact_refs SET bytes=1 WHERE locator=?1",
            [&result.reference.locator],
        )
        .expect("simulate corrupted underreported reference");
    assert!(store
        .read_bytes_bounded(
            &result.reference.locator,
            "task",
            &[],
            "image/png",
            2_000,
            3
        )
        .is_err());
}

#[test]
fn binary_artifact_batch_publishes_all_refs_or_respects_aggregate_quota() {
    let database = database("binary-batch");
    let items = [
        BinaryArtifactInput {
            kind: "generated_image",
            task_id: "image-task",
            owner_task_id: "image-task",
            content: &[1, 2, 3],
            privacy: Privacy::Workspace,
        },
        BinaryArtifactInput {
            kind: "generated_image",
            task_id: "image-task",
            owner_task_id: "image-task",
            content: &[4, 5],
            privacy: Privacy::Workspace,
        },
    ];
    let restrictive = ArtifactStore::with_quota(
        database.connection(),
        ArtifactQuota {
            per_task_bytes: 4,
            total_bytes: 4,
            default_ttl_ms: 1_000,
        },
    );
    assert!(restrictive.offload_bytes_batch(&items, 1_000).is_err());
    assert_eq!(
        restrictive.total_bytes().expect("no partial blob writes"),
        0
    );
    assert!(restrictive
        .list_refs("image-task")
        .expect("no partial refs")
        .is_empty());

    let store = ArtifactStore::new(database.connection());
    let references = store
        .offload_bytes_batch(&items, 1_100)
        .expect("batch publishes");
    assert_eq!(references.len(), 2);
    for (reference, expected) in references.iter().zip([&[1, 2, 3][..], &[4, 5][..]]) {
        assert_eq!(
            store
                .read_bytes_bounded(
                    &reference.locator,
                    "image-task",
                    &[],
                    "generated_image",
                    1_200,
                    3,
                )
                .expect("bounded read")
                .as_slice(),
            expected
        );
    }
}

#[test]
fn a_large_output_is_stored_and_summarized_for_the_context() {
    let database = database("offload");
    let store = ArtifactStore::new(database.connection());
    let content = (1..=50)
        .map(|index| format!("строка {index}"))
        .collect::<Vec<_>>()
        .join("\n");
    let result = store
        .offload(KIND, "task", "task", &content, Privacy::Workspace, 1_000)
        .expect("offload succeeds");
    assert!(!result.deduplicated);
    assert!(result.reference.summary.contains("ещё"));
    assert!(result.reference.summary.chars().count() <= ARTIFACT_SUMMARY_CHARS + 64);
    assert_eq!(result.reference.bytes, content.len() as u64);
    let read = store
        .read(&result.reference.locator, "task", &[], KIND, 2_000)
        .expect("read succeeds");
    assert_eq!(read, content);
}

#[test]
fn repeated_offload_of_the_same_content_reuses_the_artifact() {
    let database = database("dedup");
    let store = ArtifactStore::new(database.connection());
    let first = store
        .offload(
            KIND,
            "task-a",
            "task-a",
            "одно и то же",
            Privacy::Workspace,
            1_000,
        )
        .expect("offload succeeds");
    let second = store
        .offload(
            KIND,
            "task-b",
            "task-b",
            "одно и то же",
            Privacy::Workspace,
            2_000,
        )
        .expect("offload succeeds");
    assert!(!first.deduplicated);
    assert!(second.deduplicated);
    assert_eq!(first.reference.content_hash, second.reference.content_hash);
    assert_ne!(first.reference.locator, second.reference.locator);
    let stored: i64 = database
        .connection()
        .query_row("SELECT COUNT(*) FROM task_artifacts", [], |row| row.get(0))
        .expect("count");
    assert_eq!(stored, 1, "содержимое хранится один раз, ссылок — две");
}

#[test]
fn privacy_labels_forbid_offload() {
    let database = database("privacy");
    let store = ArtifactStore::new(database.connection());
    for privacy in [Privacy::Sensitive, Privacy::Secret] {
        assert!(store
            .offload(KIND, "task", "task", "секрет", privacy, 1_000)
            .is_err());
    }
}

#[test]
fn locator_access_is_limited_to_the_owner_and_its_children() {
    let database = database("access");
    let store = ArtifactStore::new(database.connection());
    let result = store
        .offload(
            KIND,
            "parent",
            "parent",
            "содержимое",
            Privacy::Workspace,
            1_000,
        )
        .expect("offload succeeds");
    assert!(store
        .read(&result.reference.locator, "parent", &[], KIND, 2_000)
        .is_ok());
    assert!(store
        .read(
            &result.reference.locator,
            "child",
            &["parent".to_string()],
            KIND,
            2_000
        )
        .is_ok());
    assert!(store
        .read(&result.reference.locator, "stranger", &[], KIND, 2_000)
        .is_err());
}

#[test]
fn a_corrupted_artifact_is_marked_invalid_and_never_enters_the_context() {
    let database = database("corruption");
    let store = ArtifactStore::new(database.connection());
    let result = store
        .offload(
            KIND,
            "task",
            "task",
            "исходное содержимое",
            Privacy::Workspace,
            1_000,
        )
        .expect("offload succeeds");
    // Подмена содержимого мимо store.
    database
        .connection()
        .execute(
            "UPDATE task_artifacts SET content = ?2 WHERE content_hash = ?1",
            rusqlite::params![result.reference.content_hash, "подменённое".as_bytes()],
        )
        .expect("tampering succeeds");

    let error = store
        .read(&result.reference.locator, "task", &[], KIND, 2_000)
        .expect_err("hash check fails");
    assert!(error.to_string().contains("hash check"));
    assert_eq!(
        store
            .get_ref(&result.reference.locator)
            .expect("read")
            .expect("ref")
            .status,
        ArtifactRefStatus::Invalid
    );
}

#[test]
fn quota_overflow_evicts_by_ttl_and_last_access_without_losing_referenced_links() {
    let database = database("quota");
    let quota = ArtifactQuota {
        per_task_bytes: 200,
        total_bytes: 200,
        default_ttl_ms: 1_000,
    };
    let store = ArtifactStore::with_quota(database.connection(), quota);
    let first = store
        .offload(
            KIND,
            "task",
            "task",
            &"a".repeat(90),
            Privacy::Workspace,
            1_000,
        )
        .expect("offload succeeds");
    // Ссылка из confirmed scratchpad: удалять содержимое молча нельзя.
    database
        .connection()
        .execute(
            "INSERT INTO task_scratchpad (
                    id, task_id, session_id, category, status, trust, privacy, revision,
                    parent_id, content, content_hash, created_at, updated_at, ttl_ms,
                    confirmation, artifact_locator, recovered_at_step
                 ) VALUES ('s1','task','session','facts','confirmed','confirmed','workspace',1,
                    NULL,'заметка','hash',1000,1000,NULL,'user_confirmed',?1,NULL)",
            [&first.reference.locator],
        )
        .expect("scratchpad link inserted");

    store
        .offload(
            KIND,
            "task",
            "task",
            &"b".repeat(90),
            Privacy::Workspace,
            2_000,
        )
        .expect("offload succeeds");
    // Третья выгрузка не помещается: сработает вытеснение.
    store
        .offload(
            KIND,
            "task",
            "task",
            &"c".repeat(90),
            Privacy::Workspace,
            5_000,
        )
        .expect("offload succeeds after eviction");

    let referenced = store
        .get_ref(&first.reference.locator)
        .expect("read")
        .expect("ref still exists");
    assert_eq!(referenced.status, ArtifactRefStatus::Expired);
    assert_eq!(referenced.bytes, 90, "размер сохраняется после вытеснения");
    assert_eq!(referenced.content_hash, first.reference.content_hash);
}

#[test]
fn tombstoned_content_is_not_reused_as_a_dedup_hit() {
    let database = database("tombstone");
    let store = ArtifactStore::new(database.connection());
    let first = store
        .offload(
            KIND,
            "task",
            "task",
            "содержимое",
            Privacy::Workspace,
            1_000,
        )
        .expect("offload succeeds");
    store
        .forget_task_artifacts("task", 2_000, "forget memory")
        .expect("cascade delete succeeds");
    let tombstone = store
        .tombstone(&first.reference.content_hash)
        .expect("read")
        .expect("tombstone exists");
    assert_eq!(tombstone.bytes, first.reference.bytes);

    let second = store
        .offload(
            KIND,
            "task",
            "task",
            "содержимое",
            Privacy::Workspace,
            3_000,
        )
        .expect("offload succeeds");
    assert!(
        !second.deduplicated,
        "tombstone не считается доступным dedup-hit"
    );
}

#[test]
fn cascade_delete_removes_refs_and_content_without_live_links() {
    let database = database("cascade");
    let store = ArtifactStore::new(database.connection());
    store
        .offload(KIND, "task", "task", "первое", Privacy::Workspace, 1_000)
        .expect("offload succeeds");
    store
        .offload(KIND, "task", "task", "второе", Privacy::Workspace, 1_100)
        .expect("offload succeeds");
    assert_eq!(
        store
            .forget_task_artifacts("task", 2_000, "forget memory")
            .expect("cascade"),
        2
    );
    assert!(store.list_refs("task").expect("list").is_empty());
    assert_eq!(store.total_bytes().expect("bytes"), 0);
}

#[test]
fn an_expired_reference_is_not_readable_but_keeps_its_hash() {
    let database = database("expired-read");
    let store = ArtifactStore::new(database.connection());
    let result = store
        .offload(
            KIND,
            "task",
            "task",
            "содержимое",
            Privacy::Workspace,
            1_000,
        )
        .expect("offload succeeds");
    store
        .set_ref_status(&result.reference.locator, ArtifactRefStatus::Expired)
        .expect("status updated");
    assert!(store
        .read(&result.reference.locator, "task", &[], KIND, 2_000)
        .is_err());
    let reference = store
        .get_ref(&result.reference.locator)
        .expect("read")
        .expect("ref");
    assert_eq!(reference.content_hash, result.reference.content_hash);
}

#[test]
fn concurrent_offload_of_identical_content_yields_one_artifact_and_two_refs() {
    let database = database("concurrent");
    let path = database.path().to_path_buf();
    drop(database);
    let handles: Vec<_> = ["task-a", "task-b"]
        .into_iter()
        .map(|task| {
            let path = path.clone();
            std::thread::spawn(move || {
                let database = LocalDatabase::open(&path).expect("database opens");
                database
                    .connection()
                    .busy_timeout(std::time::Duration::from_millis(5_000))
                    .expect("timeout set");
                let store = ArtifactStore::new(database.connection());
                store
                    .offload(
                        KIND,
                        task,
                        task,
                        "общее содержимое",
                        Privacy::Workspace,
                        1_000,
                    )
                    .expect("offload succeeds");
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread completes");
    }
    let database = LocalDatabase::open(&path).expect("database opens");
    let artifacts: i64 = database
        .connection()
        .query_row("SELECT COUNT(*) FROM task_artifacts", [], |row| row.get(0))
        .expect("count");
    let refs: i64 = database
        .connection()
        .query_row("SELECT COUNT(*) FROM task_artifact_refs", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(artifacts, 1);
    assert_eq!(refs, 2);
}
