use rusqlite::{Connection, OptionalExtension, Statement};

use crate::{execution_ledger, StorageError};

/// Shared write path for typed ledger rows, used by both `append_ledger_event`
/// and `append_ledger_event_with_node_transition` so the two INSERT column
/// lists cannot drift apart. Takes `&Connection` so callers can pass either
/// the bare connection or an in-flight `Transaction` (which derefs to it).
pub(crate) fn insert_ledger_event_row(
    connection: &Connection,
    event: &execution_ledger::ExecutionEventV1,
) -> Result<i64, StorageError> {
    let payload = serde_json::to_vec(event)?;
    connection.execute(
        "INSERT INTO events(
            task_id, event_type, payload, event_id, schema_version, run_scope,
            run_id, session_id, action_id, effect_id, workflow_run_id, state_after
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            event.task_id,
            event.body.kind_str(),
            payload,
            event.event_id,
            event.schema_version,
            event.run_scope.as_str(),
            event.run_id,
            event.session_id,
            event.action_id,
            event.effect_id,
            event.workflow_run_id,
            event.state_after.map(|state| state.as_str()),
        ],
    )?;
    Ok(connection.last_insert_rowid())
}

pub(crate) fn insert_ledger_event_row_cached(
    statement: &mut Statement<'_>,
    connection: &Connection,
    event: &execution_ledger::ExecutionEventV1,
) -> Result<i64, StorageError> {
    let payload = serde_json::to_vec(event)?;
    statement.execute(rusqlite::params![
        event.task_id,
        event.body.kind_str(),
        payload,
        event.event_id,
        event.schema_version,
        event.run_scope.as_str(),
        event.run_id,
        event.session_id,
        event.action_id,
        event.effect_id,
        event.workflow_run_id,
        event.state_after.map(|state| state.as_str()),
    ])?;
    Ok(connection.last_insert_rowid())
}

/// Enforces "a terminal action never gets a second terminal outcome" at
/// write time (план 08-1's `assert_single_terminal`, applied per-action
/// against durable history rather than an in-memory batch). A no-op when
/// the event carries no `action_id` or its `state_after` is not terminal —
/// non-terminal transitions and system/legacy rows are unaffected.
pub(crate) fn ensure_single_terminal_outcome(
    connection: &Connection,
    event: &execution_ledger::ExecutionEventV1,
) -> Result<(), StorageError> {
    let (Some(action_id), Some(state)) = (event.action_id.as_deref(), event.state_after) else {
        return Ok(());
    };
    if !state.is_terminal() {
        return Ok(());
    }
    let previous: Option<String> = connection
        .query_row(
            "SELECT state_after FROM events
              WHERE action_id = ?1 AND state_after IS NOT NULL
              ORDER BY sequence_id DESC LIMIT 1",
            [action_id],
            |row| row.get(0),
        )
        .optional()?;
    let already_terminal = previous
        .as_deref()
        .and_then(execution_ledger::ActionState::parse)
        .is_some_and(execution_ledger::ActionState::is_terminal);
    if already_terminal {
        return Err(StorageError::LedgerContract(
            execution_ledger::LedgerContractError::DuplicateTerminalOutcome {
                action_id: action_id.to_string(),
            },
        ));
    }
    Ok(())
}
