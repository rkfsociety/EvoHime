use super::*;

#[test]
fn latest_schema_installs_hot_event_indexes() {
    let path = temp_database_path("hot-event-indexes");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    let count: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='index' AND name IN
                 ('idx_events_task_sequence', 'idx_events_action_terminal',
                  'idx_events_review_lookup')",
            [],
            |row| row.get(0),
        )
        .expect("index query");
    assert_eq!(count, 3);
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn prepared_open_does_not_install_schema() {
    let path = temp_database_path("prepared-open");
    let _ = std::fs::remove_file(&path);
    let connection = rusqlite::Connection::open(&path).expect("database opens");
    connection
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .expect("schema version is writable");
    drop(connection);

    let database = LocalDatabase::open_prepared(&path).expect("prepared database opens");
    assert!(!database.has_events_table().expect("events table query"));
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db-wal"));
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
}

#[test]
fn latest_schema_installs_configuration_experience_diagnostics_task_graph_component_registry_refs_workbench_coordinator_instruction_workspace_set_and_knowledge_tables(
) {
    let path = temp_database_path("experience-replay-schema-66");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    assert_eq!(
        database.schema_version().expect("schema version"),
        SCHEMA_VERSION
    );
    let has_ui_extension_table: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='safe_ui_extensions'",
            [],
            |row| row.get(0),
        )
        .expect("UI extension table query");
    assert_eq!(has_ui_extension_table, 1);
    let has_knowledge_collections_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_collections'",
                [],
                |row| row.get(0),
            )
            .expect("knowledge collection table query");
    assert_eq!(has_knowledge_collections_table, 1);
    let has_workbench_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='capability_workbench_instances'",
                [],
                |row| row.get(0),
            )
            .expect("workbench table query");
    assert_eq!(has_workbench_table, 1);
    let has_coordinator_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='team_coordinator_work_items'",
                [],
                |row| row.get(0),
            )
            .expect("coordinator table query");
    assert_eq!(has_coordinator_table, 1);
    let has_instruction_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='project_instruction_rules'",
                [],
                |row| row.get(0),
            )
            .expect("instruction table query");
    assert_eq!(has_instruction_table, 1);
    let has_workspace_sets_table: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_sets'",
            [],
            |row| row.get(0),
        )
        .expect("workspace set table query");
    assert_eq!(has_workspace_sets_table, 1);
    let has_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='workspace_bootstrap_manifests'",
                [],
                |row| row.get(0),
            )
            .expect("bootstrap table query");
    assert_eq!(has_table, 1);
    let has_team_table: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='team_coordination_policies'",
                [],
                |row| row.get(0),
            )
            .expect("team policy table query");
    assert_eq!(has_team_table, 1);
    let has_handoff_table: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='typed_agent_handoffs'",
            [],
            |row| row.get(0),
        )
        .expect("handoff table query");
    assert_eq!(has_handoff_table, 1);
    let has_configuration_table: i64 = database.connection().query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='schema_agent_configurations'", [], |row| row.get(0)).expect("configuration table query");
    assert_eq!(has_configuration_table, 1);
    let has_experience_table: i64 = database.connection().query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='experience_replay_records'", [], |row| row.get(0)).expect("experience table query");
    assert_eq!(has_experience_table, 1);
    let has_diagnostics_table: i64 = database.connection().query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='code_diagnostics_snapshots'", [], |row| row.get(0)).expect("diagnostics table query");
    assert_eq!(has_diagnostics_table, 1);
    let has_bus_table: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='core_bus_events'",
            [],
            |row| row.get(0),
        )
        .expect("bus table query");
    assert_eq!(has_bus_table, 1);
    drop(database);
    let _ = std::fs::remove_file(&path);
}

