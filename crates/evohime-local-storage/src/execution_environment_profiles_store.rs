//! Durable metadata for execution environment profiles.
//!
//! The Core validates every record.  This store intentionally persists only
//! bounded, redacted JSON snapshots and never becomes an owner of model,
//! credential, tool, skill, or policy state.

use rusqlite::{params, Connection, OptionalExtension};

pub const MAX_RECORD_BYTES: usize = 64 * 1024;

pub fn install_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS execution_environment_profiles (
            id TEXT PRIMARY KEY NOT NULL, revision INTEGER NOT NULL,
            scope TEXT NOT NULL, state TEXT NOT NULL, content_hash TEXT NOT NULL,
            profile_json BLOB NOT NULL, updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_profile_revisions (
            profile_id TEXT NOT NULL, revision INTEGER NOT NULL,
            content_hash TEXT NOT NULL, profile_json BLOB NOT NULL,
            actor TEXT NOT NULL, created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(profile_id, revision)
        );
        CREATE TABLE IF NOT EXISTS execution_environment_activations (
            id INTEGER PRIMARY KEY AUTOINCREMENT, profile_id TEXT NOT NULL,
            revision INTEGER NOT NULL, scope TEXT NOT NULL, status TEXT NOT NULL,
            snapshot_hash TEXT NOT NULL, activation_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_current (
            scope TEXT PRIMARY KEY NOT NULL, profile_id TEXT NOT NULL,
            revision INTEGER NOT NULL, snapshot_json BLOB NOT NULL,
            updated_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_run_snapshots (
            run_id TEXT PRIMARY KEY NOT NULL, scope TEXT NOT NULL,
            profile_id TEXT NOT NULL, revision INTEGER NOT NULL,
            snapshot_hash TEXT NOT NULL, snapshot_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS execution_environment_idempotency (
            owner_scope TEXT NOT NULL, idempotency_key TEXT NOT NULL,
            command_hash TEXT NOT NULL, response_json BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            PRIMARY KEY(owner_scope, idempotency_key)
        );
        CREATE INDEX IF NOT EXISTS idx_execution_environment_activations_scope
            ON execution_environment_activations(scope, id DESC);",
    )
}

pub fn load_idempotent_command(
    connection: &Connection,
    owner_scope: &str,
    idempotency_key: &str,
) -> rusqlite::Result<Option<(String, Vec<u8>)>> {
    connection.query_row(
        "SELECT command_hash,response_json FROM execution_environment_idempotency WHERE owner_scope=?1 AND idempotency_key=?2",
        params![owner_scope, idempotency_key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()
}

pub fn save_idempotent_command(
    connection: &Connection,
    owner_scope: &str,
    idempotency_key: &str,
    command_hash: &str,
    response: &[u8],
    now_ms: i64,
) -> rusqlite::Result<()> {
    if response.len() > MAX_RECORD_BYTES {
        return Err(rusqlite::Error::InvalidQuery);
    }
    connection.execute(
        "INSERT INTO execution_environment_idempotency(owner_scope,idempotency_key,command_hash,response_json,created_at_ms) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(owner_scope,idempotency_key) DO NOTHING",
        params![owner_scope, idempotency_key, command_hash, response, now_ms],
    )?;
    Ok(())
}

pub struct SaveProfileRevisionInput<'a> {
    pub id: &'a str,
    pub revision: u64,
    pub scope: &'a str,
    pub state: &'a str,
    pub hash: &'a str,
    pub json: &'a [u8],
    pub actor: &'a str,
    pub now_ms: i64,
}

pub fn save_profile_revision(
    connection: &Connection,
    input: SaveProfileRevisionInput<'_>,
) -> rusqlite::Result<bool> {
    if input.json.len() > MAX_RECORD_BYTES {
        return Ok(false);
    }
    let tx = connection.unchecked_transaction()?;
    let current: Option<u64> = tx
        .query_row(
            "SELECT revision FROM execution_environment_profiles WHERE id=?1",
            [input.id],
            |row| row.get(0),
        )
        .optional()?;
    if current.is_some_and(|value| value >= input.revision) {
        return Ok(false);
    }
    let inserted = tx.execute("INSERT INTO execution_environment_profile_revisions(profile_id,revision,content_hash,profile_json,actor,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(profile_id,revision) DO NOTHING", params![input.id, input.revision as i64, input.hash, input.json, input.actor, input.now_ms])?;
    if inserted == 0 {
        return Ok(false);
    }
    tx.execute("INSERT INTO execution_environment_profiles(id,revision,scope,state,content_hash,profile_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET revision=excluded.revision,scope=excluded.scope,state=excluded.state,content_hash=excluded.content_hash,profile_json=excluded.profile_json,updated_at_ms=excluded.updated_at_ms", params![input.id, input.revision as i64, input.scope, input.state, input.hash, input.json, input.now_ms])?;
    tx.commit()?;
    Ok(true)
}

pub fn load_profile(connection: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT profile_json FROM execution_environment_profiles WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .optional()
}

pub fn load_profiles(connection: &Connection, limit: usize) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection
        .prepare("SELECT profile_json FROM execution_environment_profiles ORDER BY id LIMIT ?1")?;
    let records = statement
        .query_map([limit.min(256) as i64], |row| row.get(0))?
        .collect();
    records
}

pub struct SaveActivationInput<'a> {
    pub profile_id: &'a str,
    pub revision: u64,
    pub scope: &'a str,
    pub status: &'a str,
    pub snapshot_hash: &'a str,
    pub activation_json: &'a [u8],
    pub snapshot_json: &'a [u8],
    pub now_ms: i64,
}

pub fn save_activation(
    connection: &Connection,
    input: SaveActivationInput<'_>,
) -> rusqlite::Result<bool> {
    if input.activation_json.len() > MAX_RECORD_BYTES
        || input.snapshot_json.len() > MAX_RECORD_BYTES
    {
        return Ok(false);
    }
    let tx = connection.unchecked_transaction()?;
    tx.execute("INSERT INTO execution_environment_activations(profile_id,revision,scope,status,snapshot_hash,activation_json,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![input.profile_id, input.revision as i64, input.scope, input.status, input.snapshot_hash, input.activation_json, input.now_ms])?;
    tx.execute("INSERT INTO execution_environment_current(scope,profile_id,revision,snapshot_json,updated_at_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(scope) DO UPDATE SET profile_id=excluded.profile_id,revision=excluded.revision,snapshot_json=excluded.snapshot_json,updated_at_ms=excluded.updated_at_ms WHERE excluded.revision > execution_environment_current.revision", params![input.scope, input.profile_id, input.revision as i64, input.snapshot_json, input.now_ms])?;
    tx.commit()?;
    Ok(true)
}

pub fn load_current(connection: &Connection, scope: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM execution_environment_current WHERE scope=?1",
            [scope],
            |row| row.get(0),
        )
        .optional()
}

pub fn load_activations(
    connection: &Connection,
    scope: &str,
    limit: usize,
) -> rusqlite::Result<Vec<Vec<u8>>> {
    let mut statement = connection.prepare("SELECT activation_json FROM execution_environment_activations WHERE scope=?1 ORDER BY id DESC LIMIT ?2")?;
    let records = statement
        .query_map(params![scope, limit.min(256) as i64], |row| row.get(0))?
        .collect();
    records
}

/// Pins the current environment to a newly-created run. `INSERT OR IGNORE`
/// deliberately preserves the original snapshot on retries and recovery.
pub fn bind_current_to_run(
    connection: &Connection,
    run_id: &str,
    scope: &str,
    now_ms: i64,
) -> rusqlite::Result<bool> {
    let Some(snapshot_json) = load_current(connection, scope)? else {
        return Ok(false);
    };
    let value: serde_json::Value = serde_json::from_slice(&snapshot_json)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    let profile_id = value
        .get("profile_id")
        .and_then(serde_json::Value::as_str)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let revision = value
        .get("profile_revision")
        .and_then(serde_json::Value::as_u64)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    let snapshot_hash = value
        .get("snapshot_hash")
        .and_then(serde_json::Value::as_str)
        .ok_or(rusqlite::Error::InvalidQuery)?;
    Ok(connection.execute("INSERT OR IGNORE INTO execution_environment_run_snapshots(run_id,scope,profile_id,revision,snapshot_hash,snapshot_json,created_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7)", params![run_id, scope, profile_id, revision as i64, snapshot_hash, snapshot_json, now_ms])? == 1)
}

pub fn load_run_snapshot(
    connection: &Connection,
    run_id: &str,
) -> rusqlite::Result<Option<Vec<u8>>> {
    connection
        .query_row(
            "SELECT snapshot_json FROM execution_environment_run_snapshots WHERE run_id=?1",
            [run_id],
            |row| row.get(0),
        )
        .optional()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revisions_are_monotonic_and_activation_is_atomic() {
        let db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        assert!(save_profile_revision(
            &db,
            SaveProfileRevisionInput {
                id: "p",
                revision: 1,
                scope: "application:a",
                state: "ready",
                hash: "h",
                json: br#"{}"#,
                actor: "core",
                now_ms: 1
            }
        )
        .unwrap());
        assert!(!save_profile_revision(
            &db,
            SaveProfileRevisionInput {
                id: "p",
                revision: 1,
                scope: "application:a",
                state: "ready",
                hash: "h",
                json: br#"{}"#,
                actor: "core",
                now_ms: 1
            }
        )
        .unwrap());
        assert!(save_activation(
            &db,
            SaveActivationInput {
                profile_id: "p",
                revision: 1,
                scope: "application:a",
                status: "ready",
                snapshot_hash: "snap",
                activation_json: br#"{}"#,
                snapshot_json: br#"{}"#,
                now_ms: 2
            }
        )
        .unwrap());
        assert_eq!(
            load_current(&db, "application:a").unwrap(),
            Some(br#"{}"#.to_vec())
        );
    }

    #[test]
    fn existing_history_without_current_profile_is_not_rewritten_or_rejected() {
        let db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        db.execute(
            "INSERT INTO execution_environment_profile_revisions
             (profile_id, revision, content_hash, profile_json, actor, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "p",
                3_i64,
                "original",
                br#"{"original":true}"#,
                "old-core",
                10_i64
            ],
        )
        .unwrap();

        assert!(!save_profile_revision(
            &db,
            SaveProfileRevisionInput {
                id: "p",
                revision: 3,
                scope: "application:a",
                state: "ready",
                hash: "replacement",
                json: br#"{"original":false}"#,
                actor: "new-core",
                now_ms: 20,
            },
        )
        .unwrap());
        assert_eq!(
            db.query_row(
                "SELECT content_hash, profile_json FROM execution_environment_profile_revisions
                 WHERE profile_id = ?1 AND revision = ?2",
                params!["p", 3_i64],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .unwrap(),
            ("original".to_owned(), br#"{"original":true}"#.to_vec())
        );
    }

    #[test]
    fn run_binding_pins_the_first_current_snapshot() {
        let db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        let snapshot = br#"{"profile_id":"p","profile_revision":1,"snapshot_hash":"h"}"#;
        assert!(save_activation(
            &db,
            SaveActivationInput {
                profile_id: "p",
                revision: 1,
                scope: "application:application",
                status: "activated",
                snapshot_hash: "h",
                activation_json: br#"{}"#,
                snapshot_json: snapshot,
                now_ms: 1
            }
        )
        .unwrap());
        assert!(bind_current_to_run(&db, "run-1", "application:application", 2).unwrap());
        assert!(!bind_current_to_run(&db, "run-1", "application:application", 3).unwrap());
        assert_eq!(
            load_run_snapshot(&db, "run-1").unwrap(),
            Some(snapshot.to_vec())
        );
    }

    #[test]
    fn activation_revision_fence_rejects_stale_current_snapshot() {
        let db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        assert!(save_activation(
            &db,
            SaveActivationInput {
                profile_id: "new",
                revision: 2,
                scope: "application:application",
                status: "activated",
                snapshot_hash: "new-hash",
                activation_json: br#"{}"#,
                snapshot_json: br#"{\"revision\":2}"#,
                now_ms: 2
            }
        )
        .unwrap());
        assert!(save_activation(
            &db,
            SaveActivationInput {
                profile_id: "old",
                revision: 1,
                scope: "application:application",
                status: "rolled_back",
                snapshot_hash: "old-hash",
                activation_json: br#"{}"#,
                snapshot_json: br#"{\"revision\":1}"#,
                now_ms: 3
            }
        )
        .unwrap());
        assert_eq!(
            load_current(&db, "application:application").unwrap(),
            Some(br#"{\"revision\":2}"#.to_vec())
        );
    }

    #[test]
    fn duplicate_activation_revision_cannot_replace_current_snapshot() {
        let db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        assert!(save_activation(
            &db,
            SaveActivationInput {
                profile_id: "p",
                revision: 2,
                scope: "application:application",
                status: "activated",
                snapshot_hash: "first",
                activation_json: br#"{}"#,
                snapshot_json: br#"{"revision":2,"source":"first"}"#,
                now_ms: 2,
            },
        )
        .unwrap());
        assert!(save_activation(
            &db,
            SaveActivationInput {
                profile_id: "p",
                revision: 2,
                scope: "application:application",
                status: "replayed",
                snapshot_hash: "replacement",
                activation_json: br#"{"retry":true}"#,
                snapshot_json: br#"{"revision":2,"source":"replacement"}"#,
                now_ms: 3,
            },
        )
        .unwrap());
        assert_eq!(
            load_current(&db, "application:application").unwrap(),
            Some(br#"{"revision":2,"source":"first"}"#.to_vec())
        );
    }

    #[test]
    fn idempotency_key_replays_only_the_same_command() {
        let db = Connection::open_in_memory().unwrap();
        install_schema(&db).unwrap();
        save_idempotent_command(
            &db,
            "application:application",
            "create-1",
            "a",
            br#"{"ok":true}"#,
            1,
        )
        .unwrap();
        assert_eq!(
            load_idempotent_command(&db, "application:application", "create-1").unwrap(),
            Some(("a".into(), br#"{"ok":true}"#.to_vec()))
        );
        save_idempotent_command(
            &db,
            "application:application",
            "create-1",
            "different",
            br#"{"ok":false}"#,
            2,
        )
        .unwrap();
        assert_eq!(
            load_idempotent_command(&db, "application:application", "create-1").unwrap(),
            Some(("a".into(), br#"{"ok":true}"#.to_vec()))
        );
    }
}
