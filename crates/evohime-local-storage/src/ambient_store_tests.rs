use super::*;

/// Схема, эквивалентная миграции v25, плюс те таблицы Core, к которым
/// удаление обязано прикоснуться: durable journal и память.
fn schema(connection: &Connection) {
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
                 CREATE TABLE ambient_episodes (
                    episode_id TEXT PRIMARY KEY NOT NULL,
                    started_at TEXT NOT NULL,
                    ended_at TEXT,
                    utterance_count INTEGER NOT NULL,
                    speech_ms INTEGER NOT NULL,
                    engine_version TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    extraction_state TEXT NOT NULL CHECK(extraction_state IN
                        ('disabled','pending','done','failed')),
                    expires_at TEXT NOT NULL
                 );
                 CREATE TABLE ambient_utterances (
                    utterance_id TEXT PRIMARY KEY NOT NULL,
                    episode_id TEXT NOT NULL
                        REFERENCES ambient_episodes(episode_id) ON DELETE CASCADE,
                    sequence INTEGER NOT NULL,
                    started_at TEXT NOT NULL,
                    duration_ms INTEGER NOT NULL,
                    text TEXT NOT NULL,
                    text_hash TEXT NOT NULL,
                    language TEXT NOT NULL,
                    avg_logprob REAL NOT NULL,
                    speaker TEXT NOT NULL,
                    redacted INTEGER NOT NULL DEFAULT 0,
                    expires_at TEXT NOT NULL,
                    UNIQUE(episode_id, sequence)
                 );
                 CREATE TABLE ambient_tombstones (
                    tombstone_id TEXT PRIMARY KEY NOT NULL,
                    episode_id TEXT NOT NULL,
                    removed_at TEXT NOT NULL,
                    reason TEXT NOT NULL,
                    utterance_count INTEGER NOT NULL,
                    expires_at TEXT NOT NULL,
                    UNIQUE(episode_id, removed_at)
                 );
                 CREATE TABLE ambient_proposals (
                    proposal_id TEXT PRIMARY KEY NOT NULL,
                    proposal_key TEXT NOT NULL UNIQUE,
                    mute_key TEXT NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ('suggestion','reminder')),
                    subject_key TEXT NOT NULL,
                    subject TEXT NOT NULL,
                    title TEXT NOT NULL,
                    source_episode_id TEXT
                        REFERENCES ambient_episodes(episode_id) ON DELETE SET NULL,
                    source_deleted_at TEXT,
                    source_deleted_reason TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    expires_at TEXT NOT NULL,
                    occurrences INTEGER NOT NULL DEFAULT 1,
                    state TEXT NOT NULL CHECK(state IN
                        ('proposed','accepted','declined','muted','expired')),
                    accepted_task_id TEXT,
                    idempotency_key TEXT,
                    CHECK((source_deleted_at IS NULL AND source_deleted_reason IS NULL)
                       OR (source_deleted_at IS NOT NULL AND source_deleted_reason IS NOT NULL))
                 );
                 CREATE UNIQUE INDEX idx_ambient_proposal_idempotency
                    ON ambient_proposals(idempotency_key) WHERE idempotency_key IS NOT NULL;
                 CREATE TABLE ambient_proposal_mutes (
                    mute_key TEXT PRIMARY KEY NOT NULL,
                    kind TEXT NOT NULL CHECK(kind IN ('suggestion','reminder')),
                    subject_key TEXT NOT NULL,
                    muted_at TEXT NOT NULL
                 );
                 CREATE TABLE ambient_proactivity_counters (
                    profile_id TEXT PRIMARY KEY NOT NULL,
                    hour_started_at_ms INTEGER NOT NULL,
                    hour_count INTEGER NOT NULL,
                    day_started_at_ms INTEGER NOT NULL,
                    day_count INTEGER NOT NULL,
                    last_proposed_at_ms INTEGER
                 );
                 CREATE TABLE events (
                    sequence_id INTEGER PRIMARY KEY AUTOINCREMENT,
                    task_id TEXT NOT NULL,
                    event_type TEXT NOT NULL,
                    payload BLOB NOT NULL,
                    created_at TEXT NOT NULL
                 );
                 CREATE TABLE memory_entries (
                    id TEXT PRIMARY KEY NOT NULL,
                    confirmation_state TEXT NOT NULL,
                    supersession_reason TEXT,
                    provenance_source_id TEXT
                 );",
        )
        .expect("ambient schema installs");
}

