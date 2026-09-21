use super::*;
use crate::LocalDatabase;

fn database(name: &str) -> LocalDatabase {
    let path = std::env::temp_dir().join(format!(
        "evohime-scratchpad-{name}-{}-{:?}.db",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    LocalDatabase::open(&path).expect("database opens")
}

fn draft(id: &str, category: ScratchpadCategory) -> ScratchpadEntry {
    ScratchpadEntry::draft(
        id,
        "task",
        "session",
        category,
        format!("заметка {id}"),
        1_000,
    )
}

#[test]
fn an_entry_round_trips_through_sqlite() {
    let database = database("round-trip");
    let store = ScratchpadStore::new(database.connection());
    let entry = draft("s1", ScratchpadCategory::Facts);
    store.upsert(&entry).expect("write succeeds");
    assert_eq!(store.get("s1").expect("read").expect("entry"), entry);
}

#[test]
fn confirmation_requires_an_explicit_basis_and_is_persisted() {
    let database = database("confirm");
    let store = ScratchpadStore::new(database.connection());
    store
        .upsert(&draft("s1", ScratchpadCategory::Facts))
        .expect("write succeeds");
    let confirmed = store
        .confirm("s1", ConfirmationBasis::ToolProvenanceVerified, 2_000)
        .expect("confirm succeeds");
    assert_eq!(confirmed.status, ScratchpadStatus::Confirmed);
    assert_eq!(
        store.get("s1").expect("read").expect("entry").confirmation,
        Some(ConfirmationBasis::ToolProvenanceVerified)
    );
}

#[test]
fn a_confirmed_entry_cannot_be_overwritten_in_place() {
    let database = database("no-overwrite");
    let store = ScratchpadStore::new(database.connection());
    store
        .upsert(&draft("s1", ScratchpadCategory::Facts))
        .expect("write succeeds");
    let confirmed = store
        .confirm("s1", ConfirmationBasis::UserConfirmed, 2_000)
        .expect("confirm succeeds");

    let mut silent_override = confirmed.clone();
    silent_override.content = "подменённое содержимое".to_string();
    silent_override.content_hash = "other-hash".to_string();
    assert!(store.upsert(&silent_override).is_err());

    // Новая ревизия — допустимый путь.
    let revision = confirmed.revise("s2", "новое содержимое", 3_000);
    store.upsert(&revision).expect("revision is accepted");
    assert_eq!(store.get("s1").expect("read").expect("entry"), confirmed);
    assert_eq!(store.get("s2").expect("read").expect("entry").revision, 2);
}

#[test]
fn only_confirmed_entries_return_after_restart() {
    let database = database("restart");
    let store = ScratchpadStore::new(database.connection());
    store
        .upsert(&draft("draft", ScratchpadCategory::Facts))
        .expect("write");
    store
        .upsert(&draft("confirmed", ScratchpadCategory::Facts))
        .expect("write");
    store
        .confirm("confirmed", ConfirmationBasis::UserConfirmed, 1_500)
        .expect("confirm");
    let mut recovered = draft("recovered", ScratchpadCategory::Facts);
    recovered.status = ScratchpadStatus::Recovered;
    store.upsert(&recovered).expect("write");

    let (restored, isolated) = store.recover("task", 9_000, 4).expect("recovery runs");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].id, "confirmed");
    assert_eq!(isolated.len(), 1);
    assert_eq!(isolated[0].trust, Trust::Unverified);
    assert_eq!(isolated[0].recovered_at_step, Some(4));
    // Черновик не восстанавливается и не остаётся в хранилище.
    assert!(store.get("draft").expect("read").is_none());
}

#[test]
fn unconfirmed_entries_become_recovered_on_shutdown() {
    let database = database("shutdown");
    let store = ScratchpadStore::new(database.connection());
    store
        .upsert(&draft("s1", ScratchpadCategory::Facts))
        .expect("write");
    let marked = store
        .mark_unconfirmed_as_recovered("task", 5_000, 7)
        .expect("marking runs");
    assert_eq!(marked, 1);
    let entry = store.get("s1").expect("read").expect("entry");
    assert_eq!(entry.status, ScratchpadStatus::Recovered);
    assert_eq!(entry.trust, Trust::Unverified);
    assert_eq!(entry.recovered_at_step, Some(7));
}

