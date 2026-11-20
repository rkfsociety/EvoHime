use super::*;
use evohime_context_budget::{
    budget::{BudgetUnavailableStage, MandatoryPart},
    ledger::CONTEXT_LEDGER_SCHEMA_VERSION,
};

use crate::LocalDatabase;

fn database(name: &str) -> LocalDatabase {
    let path = std::env::temp_dir().join(format!(
        "evohime-ledger-{name}-{}-{:?}.db",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    LocalDatabase::open(&path).expect("database opens")
}

fn entry(id: &str, session: &str, created_at: i64) -> ContextLedgerEntry {
    let mut entry = ContextLedgerEntry {
        id: id.to_string(),
        schema_version: CONTEXT_LEDGER_SCHEMA_VERSION,
        task_id: "task".to_string(),
        session_id: session.to_string(),
        model_call_id: format!("call-{id}"),
        created_at,
        provider: "literouter".to_string(),
        model: "gpt-4o-mini".to_string(),
        profile_version: "profile-1".to_string(),
        profile_snapshot: "{}".to_string(),
        tokenizer_version: "heuristic-1".to_string(),
        normalizer_version: "norm-1".to_string(),
        strategy_version: "strategy-1".to_string(),
        mandatory_tokens: 100,
        selected_optional_tokens: 200,
        reserves_tokens: 300,
        estimated_prompt_tokens: 300,
        selected_items: vec![SelectedItemRecord {
            id: "a".to_string(),
            estimated_tokens: 100,
        }],
        dropped_items: vec![DroppedItemRecord {
            id: "b".to_string(),
            drop_reason: DropReason::LowPriority,
        }],
        mandatory_parts: vec![MandatoryPartRecord {
            part: MandatoryPart::SafetyPolicy,
            items: 1,
            tokens: 100,
        }],
        ladder_levels_applied: vec![LadderLevel::LowPriorityOptional],
        compression: Vec::new(),
        loadout: None,
        fallback_estimator: false,
        replan_of: None,
        outcome: LedgerOutcome::Sent,
        budget_unavailable: None,
        context_ledger_hash: String::new(),
    };
    entry.finalize_hash();
    entry
}

#[test]
fn an_entry_round_trips_without_changing_its_hash() {
    let database = database("round-trip");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let written = entry("ledger-1", "session-1", 1_000);
    store.append(&written).expect("append succeeds");
    let read = store
        .get("ledger-1")
        .expect("read succeeds")
        .expect("entry exists");
    assert_eq!(read, written);
    assert_eq!(read.context_ledger_hash, read.compute_hash());
}

#[test]
fn usage_is_recorded_without_touching_the_immutable_entry() {
    let database = database("usage");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let written = entry("ledger-1", "session-1", 1_000);
    store.append(&written).expect("append succeeds");
    store
        .record_usage(&ContextLedgerUsage {
            ledger_id: "ledger-1".to_string(),
            actual_prompt_tokens: 290,
            actual_completion_tokens: 50,
            cache_read_input_tokens: Some(120),
            estimator_drift: 0.034,
            recorded_at: 2_000,
            route_scope: "primary".to_string(),
        })
        .expect("usage recorded");
    let read = store.get("ledger-1").expect("read").expect("entry");
    assert_eq!(read.context_ledger_hash, written.context_ledger_hash);
    let usage_rows: i64 = database
        .connection()
        .query_row("SELECT COUNT(*) FROM context_ledger_usage", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(usage_rows, 1);
    let recorded_cache_hit: Option<u32> = database
        .connection()
        .query_row(
            "SELECT cache_read_input_tokens FROM context_ledger_usage",
            [],
            |row| row.get(0),
        )
        .expect("cache-hit count");
    assert_eq!(recorded_cache_hit, Some(120));
}

#[test]
fn deepseek_daily_usage_projection_includes_actual_cache_hits_and_model() {
    let database = database("deepseek-daily-usage");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let mut written = entry("deepseek-ledger", "session-1", 1_000);
    written.provider = "openai_compatible".into();
    written.model = "deepseek-flash".into();
    written.created_at = 2_000;
    written.finalize_hash();
    store.append(&written).expect("append succeeds");
    store
        .record_usage(&ContextLedgerUsage {
            ledger_id: written.id,
            actual_prompt_tokens: 100,
            actual_completion_tokens: 20,
            cache_read_input_tokens: Some(65),
            estimator_drift: 0.0,
            recorded_at: 2_000,
            route_scope: "primary".to_string(),
        })
        .expect("usage recorded");

    let usage = store
        .deepseek_usage_since(1_500, "openai_compatible", "deepseek-flash")
        .expect("daily usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].model, "deepseek-flash");
    assert_eq!(usage[0].prompt_tokens, 100);
    assert_eq!(usage[0].completion_tokens, 20);
    assert_eq!(usage[0].cache_read_input_tokens, Some(65));
    assert_eq!(usage[0].recorded_at, 2_000);
    assert_eq!(usage[0].request_at, 2_000);
}

#[test]
fn deepseek_daily_usage_excludes_other_routes_and_pre_midnight_requests() {
    let database = database("deepseek-daily-scope");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let mut before_midnight = entry("before-midnight", "session-1", 1_000);
    before_midnight.provider = "openai_compatible".into();
    before_midnight.model = "deepseek-flash".into();
    before_midnight.created_at = 1_000;
    before_midnight.finalize_hash();
    store.append(&before_midnight).expect("append succeeds");
    let mut other_route = entry("other-route", "session-1", 1_000);
    other_route.provider = "literouter".into();
    other_route.model = "deepseek-flash".into();
    other_route.created_at = 2_000;
    other_route.finalize_hash();
    store.append(&other_route).expect("append succeeds");
    let mut current = entry("current", "session-1", 1_000);
    current.provider = "openai_compatible".into();
    current.model = "deepseek-flash".into();
    current.created_at = 2_000;
    current.finalize_hash();
    store.append(&current).expect("append succeeds");
    for ledger_id in [before_midnight.id, other_route.id, current.id] {
        store
            .record_usage(&ContextLedgerUsage {
                ledger_id,
                actual_prompt_tokens: 10,
                actual_completion_tokens: 1,
                cache_read_input_tokens: None,
                estimator_drift: 0.0,
                recorded_at: 2_000,
                route_scope: "primary".to_string(),
            })
            .expect("usage recorded");
    }

    let usage = store
        .deepseek_usage_since(1_500, "openai_compatible", "deepseek-flash")
        .expect("daily usage");
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].request_at, 2_000);
    assert_eq!(usage[0].recorded_at, 2_000);
}

#[test]
fn route_token_totals_only_count_matching_route_scope() {
    let database = database("route-token-scope");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    for (id, scope, prompt, completion) in [
        ("primary-1", "primary", 100_u32, 20_u32),
        ("primary-2", "primary", 50, 5),
        ("hive-1", "hive", 900, 90),
        ("legacy-1", "", 7, 7),
    ] {
        let mut written = entry(id, "session-1", 1_000);
        written.provider = "openai_compatible".into();
        written.model = "deepseek-flash".into();
        written.finalize_hash();
        store.append(&written).expect("append succeeds");
        store
            .record_usage(&ContextLedgerUsage {
                ledger_id: id.to_string(),
                actual_prompt_tokens: prompt,
                actual_completion_tokens: completion,
                cache_read_input_tokens: None,
                estimator_drift: 0.0,
                recorded_at: 2_000,
                route_scope: scope.to_string(),
            })
            .expect("usage recorded");
    }

    assert_eq!(
        store.route_tokens_since(1_500, "primary").expect("primary"),
        175
    );
    assert_eq!(store.route_tokens_since(1_500, "hive").expect("hive"), 990);
    assert_eq!(store.route_tokens_since(1_500, "").expect("legacy"), 14);
    assert_eq!(
        store.route_tokens_since(1_500, "unknown").expect("unknown"),
        0
    );
}

#[test]
fn lookup_by_hash_finds_the_recorded_entry() {
    let database = database("by-hash");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let written = entry("ledger-1", "session-1", 1_000);
    store.append(&written).expect("append succeeds");
    let found = store
        .find_by_hash(&written.context_ledger_hash)
        .expect("read")
        .expect("entry");
    assert_eq!(found.id, "ledger-1");
}

#[test]
fn projection_is_bounded_and_marks_truncation() {
    let database = database("projection");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let mut written = entry("ledger-1", "session-1", 1_000);
    written.selected_items = (0..250)
        .map(|index| SelectedItemRecord {
            id: format!("item-{index:03}"),
            estimated_tokens: 1,
        })
        .collect();
    written.finalize_hash();
    store.append(&written).expect("append succeeds");
    let projections = store.projection("task", 10).expect("projection");
    assert_eq!(projections.len(), 1);
    assert_eq!(projections[0].selected_item_ids.len(), BOUNDED_ID_LIMIT);
    assert!(projections[0].truncated);
}

#[test]
fn a_refused_assembly_keeps_its_stage_in_the_projection() {
    let database = database("refusal");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let mut written = entry("ledger-1", "session-1", 1_000);
    written.outcome = LedgerOutcome::BudgetUnavailable;
    written.budget_unavailable = Some(
        BudgetUnavailable::new(
            BudgetUnavailableStage::MandatoryOverflow,
            1_000,
            500,
            "profile-1",
            "heuristic-1",
        )
        .with_missing_part(Some(MandatoryPart::UserPrompt)),
    );
    written.finalize_hash();
    store.append(&written).expect("append succeeds");
    let projection = store.projection("task", 10).expect("projection");
    let refusal = projection[0]
        .budget_unavailable
        .as_ref()
        .expect("refusal is visible");
    assert_eq!(refusal.stage, BudgetUnavailableStage::MandatoryOverflow);
    assert_eq!(refusal.missing_part, Some(MandatoryPart::UserPrompt));
    assert_eq!(projection[0].outcome, "budget_unavailable");
}

#[test]
fn rotation_removes_old_entries_together_with_their_usage_rows() {
    let database = database("rotation");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let now = 1_800_000_000_000_i64;
    let old = now - 40 * 24 * 60 * 60 * 1000;
    // Свежая запись новой сессии: старая сессия перестаёт быть «последней».
    for index in 0..(LEDGER_RETAINED_SESSIONS + 1) {
        let mut fresh = entry(&format!("fresh-{index}"), &format!("session-{index}"), now);
        fresh.finalize_hash();
        store.append(&fresh).expect("append succeeds");
    }
    let stale = entry("stale", "session-stale", old);
    store.append(&stale).expect("append succeeds");
    store
        .record_usage(&ContextLedgerUsage {
            ledger_id: "stale".to_string(),
            actual_prompt_tokens: 1,
            actual_completion_tokens: 1,
            cache_read_input_tokens: None,
            estimator_drift: 0.0,
            recorded_at: old,
            route_scope: "primary".to_string(),
        })
        .expect("usage recorded");

    let removed = store.prune(now).expect("prune runs");
    assert_eq!(removed, 1);
    assert!(store.get("stale").expect("read").is_none());
    let usage_rows: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM context_ledger_usage WHERE ledger_id = 'stale'",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(usage_rows, 0);
}

#[test]
fn rotation_keeps_entries_referenced_by_an_unexported_receipt() {
    let database = database("receipt");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let now = 1_800_000_000_000_i64;
    let old = now - 40 * 24 * 60 * 60 * 1000;
    for index in 0..(LEDGER_RETAINED_SESSIONS + 1) {
        store
            .append(&entry(
                &format!("fresh-{index}"),
                &format!("session-{index}"),
                now,
            ))
            .expect("append succeeds");
    }
    store
        .append(&entry("pinned", "session-pinned", old))
        .expect("append succeeds");
    store
        .register_receipt("pinned", "receipt-1", false)
        .expect("receipt registered");

    assert_eq!(store.prune(now).expect("prune runs"), 0);
    assert!(store.get("pinned").expect("read").is_some());

    store
        .register_receipt("pinned", "receipt-1", true)
        .expect("receipt exported");
    assert_eq!(store.prune(now).expect("prune runs"), 1);
}

#[test]
fn recent_sessions_survive_the_age_cutoff() {
    let database = database("recent-session");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    let now = 1_800_000_000_000_i64;
    let old = now - 40 * 24 * 60 * 60 * 1000;
    store
        .append(&entry("old-but-recent-session", "session-1", old))
        .expect("append succeeds");
    assert_eq!(store.prune(now).expect("prune runs"), 0);
}

#[test]
fn a_golden_entry_of_the_previous_schema_version_reads_without_rewrite() {
    let database = database("golden");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    // «Золотая» запись предыдущей версии: неизвестный `drop_reason` и
    // отсутствующее необязательное поле.
    database
        .connection()
        .execute(
            "INSERT INTO context_ledger (
                    id, schema_version, task_id, session_id, model_call_id, created_at,
                    provider, model, profile_version, profile_snapshot, tokenizer_version,
                    normalizer_version, strategy_version, mandatory_tokens,
                    selected_optional_tokens, reserves_tokens, estimated_prompt_tokens,
                    selected_items, dropped_items, mandatory_parts, ladder_levels_applied,
                    compression, loadout, fallback_estimator, replan_of, outcome,
                    budget_unavailable, context_ledger_hash
                 ) VALUES (
                    'golden', 0, 'task', 'session-1', 'call', 1000,
                    'literouter', 'model', 'profile-0', '{}', 'tok-0',
                    'norm-0', 'strategy-0', 10, 20, 30, 30,
                    '[{\"id\":\"a\",\"estimated_tokens\":10}]',
                    '[{\"id\":\"b\",\"drop_reason\":\"future_reason\"}]',
                    '[]', '[]', '[]', NULL, 0, NULL, 'sent', NULL, 'golden-hash'
                 )",
            [],
        )
        .expect("golden row inserted");

    let read = store.get("golden").expect("read").expect("entry exists");
    assert_eq!(read.schema_version, 0);
    assert_eq!(read.context_ledger_hash, "golden-hash");
    // Неизвестный `drop_reason` не роняет чтение и не переписывает запись.
    let stored_hash: String = database
        .connection()
        .query_row(
            "SELECT context_ledger_hash FROM context_ledger WHERE id = 'golden'",
            [],
            |row| row.get(0),
        )
        .expect("hash still readable");
    assert_eq!(stored_hash, "golden-hash");
}

#[test]
fn concurrent_appends_stay_atomic_and_hash_stable() {
    let database = database("concurrent");
    let path = database.path().to_path_buf();
    drop(database);
    let entries: Vec<ContextLedgerEntry> = (0..8)
        .map(|index| {
            entry(
                &format!("ledger-{index}"),
                &format!("session-{index}"),
                1_000,
            )
        })
        .collect();
    let expected: Vec<String> = entries
        .iter()
        .map(|entry| entry.context_ledger_hash.clone())
        .collect();

    let handles: Vec<_> = entries
        .into_iter()
        .map(|entry| {
            let path = path.clone();
            std::thread::spawn(move || {
                let database = LocalDatabase::open(&path).expect("database opens");
                let store = ContextLedgerStore::new(database.connection()).expect("store opens");
                store.append(&entry).expect("append succeeds");
            })
        })
        .collect();
    for handle in handles {
        handle.join().expect("thread completes");
    }

    let database = LocalDatabase::open(&path).expect("database opens");
    let store = ContextLedgerStore::new(database.connection()).expect("store opens");
    assert_eq!(store.count().expect("count"), 8);
    for (index, hash) in expected.iter().enumerate() {
        let read = store
            .get(&format!("ledger-{index}"))
            .expect("read")
            .expect("entry");
        // Hash не зависит от порядка коммитов соседних задач.
        assert_eq!(&read.context_ledger_hash, hash);
    }
}