/// Clearing the review history must hide earlier runs from the list while
/// leaving them in the journal, which stays append-only for audit and
/// export.
#[test]
fn review_history_starts_after_the_newest_clear_marker() {
    let path = temp_database_path("review-history");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    database
        .append_event("review-old", "task.completed", b"{}")
        .expect("old review records");
    assert_eq!(
        database
            .read_review_events(10)
            .expect("history reads")
            .len(),
        1
    );

    database
        .append_event("review-history-1", "review.history_cleared", b"{}")
        .expect("marker records");
    assert!(database
        .read_review_events(10)
        .expect("history reads")
        .is_empty());

    database
        .append_event("review-new", "task.completed", b"{}")
        .expect("new review records");
    let visible = database.read_review_events(10).expect("history reads");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].task_id, "review-new");
    // The cleared review is hidden from the list, not removed.
    let all = database
        .read_events_after(0, usize::MAX)
        .expect("journal reads");
    assert!(all.iter().any(|event| event.task_id == "review-old"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn creates_schema_and_reports_version() {
    let path = temp_database_path("schema");
    let _ = std::fs::remove_file(&path);
    let database = LocalDatabase::open(&path).expect("database opens");
    assert_eq!(
        database.schema_version().expect("version reads"),
        SCHEMA_VERSION
    );
    assert!(database.has_events_table().expect("table exists"));
    for table in [
        "persistent_agents",
        "persistent_agent_revisions",
        "persistent_agent_reporting_history",
        "persistent_agent_goal_bindings",
        "persistent_agent_assignments",
        "persistent_agent_commands",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("persistent agent table lookup");
        assert_eq!(exists, 1, "{table} must exist in a fresh schema");
    }
    for table in [
        "continuation_policies",
        "continuation_runs",
        "continuation_attempts",
        "continuation_actions",
        "continuation_gate_results",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("continuation table lookup");
        assert_eq!(exists, 1, "{table} must exist in a fresh schema");
    }
    let id = database
        .record_tool_metric(ToolMetricInput {
            task_id: "task-1",
            tool_name: "filesystem.read",
            iteration: 2,
            ok: false,
            failure_kind: Some("not_found"),
            recovery_hint: true,
            escalated: false,
        })
        .expect("metric records");
    assert!(id > 0);
    let metrics = database
        .read_tool_metrics("task-1", 10)
        .expect("metrics read");
    assert_eq!(metrics.len(), 1);
    assert_eq!(metrics[0].failure_kind.as_deref(), Some("not_found"));
    assert!(metrics[0].recovery_hint);
    drop(database);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn schema_90_migrates_guided_calibration_and_persistent_agent_registry_atomically() {
    let path = temp_database_path("persistent-agent-schema-90");
    let backup = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy db opens");
        connection
            .pragma_update(None, "user_version", 90_u32)
            .expect("legacy version writes");
        connection
            .execute_batch("CREATE TABLE migration_marker(value TEXT NOT NULL);")
            .expect("legacy marker writes");
    }
    let database = LocalDatabase::open(&path).expect("legacy db migrates");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    for table in ["guided_calibration_sessions", "persistent_agents"] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must be installed by 90->92");
    }
    assert!(backup.exists(), "migration backup must be retained");
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup);
}

#[test]
fn migrates_schema_32_to_goals_schema_33_with_backup() {
    let path = temp_database_path("migration-32-to-33-goals");
    let _ = std::fs::remove_file(&path);
    let backup_path = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&backup_path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE legacy_marker(id INTEGER);
                     INSERT INTO legacy_marker(id) VALUES (25);
                     PRAGMA user_version = 32;",
            )
            .expect("legacy schema seeds");
    }

    let database = LocalDatabase::open(&path).expect("migration succeeds");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(backup_path.exists(), "pre-migration backup must be written");
    for table in ["goals", "goal_revisions", "goal_events", "goal_commands"] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist after migration to schema 33");
    }
    let preserved: i64 = database
        .connection()
        .query_row("SELECT id FROM legacy_marker", [], |row| row.get(0))
        .expect("legacy row survives");
    assert_eq!(preserved, 25);

    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup_path);
}

