use super::*;
use crate::LocalDatabase;

fn database(name: &str) -> LocalDatabase {
    let path = std::env::temp_dir().join(format!(
        "evohime-context-command-{name}-{}-{:?}.db",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    LocalDatabase::open(&path).expect("database opens")
}

#[test]
fn pin_and_unpin_are_persisted_and_audited() {
    let database = database("pin");
    let store = ContextCommandStore::new(database.connection());
    store
        .set_pin("task", "msg-0001-user", true, 1_000)
        .expect("pin");
    assert_eq!(
        store.pinned_items("task").expect("read"),
        vec!["msg-0001-user".to_string()]
    );
    store
        .set_pin("task", "msg-0001-user", false, 2_000)
        .expect("unpin");
    assert!(store.pinned_items("task").expect("read").is_empty());
    let audit = store.audit_log("task", 10).expect("audit");
    assert_eq!(audit.len(), 2);
    assert!(audit.iter().all(|record| record.outcome == "applied"));
}

#[test]
fn every_mutation_command_gets_a_ledger_entry_in_the_audit_log() {
    let database = database("audit");
    let store = ContextCommandStore::new(database.connection());
    store
        .request_summarize("task", 1_000)
        .expect("summarize now");
    store.clear_task("task", 1_100).expect("clear");
    store.set_pin("task", "item", true, 1_200).expect("pin");
    let commands: Vec<String> = store
        .audit_log("task", 10)
        .expect("audit")
        .into_iter()
        .map(|record| record.command)
        .collect();
    assert!(commands.contains(&"summarize_now".to_string()));
    assert!(commands.contains(&"clear_task_scratchpad".to_string()));
    assert!(commands.contains(&"pin_context_item".to_string()));
}

#[test]
fn the_rate_limit_rejects_excess_calls_and_records_the_refusal() {
    let database = database("rate-limit");
    let store = ContextCommandStore::new(database.connection());
    for index in 0..RATE_LIMIT_MAX_CALLS {
        store
            .set_pin("task", &format!("item-{index}"), true, 1_000)
            .expect("pin within the limit");
    }
    let error = store
        .set_pin("task", "item-over", true, 1_000)
        .expect_err("rate limit trips");
    assert!(error.to_string().contains("rate limit"));
    let refusals = store
        .audit_log("task", 100)
        .expect("audit")
        .into_iter()
        .filter(|record| record.outcome == "rate_limited")
        .count();
    assert_eq!(refusals, 1);
}

#[test]
fn the_rate_limit_window_slides() {
    let database = database("rate-window");
    let store = ContextCommandStore::new(database.connection());
    for index in 0..RATE_LIMIT_MAX_CALLS {
        store
            .set_pin("task", &format!("item-{index}"), true, 1_000)
            .expect("pin within the limit");
    }
    // За пределами окна счётчик снова пуст.
    store
        .set_pin("task", "item-later", true, 1_000 + RATE_LIMIT_WINDOW_MS + 1)
        .expect("pin after the window");
}

#[test]
fn a_summarize_request_is_consumed_exactly_once() {
    let database = database("summarize");
    let store = ContextCommandStore::new(database.connection());
    assert!(!store.take_pending_summarize("task", 900).expect("read"));
    store.request_summarize("task", 1_000).expect("request");
    store.request_summarize("task", 1_050).expect("request");
    assert!(store.take_pending_summarize("task", 1_100).expect("take"));
    assert!(!store.take_pending_summarize("task", 1_200).expect("take"));
}

#[test]
fn tasks_do_not_share_pins_or_rate_limits() {
    let database = database("isolation");
    let store = ContextCommandStore::new(database.connection());
    store.set_pin("task-a", "item", true, 1_000).expect("pin");
    assert!(store.pinned_items("task-b").expect("read").is_empty());
    // Первый pin уже израсходовал одну единицу лимита задачи task-a.
    for index in 1..RATE_LIMIT_MAX_CALLS {
        store
            .set_pin("task-a", &format!("item-{index}"), true, 1_000)
            .expect("pin");
    }
    store
        .set_pin("task-b", "item", true, 1_000)
        .expect("другая задача не ограничена чужим лимитом");
}