fn open() -> Connection {
    let connection = Connection::open_in_memory().expect("in-memory database");
    schema(&connection);
    connection
}

fn episode(id: &str, started_at: &str, expires_at: &str) -> AmbientEpisodeRecord {
    AmbientEpisodeRecord {
        episode_id: id.to_owned(),
        started_at: started_at.to_owned(),
        ended_at: None,
        utterance_count: 0,
        speech_ms: 0,
        engine_version: "whisper-base-q5_1".to_owned(),
        model_id: "base-q5_1".to_owned(),
        extraction_state: ExtractionState::Pending,
        expires_at: expires_at.to_owned(),
    }
}

fn utterance(
    id: &str,
    episode_id: &str,
    sequence: i64,
    started_at: &str,
    text: &str,
    expires_at: &str,
) -> AmbientUtteranceRecord {
    AmbientUtteranceRecord {
        utterance_id: id.to_owned(),
        episode_id: episode_id.to_owned(),
        sequence,
        started_at: started_at.to_owned(),
        duration_ms: 1_000,
        text: text.to_owned(),
        text_hash: format!("hash-{text}"),
        language: "ru".to_owned(),
        avg_logprob: -0.25,
        speaker: SPEAKER_UNVERIFIED.to_owned(),
        redacted: false,
        expires_at: expires_at.to_owned(),
    }
}

fn append_event(connection: &Connection, task_id: &str, event_type: &str, created_at: &str) {
    connection
        .execute(
            "INSERT INTO events(task_id, event_type, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
            params![task_id, event_type, Vec::<u8>::new(), created_at],
        )
        .expect("event appends");
}

fn candidate(connection: &Connection, id: &str, source: &str, state: &str) {
    connection
        .execute(
            "INSERT INTO memory_entries(id, confirmation_state, provenance_source_id)
                 VALUES (?1, ?2, ?3)",
            params![id, state, source],
        )
        .expect("candidate inserts");
}