#[test]
fn migrates_schema_37_to_analysis_kernel_schema_38_with_backup() {
    let path = temp_database_path("migration-37-to-38-analysis-kernel");
    let _ = std::fs::remove_file(&path);
    let backup_path = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&backup_path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE legacy_marker(id INTEGER);
                     INSERT INTO legacy_marker(id) VALUES (28);
                     PRAGMA user_version = 37;",
            )
            .expect("legacy schema seeds");
    }
    let database = LocalDatabase::open(&path).expect("migration succeeds");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(backup_path.exists(), "pre-migration backup must be written");
    for table in [
        "analysis_kernel_sessions",
        "analysis_kernel_objects",
        "analysis_kernel_events",
        "analysis_kernel_idempotency",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist after migration to schema 38");
    }
    let preserved: i64 = database
        .connection()
        .query_row("SELECT id FROM legacy_marker", [], |row| row.get(0))
        .expect("legacy row survives");
    assert_eq!(preserved, 28);
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup_path);
}

#[test]
fn migrates_schema_18_to_workspace_rag_schema_19_transactionally() {
    let path = temp_database_path("migration-18-to-19-rag");
    let _ = std::fs::remove_file(&path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 18;")
            .expect("legacy schema seeds");
    }
    let database = LocalDatabase::open(&path).expect("migration succeeds");
    // Открытие всегда доводит базу до текущей версии, поэтому проверяется
    // не «19», а наличие таблиц, которые добавила именно эта миграция.
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    for table in [
        "workspace_index_runs",
        "workspace_documents",
        "document_chunks",
        "workspace_vector_indexes",
        "workspace_chunk_vectors",
        "rag_context_ledger",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist after migration");
    }
    let _ = std::fs::remove_file(path);
}

/// Лимиты моделей должны появиться на уже работающей базе: пользователь
/// обновляет сборку, а не заводит хранилище заново.
#[test]
fn migrates_schema_19_to_model_context_limits_schema_20() {
    let path = temp_database_path("migration-19-to-20-limits");
    let _ = std::fs::remove_file(&path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 19;")
            .expect("legacy schema seeds");
    }
    let database = LocalDatabase::open(&path).expect("migration succeeds");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'model_context_limits'",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(exists, 1, "model_context_limits must exist after migration");
    let _ = std::fs::remove_file(path);
}

/// Stage 01.4: a pre-22 database gaining `receipt_records` et al. must go
/// through the same backup-before-migrate path as any other schema
/// change, not the unconditional idempotent call `open_internal` also
/// makes after `migrate` returns.
#[test]
fn migrates_schema_21_to_receipts_v1_schema_22_with_backup() {
    let path = temp_database_path("migration-21-to-22-receipts");
    let _ = std::fs::remove_file(&path);
    let backup_path = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&backup_path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch("CREATE TABLE legacy_marker(id INTEGER); PRAGMA user_version = 21;")
            .expect("legacy schema seeds");
    }
    let database = LocalDatabase::open(&path).expect("migration succeeds");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(
        backup_path.exists(),
        "pre-migration backup must be written before schema 22 applies"
    );
    for table in [
        "receipt_records",
        "receipt_actions",
        "receipt_chain_heads",
        "receipt_checkpoints",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist after migration to schema 22");
    }
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup_path);
}