#[test]
fn expired_recovered_entries_are_discarded_by_policy() {
    let database = database("recovery-policy");
    let store = ScratchpadStore::new(database.connection());
    let mut recovered = draft("s1", ScratchpadCategory::Facts);
    recovered.status = ScratchpadStatus::Recovered;
    recovered.updated_at = 0;
    recovered.recovered_at_step = Some(0);
    store.upsert(&recovered).expect("write");

    let policy = RecoveryPolicy::default();
    assert_eq!(
        store
            .discard_expired_recovered("task", policy, 1_000, 1)
            .expect("policy runs"),
        0
    );
    assert_eq!(
        store
            .discard_expired_recovered("task", policy, policy.max_age_ms, 1)
            .expect("policy runs"),
        1
    );
}

#[test]
fn listing_filters_by_category_and_status() {
    let database = database("filter");
    let store = ScratchpadStore::new(database.connection());
    store
        .upsert(&draft("fact", ScratchpadCategory::Facts))
        .expect("write");
    store
        .upsert(&draft("question", ScratchpadCategory::OpenQuestions))
        .expect("write");
    store
        .confirm("fact", ConfirmationBasis::UserConfirmed, 2_000)
        .expect("confirm");

    let facts = store
        .list("task", Some(ScratchpadCategory::Facts), None, 50)
        .expect("list");
    assert_eq!(facts.len(), 1);
    let confirmed = store
        .list("task", None, Some(ScratchpadStatus::Confirmed), 50)
        .expect("list");
    assert_eq!(confirmed.len(), 1);
    assert_eq!(confirmed[0].id, "fact");
}

#[test]
fn projection_is_bounded_and_marks_truncation() {
    let database = database("projection");
    let store = ScratchpadStore::new(database.connection());
    let mut long = draft("s1", ScratchpadCategory::Facts);
    long.content = "я".repeat(500);
    store.upsert(&long).expect("write");
    let projection = store
        .projection("task", None, None, 50, 40)
        .expect("projection");
    assert_eq!(projection[0].preview.chars().count(), 40);
    assert!(projection[0].truncated);
}

#[test]
fn forgetting_an_entry_removes_its_revisions_too() {
    let database = database("forget");
    let store = ScratchpadStore::new(database.connection());
    let mut base = draft("s1", ScratchpadCategory::Facts);
    base.confirm(ConfirmationBasis::UserConfirmed, 1_500);
    store.upsert(&base).expect("write");
    store
        .upsert(&base.revise("s2", "новая ревизия", 2_000))
        .expect("write");
    assert_eq!(store.forget("s1").expect("forget"), 2);
    assert!(store.get("s2").expect("read").is_none());
}

#[test]
fn clearing_a_task_removes_only_that_task() {
    let database = database("clear");
    let store = ScratchpadStore::new(database.connection());
    store
        .upsert(&draft("s1", ScratchpadCategory::Facts))
        .expect("write");
    let mut other = draft("s2", ScratchpadCategory::Facts);
    other.task_id = "other-task".to_string();
    store.upsert(&other).expect("write");
    assert_eq!(store.clear_task("task").expect("clear"), 1);
    assert!(store.get("s2").expect("read").is_some());
}

#[test]
fn open_questions_are_never_offload_candidates() {
    let database = database("offload-candidates");
    let store = ScratchpadStore::new(database.connection());
    for (id, category) in [
        ("fact", ScratchpadCategory::Facts),
        ("question", ScratchpadCategory::OpenQuestions),
    ] {
        store.upsert(&draft(id, category)).expect("write");
        store
            .confirm(id, ConfirmationBasis::UserConfirmed, 2_000)
            .expect("confirm");
    }
    let candidates = store.offload_candidates("task", 10).expect("candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, "fact");
}