fn candidate_state(connection: &Connection, id: &str) -> (String, Option<String>) {
    connection
        .query_row(
            "SELECT confirmation_state, supersession_reason FROM memory_entries WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("candidate exists")
}

fn counters(connection: &Connection, episode_id: &str) -> (i64, i64) {
    connection
        .query_row(
            "SELECT utterance_count, speech_ms FROM ambient_episodes WHERE episode_id = ?1",
            params![episode_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("episode exists")
}

fn event_types(connection: &Connection) -> Vec<String> {
    let mut statement = connection
        .prepare("SELECT event_type FROM events ORDER BY sequence_id")
        .expect("statement");
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query");
    rows.map(|row| row.expect("row")).collect()
}

#[test]
fn utterances_round_trip_and_keep_episode_counters_in_step() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    for (index, text) in ["первое", "второе"].into_iter().enumerate() {
        assert!(AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                &format!("u-{index}"),
                "ep-1",
                index as i64,
                &format!("2026-08-20T10:0{index}:00.000Z"),
                text,
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:59:00.000Z",
        )
        .expect("utterance inserts"));
    }
    assert_eq!(counters(&connection, "ep-1"), (2, 2_000));
    assert!(
        AmbientStoreSql::close_episode(&connection, "ep-1", "2026-08-20T10:05:00.000Z")
            .expect("episode closes")
    );
    let stored = AmbientStoreSql::get_episode(&connection, "ep-1")
        .expect("read")
        .expect("episode exists");
    assert_eq!(stored.ended_at.as_deref(), Some("2026-08-20T10:05:00.000Z"));
    assert_eq!(stored.extraction_state, ExtractionState::Pending);
    assert!(
        AmbientStoreSql::set_extraction_state(&connection, "ep-1", ExtractionState::Done)
            .expect("state updates")
    );
    let texts: Vec<String> = AmbientStoreSql::list_utterances(&connection, "ep-1", 100)
        .expect("read")
        .into_iter()
        .map(|record| record.text)
        .collect();
    assert_eq!(texts, vec!["первое".to_owned(), "второе".to_owned()]);
    assert_eq!(
        AmbientStoreSql::list_episodes(&connection, 100)
            .expect("read")
            .len(),
        1
    );
}

#[test]
fn duplicate_text_inside_the_window_is_dropped_but_accepted_after_it() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    assert!(AmbientStoreSql::insert_utterance(
        &connection,
        &utterance(
            "u-0",
            "ep-1",
            0,
            "2026-08-20T10:00:00.000Z",
            "повтор",
            "2026-08-27T10:00:00.000Z",
        ),
        "2026-08-20T09:59:00.000Z",
    )
    .expect("insert"));
    assert!(!AmbientStoreSql::insert_utterance(
        &connection,
        &utterance(
            "u-1",
            "ep-1",
            1,
            "2026-08-20T10:00:30.000Z",
            "повтор",
            "2026-08-27T10:00:00.000Z",
        ),
        "2026-08-20T09:59:30.000Z",
    )
    .expect("insert"));
    assert!(AmbientStoreSql::insert_utterance(
        &connection,
        &utterance(
            "u-2",
            "ep-1",
            2,
            "2026-08-20T11:00:00.000Z",
            "повтор",
            "2026-08-27T10:00:00.000Z",
        ),
        "2026-08-20T10:59:00.000Z",
    )
    .expect("insert"));
    assert_eq!(counters(&connection, "ep-1"), (2, 2_000));
}

#[test]
fn a_future_duplicate_does_not_hide_an_older_out_of_order_utterance() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    let future = utterance(
        "u-future",
        "ep-1",
        1,
        "2026-08-20T10:05:00.000Z",
        "повтор",
        "2026-08-27T10:00:00.000Z",
    );
    assert!(
        AmbientStoreSql::insert_utterance(&connection, &future, "2026-08-20T09:00:00.000Z")
            .expect("future insert")
    );
    let older = utterance(
        "u-older",
        "ep-1",
        0,
        "2026-08-20T10:04:00.000Z",
        "повтор",
        "2026-08-27T10:00:00.000Z",
    );
    assert!(
        AmbientStoreSql::insert_utterance(&connection, &older, "2026-08-20T10:00:00.000Z")
            .expect("older insert")
    );
}

#[test]
fn v1_stores_no_speaker_identity() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    let mut record = utterance(
        "u-0",
        "ep-1",
        0,
        "2026-08-20T10:00:00.000Z",
        "фраза",
        "2026-08-27T10:00:00.000Z",
    );
    record.speaker = "роман".to_owned();
    assert!(matches!(
        AmbientStoreSql::insert_utterance(&connection, &record, "2026-08-20T09:00:00.000Z"),
        Err(AmbientStoreError::InvalidSpeaker)
    ));
}

#[test]
fn store_owns_episode_counters_and_rejects_prefilled_values() {
    let connection = open();
    let mut record = episode(
        "ep-1",
        "2026-08-20T10:00:00.000Z",
        "2026-09-19T10:00:00.000Z",
    );
    record.utterance_count = 1;
    assert!(matches!(
        AmbientStoreSql::open_episode(&connection, &record),
        Err(AmbientStoreError::InvalidInitialCounters)
    ));
}