/// Этап 04.2: ambient-таблицы приезжают миграцией v25 поверх живой базы,
/// не трогая уже существующие строки.
#[test]
fn migrates_schema_24_to_ambient_schema_25_without_touching_existing_rows() {
    let path = temp_database_path("migration-24-to-25-ambient");
    let _ = std::fs::remove_file(&path);
    let backup_path = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&backup_path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE legacy_marker(id INTEGER);
                     INSERT INTO legacy_marker(id) VALUES (7);
                     PRAGMA user_version = 24;",
            )
            .expect("legacy schema seeds");
    }
    let database = LocalDatabase::open(&path).expect("migration succeeds");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(
        backup_path.exists(),
        "pre-migration backup must be written before schema 25 applies"
    );
    let preserved: i64 = database
        .connection()
        .query_row("SELECT id FROM legacy_marker", [], |row| row.get(0))
        .expect("legacy row survives");
    assert_eq!(preserved, 7);
    for table in [
        "ambient_episodes",
        "ambient_utterances",
        "ambient_tombstones",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist after migration to schema 25");
    }
    // Повторное открытие не пытается мигрировать второй раз.
    drop(database);
    let reopened = LocalDatabase::open(&path).expect("second open is a no-op");
    assert_eq!(reopened.schema_version().unwrap(), SCHEMA_VERSION);
    drop(reopened);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup_path);
}

/// Этап 04.7: таблицы предложений приезжают миграцией v26 поверх живого
/// ambient-хранилища v25 и не трогают его строк.
#[test]
fn migrates_schema_25_to_proactivity_schema_26_without_touching_ambient_rows() {
    let path = temp_database_path("migration-25-to-26-proactivity");
    let _ = std::fs::remove_file(&path);
    let backup_path = path.with_extension("db.bak");
    let _ = std::fs::remove_file(&backup_path);
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE ambient_episodes (
                        episode_id TEXT PRIMARY KEY NOT NULL,
                        started_at TEXT NOT NULL,
                        ended_at TEXT,
                        utterance_count INTEGER NOT NULL,
                        speech_ms INTEGER NOT NULL,
                        engine_version TEXT NOT NULL,
                        model_id TEXT NOT NULL,
                        extraction_state TEXT NOT NULL,
                        expires_at TEXT NOT NULL
                     );
                     INSERT INTO ambient_episodes VALUES
                        ('ep-1','2026-08-21T10:00:00.000Z',NULL,1,900,'whisper','base',
                         'done','2026-09-20T10:00:00.000Z');
                     PRAGMA user_version = 25;",
            )
            .expect("v25 schema seeds");
    }
    let database = LocalDatabase::open(&path).expect("migration succeeds");
    assert_eq!(database.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(
        backup_path.exists(),
        "pre-migration backup must be written before schema 26 applies"
    );
    let preserved: String = database
        .connection()
        .query_row("SELECT episode_id FROM ambient_episodes", [], |row| {
            row.get(0)
        })
        .expect("ambient row survives");
    assert_eq!(preserved, "ep-1");
    for table in [
        "ambient_proposals",
        "ambient_proposal_mutes",
        "ambient_proactivity_counters",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(exists, 1, "{table} must exist after migration to schema 26");
    }
    drop(database);
    let reopened = LocalDatabase::open(&path).expect("second open is a no-op");
    assert_eq!(reopened.schema_version().unwrap(), SCHEMA_VERSION);
    drop(reopened);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&backup_path);
}

/// Схема физически не может хранить аудио: ни одна ambient-колонка не
/// объявлена как BLOB, поэтому «PCM не пишется на диск» — свойство
/// таблицы, а не дисциплины вызывающего кода.
#[test]
fn ambient_tables_have_no_blob_column() {
    let path = temp_database_path("ambient-no-blob");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    let database = LocalDatabase::open(&path).expect("database opens");
    for table in [
        "ambient_episodes",
        "ambient_utterances",
        "ambient_tombstones",
        "ambient_proposals",
        "ambient_proposal_mutes",
        "ambient_proactivity_counters",
    ] {
        let mut statement = database
            .connection()
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("pragma prepares");
        let types: Vec<String> = statement
            .query_map([], |row| row.get::<_, String>(2))
            .expect("pragma runs")
            .map(|row| row.expect("column type").to_ascii_uppercase())
            .collect();
        assert!(!types.is_empty(), "{table} must exist");
        assert!(
            !types.iter().any(|kind| kind.contains("BLOB")),
            "{table} must not be able to hold audio: {types:?}"
        );
    }
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
}

