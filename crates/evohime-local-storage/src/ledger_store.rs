use crate::ledger_helpers::{
    ensure_single_terminal_outcome, insert_ledger_event_row, insert_ledger_event_row_cached,
};
use crate::{execution_ledger, workflow_store, LocalDatabase, StorageError};
use rusqlite::OptionalExtension;

impl LocalDatabase {
    /// Публикует один typed execution ledger event (план 08-1/08-2). Валидный
    /// self-consistency контракт (`event.validate()`) проверяется до записи;
    /// `payload` хранит канонический JSON события — старые читатели видят
    /// его как непрозрачный BLOB, `event_type` несёт стабильный
    /// `body.kind_str()` для обратной совместимости с generic-путём чтения.
    pub fn append_ledger_event(
        &self,
        event: &execution_ledger::ExecutionEventV1,
    ) -> Result<i64, StorageError> {
        event.validate()?;
        ensure_single_terminal_outcome(&self.connection, event)?;
        insert_ledger_event_row(&self.connection, event)
    }

    /// Публикует несколько typed ledger events одной транзакцией. Внутри
    /// транзакции INSERT и проверка terminal outcome используют cached
    /// prepared statements, поэтому batch не создаёт отдельные BEGIN/COMMIT
    /// и не компилирует один и тот же SQL для каждого события.
    pub fn append_ledger_events(
        &self,
        events: &[execution_ledger::ExecutionEventV1],
    ) -> Result<Vec<i64>, StorageError> {
        if events.is_empty() {
            return Ok(Vec::new());
        }
        for event in events {
            event.validate()?;
        }

        let transaction = self.connection.unchecked_transaction()?;
        let mut terminal_check = transaction.prepare_cached(
            "SELECT state_after FROM events
              WHERE action_id = ?1 AND state_after IS NOT NULL
              ORDER BY sequence_id DESC LIMIT 1",
        )?;
        let mut insert = transaction.prepare_cached(
            "INSERT INTO events(
                task_id, event_type, payload, event_id, schema_version, run_scope,
                run_id, session_id, action_id, effect_id, workflow_run_id, state_after
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?;

        let mut sequence_ids = Vec::with_capacity(events.len());
        for event in events {
            if let (Some(action_id), Some(state)) = (event.action_id.as_deref(), event.state_after)
            {
                if state.is_terminal() {
                    let previous: Option<String> = terminal_check
                        .query_row([action_id], |row| row.get(0))
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
                }
            }
            sequence_ids.push(insert_ledger_event_row_cached(
                &mut insert,
                &transaction,
                event,
            )?);
        }
        drop(insert);
        drop(terminal_check);
        transaction.commit()?;
        Ok(sequence_ids)
    }

    /// Атомарно публикует typed ledger event и переводит связанный
    /// `workflow_run_nodes` узел из `from` в `to` — одна SQLite-транзакция,
    /// один `commit()`. Незаконный переход или узел, уже покинувший `from`
    /// (гонка/устаревший вызов), откатывает обе части: строка `events` не
    /// появляется. Тот же SQL-guard, что `workflow_store::update_node_state`.
    pub fn append_ledger_event_with_node_transition(
        &self,
        event: &execution_ledger::ExecutionEventV1,
        run_id: &str,
        node_id: &str,
        from: execution_ledger::ActionState,
        to: execution_ledger::ActionState,
        now_ms: i64,
    ) -> Result<i64, StorageError> {
        execution_ledger::validate_transition(from, to)?;
        event.validate()?;
        let transaction = self.connection.unchecked_transaction()?;
        ensure_single_terminal_outcome(&transaction, event)?;
        let changed = transaction.execute(
            "UPDATE workflow_run_nodes SET state = ?3, updated_at_ms = ?4
              WHERE run_id = ?1 AND node_id = ?2 AND state = ?5",
            rusqlite::params![run_id, node_id, to.as_str(), now_ms, from.as_str()],
        )?;
        if changed == 0 {
            return Err(StorageError::LedgerNodeTransitionConflict {
                run_id: run_id.to_string(),
                node_id: node_id.to_string(),
            });
        }
        let sequence_id = insert_ledger_event_row(&transaction, event)?;
        // План 08-2/08-4: связывает per-run bounded projection
        // (`workflow_run_events.run_sequence`) с только что записанной
        // глобальной строкой ledger — в той же транзакции, тем же commit.
        let payload_json = serde_json::to_string(event)?;
        workflow_store::append_event_linked(
            &transaction,
            workflow_store::AppendEventLinkedInput {
                run_id,
                node_id,
                attempt_id: event.attempt_id.as_deref().unwrap_or(""),
                event_type: event.body.kind_str(),
                payload_json: &payload_json,
                now_ms,
                ledger_sequence_id: sequence_id,
                ledger_event_id: &event.event_id,
            },
        )?;
        transaction.commit()?;
        Ok(sequence_id)
    }
}