#[test]
fn deleting_an_episode_leaves_a_tombstone_and_no_orphans() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    AmbientStoreSql::insert_utterance(
        &connection,
        &utterance(
            "u-0",
            "ep-1",
            0,
            "2026-08-20T10:00:00.000Z",
            "фраза",
            "2026-08-27T10:00:00.000Z",
        ),
        "2026-08-20T09:00:00.000Z",
    )
    .expect("insert");
    append_event(
        &connection,
        "ep-1",
        "ambient.transcript",
        "2026-08-20T10:00:01.000Z",
    );
    append_event(
        &connection,
        "task-7",
        "task.started",
        "2026-08-20T10:00:02.000Z",
    );
    candidate(&connection, "mem-1", "ep-1", "pending_confirmation");
    candidate(&connection, "mem-2", "ep-1", "confirmed");

    let deletion = AmbientStoreSql::delete_episode(
        &connection,
        "ep-1",
        REASON_USER_REQUEST,
        "2026-08-20T12:00:00.000Z",
        "2026-09-19T12:00:00.000Z",
    )
    .expect("episode deletes");
    assert_eq!(deletion.episodes_removed, 1);
    assert_eq!(deletion.utterances_removed, 1);
    assert_eq!(deletion.tombstones_written, 1);
    assert_eq!(deletion.events_removed, 1);
    assert_eq!(deletion.candidates_rejected, 1);

    let tombstones = AmbientStoreSql::list_tombstones(&connection, 10).expect("read");
    assert_eq!(tombstones.len(), 1);
    assert_eq!(tombstones[0].episode_id, "ep-1");
    assert_eq!(tombstones[0].utterance_count, 1);
    assert_eq!(tombstones[0].reason, REASON_USER_REQUEST);

    let remaining: i64 = connection
        .query_row("SELECT COUNT(*) FROM ambient_utterances", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(remaining, 0);
    assert_eq!(
        event_types(&connection),
        vec!["task.started".to_owned()],
        "ambient-строки уходят, чужие события остаются"
    );
    assert_eq!(
        candidate_state(&connection, "mem-1"),
        (
            "rejected".to_owned(),
            Some(CANDIDATE_REJECTION_REASON.to_owned())
        )
    );
    assert_eq!(candidate_state(&connection, "mem-2").0, "confirmed");
}

#[test]
fn unknown_removal_reason_never_reaches_a_tombstone() {
    let connection = open();
    assert!(matches!(
        AmbientStoreSql::delete_episode(
            &connection,
            "ep-1",
            "потому что",
            "2026-08-20T12:00:00.000Z",
            "2026-09-19T12:00:00.000Z",
        ),
        Err(AmbientStoreError::InvalidReason)
    ));
}

#[test]
fn forget_window_spares_the_episode_that_only_crosses_its_border() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T09:50:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-2",
            "2026-08-20T10:30:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    // ep-1 наполовину внутри окна, ep-2 — целиком.
    for (id, episode_id, sequence, started_at, text) in [
        ("u-0", "ep-1", 0, "2026-08-20T09:51:00.000Z", "до окна"),
        ("u-1", "ep-1", 1, "2026-08-20T10:31:00.000Z", "в окне"),
        ("u-2", "ep-2", 0, "2026-08-20T10:32:00.000Z", "тоже в окне"),
    ] {
        AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                id,
                episode_id,
                sequence,
                started_at,
                text,
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:00:00.000Z",
        )
        .expect("insert");
    }
    candidate(&connection, "mem-1", "ep-1", "candidate");
    candidate(&connection, "mem-2", "ep-2", "candidate");
    append_event(
        &connection,
        "ep-2",
        "ambient.transcript",
        "2026-08-20T10:32:01.000Z",
    );
    append_event(
        &connection,
        "ambient-session",
        "ambient.state",
        "2026-08-20T10:31:00.000Z",
    );
    append_event(
        &connection,
        "ep-1",
        "ambient.transcript",
        "2026-08-20T09:51:01.000Z",
    );

    let deletion = AmbientStoreSql::forget_window(
        &connection,
        "2026-08-20T10:30:00.000Z",
        "2026-08-20T11:00:00.000Z",
        "2026-08-20T11:00:00.000Z",
        "2026-09-19T11:00:00.000Z",
    )
    .expect("window forgets");
    assert_eq!(deletion.utterances_removed, 2);
    assert_eq!(deletion.episodes_removed, 1, "пустой эпизод уходит целиком");
    assert_eq!(deletion.candidates_rejected, 2);
    assert_eq!(deletion.events_removed, 3);

    assert!(AmbientStoreSql::get_episode(&connection, "ep-1")
        .expect("read")
        .is_some());
    assert!(AmbientStoreSql::get_episode(&connection, "ep-2")
        .expect("read")
        .is_none());
    assert_eq!(counters(&connection, "ep-1"), (1, 1_000));
    assert_eq!(candidate_state(&connection, "mem-1").0, "rejected");
    assert_eq!(candidate_state(&connection, "mem-2").0, "rejected");
    assert_eq!(
        event_types(&connection),
        Vec::<String>::new(),
        "ambient-строки затронутых эпизодов не переживают forget"
    );
}