#[test]
fn backs_up_existing_database_before_migration() {
    let path = temp_database_path("backup");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .pragma_update(None, "user_version", 0_u32)
            .expect("legacy version writes");
    }
    let _database = LocalDatabase::open(&path).expect("database migrates");
    assert!(path.with_extension("db.bak").exists());
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
}

#[test]
fn migration_12_to_16_is_idempotent_and_preserves_existing_rows() {
    let path = temp_database_path("feedback-migration");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    {
        // Seed a pre-wave (user_version 12) database with an existing
        // memory_entries row, so we can confirm migrations 13 through 16 do not
        // touch unrelated data.
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL,
                        confirmations INTEGER NOT NULL DEFAULT 1,
                        lesson_key TEXT
                    );
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten)
                        VALUES ('m-1', 'project', 'p-1', 'Decision', 'keep this', 'run:1',
                                'internal', '2026-08-01T00:00:00Z', NULL, 0, 0);
                    PRAGMA user_version = 12;",
            )
            .expect("legacy schema and data write");
    }

    let database = LocalDatabase::open(&path).expect("database migrates forward");
    assert_eq!(
        database.schema_version().expect("version reads"),
        SCHEMA_VERSION
    );
    let feedback_table_exists: i64 = database
        .connection()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='feedback_entries'",
            [],
            |row| row.get(0),
        )
        .expect("feedback table check");
    assert_eq!(feedback_table_exists, 1);
    let preserved: String = database
        .connection()
        .query_row(
            "SELECT content FROM memory_entries WHERE id = 'm-1'",
            [],
            |row| row.get(0),
        )
        .expect("existing memory row survives migration");
    assert_eq!(preserved, "keep this");
    drop(database);

    // Re-opening an already-migrated database must not error and must
    // not duplicate the feedback_entries table or existing rows
    // (guarded CREATE TABLE IF NOT EXISTS / PRAGMA user_version checks).
    let reopened = LocalDatabase::open(&path).expect("reopen is idempotent");
    assert_eq!(
        reopened.schema_version().expect("version stays current"),
        SCHEMA_VERSION
    );
    let row_count: i64 = reopened
        .connection()
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
        .expect("row count reads");
    assert_eq!(row_count, 1);
    drop(reopened);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
}