#[test]
fn a_journal_reader_walks_over_the_gap_left_by_forget() {
    let connection = open();
    for index in 0..5 {
        let event_type = if index % 2 == 0 {
            "ambient.state"
        } else {
            "task.progress"
        };
        append_event(
            &connection,
            "ambient-session",
            event_type,
            &format!("2026-08-20T10:0{index}:00.000Z"),
        );
    }
    AmbientStoreSql::forget_window(
        &connection,
        "2026-08-20T10:00:00.000Z",
        "2026-08-20T10:04:00.000Z",
        "2026-08-20T10:05:00.000Z",
        "2026-09-19T10:05:00.000Z",
    )
    .expect("window forgets");
    let mut cursor = 0_i64;
    let mut seen = Vec::new();
    loop {
        let next: Option<(i64, String)> = connection
            .query_row(
                "SELECT sequence_id, event_type FROM events
                     WHERE sequence_id > ?1 ORDER BY sequence_id LIMIT 1",
                params![cursor],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .expect("cursor read");
        let Some((sequence_id, event_type)) = next else {
            break;
        };
        assert!(sequence_id > cursor, "курсор монотонен");
        cursor = sequence_id;
        seen.push(event_type);
    }
    assert_eq!(seen, vec!["task.progress".to_owned(); 2]);
    assert_eq!(cursor, 4, "дырка в нумерации не останавливает чтение");
}

#[test]
fn retention_removes_exactly_what_expired_and_lets_tombstones_expire_too() {
    let connection = open();
    // ep-1: текст истёк, метаданные ещё живут.
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-01T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    AmbientStoreSql::insert_utterance(
        &connection,
        &utterance(
            "u-0",
            "ep-1",
            0,
            "2026-08-01T10:00:00.000Z",
            "истёкшее",
            "2026-08-08T10:00:00.000Z",
        ),
        "2026-08-01T09:00:00.000Z",
    )
    .expect("insert");
    AmbientStoreSql::insert_utterance(
        &connection,
        &utterance(
            "u-1",
            "ep-1",
            1,
            "2026-08-20T10:00:00.000Z",
            "свежее",
            "2026-08-27T10:00:00.000Z",
        ),
        "2026-08-20T09:00:00.000Z",
    )
    .expect("insert");
    // ep-2: истекли и метаданные.
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-2",
            "2026-07-01T10:00:00.000Z",
            "2026-07-31T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    candidate(&connection, "mem-2", "ep-2", "candidate");
    append_event(
        &connection,
        "ep-2",
        "ambient.transcript",
        "2026-07-01T10:00:01.000Z",
    );
    append_event(
        &connection,
        "ambient-session",
        "ambient.state",
        "2026-08-20T09:00:00.000Z",
    );
    append_event(
        &connection,
        "task-1",
        "task.started",
        "2026-07-01T09:00:00.000Z",
    );
    // Просроченный tombstone от прошлого удаления.
    connection
        .execute(
            "INSERT INTO ambient_tombstones
                 (tombstone_id, episode_id, removed_at, reason, utterance_count, expires_at)
                 VALUES ('old', 'ep-0', '2026-06-01T10:00:00.000Z', 'retention', 3,
                         '2026-07-01T10:00:00.000Z')",
            [],
        )
        .expect("stale tombstone");

    let purge = AmbientStoreSql::purge_expired(
        &connection,
        "2026-08-20T12:00:00.000Z",
        "2026-09-19T12:00:00.000Z",
        "2026-07-21T12:00:00.000Z",
    )
    .expect("purge runs");
    assert_eq!(purge.utterances_removed, 1);
    assert_eq!(purge.episodes_removed, 1);
    assert_eq!(purge.tombstones_written, 1);
    assert_eq!(purge.tombstones_removed, 1);
    assert_eq!(purge.candidates_rejected, 1);
    assert_eq!(purge.events_removed, 1);

    assert_eq!(counters(&connection, "ep-1"), (1, 1_000));
    assert!(AmbientStoreSql::get_episode(&connection, "ep-2")
        .expect("read")
        .is_none());
    let tombstones = AmbientStoreSql::list_tombstones(&connection, 10).expect("read");
    assert_eq!(tombstones.len(), 1);
    assert_eq!(tombstones[0].episode_id, "ep-2");
    assert_eq!(candidate_state(&connection, "mem-2").0, "rejected");
    assert_eq!(
        event_types(&connection),
        vec!["ambient.state".to_owned(), "task.started".to_owned()],
        "свежая ambient-строка и чужие события остаются"
    );

    // Повторный прогон на тех же данных ничего не меняет.
    let repeat = AmbientStoreSql::purge_expired(
        &connection,
        "2026-08-20T12:00:00.000Z",
        "2026-09-19T12:00:00.000Z",
        "2026-07-21T12:00:00.000Z",
    )
    .expect("purge repeats");
    assert_eq!(repeat, AmbientPurge::default());
}

#[test]
fn reads_stay_bounded_even_when_the_caller_asks_for_everything() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-20T10:00:00.000Z",
            "2026-09-19T10:00:00.000Z",
        ),
    )
    .expect("episode opens");
    for index in 0..(MAX_ROWS_PER_READ + 10) {
        AmbientStoreSql::insert_utterance(
            &connection,
            &utterance(
                &format!("u-{index}"),
                "ep-1",
                index as i64,
                "2026-08-20T10:00:00.000Z",
                &format!("фраза {index}"),
                "2026-08-27T10:00:00.000Z",
            ),
            "2026-08-20T09:00:00.000Z",
        )
        .expect("insert");
    }
    assert_eq!(
        AmbientStoreSql::list_utterances(&connection, "ep-1", usize::MAX)
            .expect("read")
            .len(),
        MAX_ROWS_PER_READ
    );
}

fn proposal(
    id: &str,
    key: &str,
    mute_key: &str,
    episode_id: Option<&str>,
    created_at: &str,
    expires_at: &str,
) -> AmbientProposalRecord {
    AmbientProposalRecord {
        proposal_id: id.to_owned(),
        proposal_key: key.to_owned(),
        mute_key: mute_key.to_owned(),
        kind: ProposalKind::Reminder,
        subject_key: "hleb".to_owned(),
        subject: "хлеб".to_owned(),
        title: "Напомнить купить хлеб".to_owned(),
        source_episode_id: episode_id.map(str::to_owned),
        source_deleted_at: None,
        source_deleted_reason: None,
        created_at: created_at.to_owned(),
        updated_at: created_at.to_owned(),
        expires_at: expires_at.to_owned(),
        occurrences: 1,
        state: ProposalState::Proposed,
        accepted_task_id: None,
        idempotency_key: None,
    }
}

/// Повтор той же темы в той же временной корзине поднимает счётчик
/// существующей карточки. Второй карточки не появляется: иначе за час
/// разговора об одном и том же очередь заполнилась бы копиями.
#[test]
fn a_duplicate_proposal_raises_a_counter_instead_of_creating_a_second_card() {
    let connection = open();
    let record = proposal(
        "p-1",
        "reminder:hleb:470000",
        "reminder:hleb",
        None,
        "2026-08-21T10:00:00.000Z",
        "2026-08-22T10:00:00.000Z",
    );
    assert_eq!(
        AmbientStoreSql::record_proposal(&connection, &record).unwrap(),
        ProposalInsert::Created
    );
    let again = AmbientProposalRecord {
        proposal_id: "p-2".to_owned(),
        updated_at: "2026-08-21T10:20:00.000Z".to_owned(),
        ..record.clone()
    };
    assert_eq!(
        AmbientStoreSql::record_proposal(&connection, &again).unwrap(),
        ProposalInsert::Duplicate {
            proposal_id: "p-1".to_owned(),
            occurrences: 2,
        }
    );
    let open_cards = AmbientStoreSql::list_open_proposals(&connection, 10).unwrap();
    assert_eq!(open_cards.len(), 1);
    assert_eq!(open_cards[0].occurrences, 2);
    assert!(AmbientStoreSql::get_proposal(&connection, "p-2")
        .unwrap()
        .is_none());
}