#[test]
fn migration_16_maps_memory_v1_rows_onto_the_extraction_contract() {
    // Memory v1 -> Memory Extraction: явные failure lessons получают
    // kind=lesson, прочие старые факты -- kind=entity; все legacy rows
    // остаются активной памятью с legacy-версиями extractor/policy и
    // пустой цепочкой supersede.
    let path = temp_database_path("memory-extraction-migration");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE memory_entries (
                        id TEXT PRIMARY KEY NOT NULL,
                        scope_kind TEXT NOT NULL,
                        scope_id TEXT NOT NULL,
                        title TEXT NOT NULL,
                        content TEXT NOT NULL,
                        provenance TEXT NOT NULL,
                        privacy TEXT NOT NULL,
                        created_at TEXT NOT NULL,
                        expires_at TEXT,
                        archived INTEGER NOT NULL,
                        forgotten INTEGER NOT NULL,
                        confirmations INTEGER NOT NULL DEFAULT 1,
                        lesson_key TEXT
                    );
                    INSERT INTO memory_entries
                        (id, scope_kind, scope_id, title, content, provenance, privacy,
                         created_at, expires_at, archived, forgotten, confirmations, lesson_key)
                        VALUES
                        ('fact-1', 'project', 'p-1', 'Решение', 'сборка через cargo', 'run:1',
                         'internal', '2026-08-01T00:00:00Z', NULL, 0, 0, 1, NULL),
                        ('lesson-1', 'project', 'p-1', 'Урок', 'проверяй аргументы', 'task:t-1',
                         'private', '2026-08-02T00:00:00Z', NULL, 0, 0, 3, 'lesson-key-1'),
                        ('gone-1', 'project', 'p-1', '', '', '', 'internal',
                         '2026-08-03T00:00:00Z', NULL, 0, 1, 1, NULL);
                    PRAGMA user_version = 12;",
            )
            .expect("legacy schema and data write");
    }

    let database = LocalDatabase::open(&path).expect("database migrates forward");
    assert_eq!(
        database.schema_version().expect("version reads"),
        SCHEMA_VERSION
    );
    // Транзакционность миграции подтверждается наличием backup рядом.
    assert!(path.with_extension("db.bak").exists());

    let mapped = |id: &str| -> (String, String, String, String, f64, f64) {
        database
            .connection()
            .query_row(
                "SELECT kind, confirmation_state, extractor_version, policy_version,
                            model_confidence, verification_confidence
                     FROM memory_entries WHERE id = ?1",
                rusqlite::params![id],
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
            .expect("mapped row reads")
    };
    assert_eq!(
        mapped("fact-1"),
        (
            "entity".to_owned(),
            "confirmed".to_owned(),
            "v1_legacy".to_owned(),
            "legacy-v1".to_owned(),
            1.0,
            1.0
        )
    );
    assert_eq!(mapped("lesson-1").0, "lesson");
    assert_eq!(mapped("lesson-1").1, "confirmed");
    // Уже забытая запись не воскресает в состоянии confirmed.
    assert_eq!(mapped("gone-1").1, "forgotten");

    let (supersedes, superseded_by): (Option<String>, Option<String>) = database
        .connection()
        .query_row(
            "SELECT supersedes, superseded_by FROM memory_entries WHERE id = 'fact-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("supersede columns read");
    assert_eq!(supersedes, None);
    assert_eq!(superseded_by, None);

    // Исходные statement и provenance сохранены дословно.
    let (content, provenance): (String, String) = database
        .connection()
        .query_row(
            "SELECT content, provenance FROM memory_entries WHERE id = 'fact-1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("statement survives");
    assert_eq!(content, "сборка через cargo");
    assert_eq!(provenance, "run:1");

    // canonical_subject остаётся NULL: нормализатор версионируется в Core.
    let subject: Option<String> = database
        .connection()
        .query_row(
            "SELECT canonical_subject FROM memory_entries WHERE id = 'fact-1'",
            [],
            |row| row.get(0),
        )
        .expect("subject reads");
    assert_eq!(subject, None);

    for table in [
        "memory_aliases",
        "memory_tombstones",
        "memory_session_notes",
    ] {
        let exists: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                rusqlite::params![table],
                |row| row.get(0),
            )
            .expect("table check");
        assert_eq!(exists, 1, "{table} must exist after migration 16");
    }
    drop(database);

    // Повторное открытие не дублирует колонки и не меняет данные.
    let reopened = LocalDatabase::open(&path).expect("reopen is idempotent");
    let rows: i64 = reopened
        .connection()
        .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))
        .expect("row count reads");
    assert_eq!(rows, 3);
    drop(reopened);

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
}

#[test]
fn restores_backup_when_migration_fails() {
    let path = temp_database_path("rollback");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
    {
        let connection = rusqlite::Connection::open(&path).expect("legacy database opens");
        connection
            .execute_batch(
                "CREATE TABLE marker(value TEXT NOT NULL); INSERT INTO marker VALUES ('legacy');",
            )
            .expect("legacy data writes");
    }
    assert!(LocalDatabase::open_internal(&path, true).is_err());
    let database = LocalDatabase::open(&path).expect("database restores and migrates");
    let marker: String = database
        .connection
        .query_row("SELECT value FROM marker", [], |row| row.get(0))
        .expect("legacy marker survives rollback");
    assert_eq!(marker, "legacy");
    drop(database);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("db.bak"));
}