/// Mute идёт по ключу без времени, поэтому он продолжает действовать и
/// после смены временной корзины — то есть глушит предложение, чей
/// `proposal_key` уже другой.
#[test]
fn a_mute_outlives_the_time_bucket_it_was_set_in() {
    let connection = open();
    AmbientStoreSql::mute_subject(
        &connection,
        "reminder:hleb",
        ProposalKind::Reminder,
        "hleb",
        "2026-08-21T10:00:00.000Z",
    )
    .unwrap();
    let later = proposal(
        "p-9",
        "reminder:hleb:999999",
        "reminder:hleb",
        None,
        "2026-09-01T10:00:00.000Z",
        "2026-09-02T10:00:00.000Z",
    );
    assert_eq!(
        AmbientStoreSql::record_proposal(&connection, &later).unwrap(),
        ProposalInsert::Muted
    );
    assert!(AmbientStoreSql::list_open_proposals(&connection, 10)
        .unwrap()
        .is_empty());
    assert_eq!(
        AmbientStoreSql::list_mute_keys(&connection).unwrap(),
        vec!["reminder:hleb".to_owned()]
    );
}

/// Истёкшее предложение освобождает тему: новая корзина времени даёт
/// новый `proposal_key`, и `UNIQUE` ему не мешает.
#[test]
fn a_new_proposal_after_expiry_does_not_hit_the_unique_key() {
    let connection = open();
    let first = proposal(
        "p-1",
        "reminder:hleb:470000",
        "reminder:hleb",
        None,
        "2026-08-21T10:00:00.000Z",
        "2026-08-22T10:00:00.000Z",
    );
    AmbientStoreSql::record_proposal(&connection, &first).unwrap();
    assert_eq!(
        AmbientStoreSql::expire_stale_proposals(&connection, "2026-08-22T10:00:00.000Z").unwrap(),
        1
    );
    assert!(AmbientStoreSql::list_open_proposals(&connection, 10)
        .unwrap()
        .is_empty());
    let second = proposal(
        "p-2",
        "reminder:hleb:470024",
        "reminder:hleb",
        None,
        "2026-08-22T11:00:00.000Z",
        "2026-08-23T11:00:00.000Z",
    );
    assert_eq!(
        AmbientStoreSql::record_proposal(&connection, &second).unwrap(),
        ProposalInsert::Created
    );
}

/// Удаление эпизода-источника обязано пометить его предложения истёкшими
/// с причиной `source_deleted`, а не оставить их с обнулённой ссылкой.
#[test]
fn deleting_the_source_episode_expires_its_proposals_instead_of_orphaning_them() {
    let connection = open();
    AmbientStoreSql::open_episode(
        &connection,
        &episode(
            "ep-1",
            "2026-08-21T10:00:00.000Z",
            "2026-09-20T10:00:00.000Z",
        ),
    )
    .unwrap();
    AmbientStoreSql::record_proposal(
        &connection,
        &proposal(
            "p-1",
            "reminder:hleb:470000",
            "reminder:hleb",
            Some("ep-1"),
            "2026-08-21T10:00:00.000Z",
            "2026-08-22T10:00:00.000Z",
        ),
    )
    .unwrap();
    let deletion = AmbientStoreSql::delete_episode(
        &connection,
        "ep-1",
        REASON_USER_REQUEST,
        "2026-08-21T11:00:00.000Z",
        "2026-09-20T11:00:00.000Z",
    )
    .unwrap();
    assert_eq!(deletion.proposals_expired, 1);
    let stored = AmbientStoreSql::get_proposal(&connection, "p-1")
        .unwrap()
        .expect("предложение переживает удаление источника как след, а не как карточка");
    assert_eq!(stored.state, ProposalState::Expired);
    assert_eq!(
        stored.source_deleted_reason.as_deref(),
        Some(CANDIDATE_REJECTION_REASON)
    );
    assert_eq!(
        stored.source_deleted_at.as_deref(),
        Some("2026-08-21T11:00:00.000Z")
    );
    assert!(AmbientStoreSql::list_open_proposals(&connection, 10)
        .unwrap()
        .is_empty());
}

/// Второй `resolve` того же предложения не проходит: терминальное
/// состояние не переигрывается, и повторный клик не создаёт вторую задачу.
#[test]
fn a_resolved_proposal_never_moves_again() {
    let connection = open();
    AmbientStoreSql::record_proposal(
        &connection,
        &proposal(
            "p-1",
            "reminder:hleb:470000",
            "reminder:hleb",
            None,
            "2026-08-21T10:00:00.000Z",
            "2026-08-22T10:00:00.000Z",
        ),
    )
    .unwrap();
    assert!(AmbientStoreSql::resolve_proposal(
        &connection,
        "p-1",
        ProposalState::Accepted,
        "2026-08-21T10:05:00.000Z",
        Some("task-1"),
        Some("idem-1"),
    )
    .unwrap());
    assert!(!AmbientStoreSql::resolve_proposal(
        &connection,
        "p-1",
        ProposalState::Declined,
        "2026-08-21T10:06:00.000Z",
        None,
        Some("idem-2"),
    )
    .unwrap());
    let replay = AmbientStoreSql::find_proposal_by_idempotency(&connection, "idem-1")
        .unwrap()
        .expect("повтор с тем же ключом находит первое решение");
    assert_eq!(replay.state, ProposalState::Accepted);
    assert_eq!(replay.accepted_task_id.as_deref(), Some("task-1"));
}

/// Счётчики окна переживают рестарт: без строки в таблице перезапуск Core
/// обнулял бы часовой потолок.
#[test]
fn proactivity_counters_round_trip_through_their_row() {
    let connection = open();
    assert_eq!(AmbientStoreSql::load_counters(&connection).unwrap(), None);
    let row = ProactivityCountersRow {
        hour_started_at_ms: 1_770_000_000_000,
        hour_count: 2,
        day_started_at_ms: 1_769_990_000_000,
        day_count: 5,
        last_proposed_at_ms: Some(1_770_000_500_000),
    };
    AmbientStoreSql::save_counters(&connection, row).unwrap();
    assert_eq!(
        AmbientStoreSql::load_counters(&connection).unwrap(),
        Some(row)
    );
    let updated = ProactivityCountersRow {
        hour_count: 3,
        ..row
    };
    AmbientStoreSql::save_counters(&connection, updated).unwrap();
    assert_eq!(
        AmbientStoreSql::load_counters(&connection).unwrap(),
        Some(updated)
    );
}

/// Половинчатая пара `source_deleted_*` отвергается до SQL: «источник
/// удалён неизвестно когда» — не состояние, а порча данных.
#[test]
fn a_half_filled_source_deletion_is_rejected() {
    let connection = open();
    let broken = AmbientProposalRecord {
        source_deleted_reason: Some(CANDIDATE_REJECTION_REASON.to_owned()),
        ..proposal(
            "p-1",
            "reminder:hleb:470000",
            "reminder:hleb",
            None,
            "2026-08-21T10:00:00.000Z",
            "2026-08-22T10:00:00.000Z",
        )
    };
    assert!(matches!(
        AmbientStoreSql::record_proposal(&connection, &broken),
        Err(AmbientStoreError::InvalidSourceDeletion)
    ));
}
