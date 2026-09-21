use crate::{execution_ledger, LocalDatabase, StorageError};
use rusqlite::OptionalExtension;

impl LocalDatabase {
    /// Публикует один bounded `core_start` event для нового Core instance
    /// (план 08-2 п.5). Вызывается ровно один раз при старте, до любой
    /// reconciliation-классификации.
    pub fn record_core_start(&self, core_instance_id: &str) -> Result<i64, StorageError> {
        let event = execution_ledger::ExecutionEventV1 {
            schema_version: 1,
            event_id: uuid::Uuid::now_v7().to_string(),
            sequence_id: None,
            run_scope: execution_ledger::RunScope::System,
            run_id: String::new(),
            session_id: None,
            task_id: "core".to_string(),
            created_at_ms: 0,
            state_after: None,
            action_id: None,
            tool_call_id: None,
            observation_id: None,
            receipt_id: None,
            failure_id: None,
            workflow_run_id: None,
            node_id: None,
            attempt_id: None,
            effect_id: None,
            model_request_id: None,
            body: execution_ledger::ExecutionEventBody::RecoveryDecision {
                decision: "core_start".to_string(),
                evidence_digest: execution_ledger::legacy_event_id(
                    0,
                    core_instance_id,
                    "core_start",
                    core_instance_id.as_bytes(),
                    "",
                ),
            },
            redaction: execution_ledger::RedactionMeta::default(),
        };
        self.append_ledger_event(&event)
    }

    /// Reconciliation при старте Core (план 08-2 п.5): по каждому
    /// нетерминальному action с известным `effect_id` смотрит на dispatch
    /// marker в `run_effects` и, если он открыт (started без completed),
    /// публикует НОВОЕ `unknown_outcome`-событие — исходная строка не
    /// переписывается (единственный terminal outcome гарантируется
    /// `execution_ledger::assert_single_terminal` на стороне читателя).
    /// Pre-dispatch (marker отсутствует) и `waiting_approval` не трогаются —
    /// они уже безопасны по контракту.
    pub fn reconcile_ledger_on_startup(
        &self,
    ) -> Result<Vec<(String, execution_ledger::ActionState)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT e.action_id, e.effect_id, e.state_after, e.task_id, e.run_scope,
                    e.run_id, e.session_id, e.workflow_run_id
               FROM events e
               JOIN (
                   SELECT action_id, MAX(sequence_id) AS latest_sequence
                     FROM events
                    WHERE action_id IS NOT NULL AND state_after IS NOT NULL
                    GROUP BY action_id
               ) latest ON latest.action_id = e.action_id
                       AND latest.latest_sequence = e.sequence_id",
        )?;
        struct LedgerActionRow {
            action_id: String,
            effect_id: Option<String>,
            state_after: String,
            task_id: String,
            run_scope: Option<String>,
            run_id: Option<String>,
            session_id: Option<String>,
            workflow_run_id: Option<String>,
        }
        let rows: Vec<LedgerActionRow> = statement
            .query_map([], |row| {
                Ok(LedgerActionRow {
                    action_id: row.get(0)?,
                    effect_id: row.get(1)?,
                    state_after: row.get(2)?,
                    task_id: row.get(3)?,
                    run_scope: row.get(4)?,
                    run_id: row.get(5)?,
                    session_id: row.get(6)?,
                    workflow_run_id: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut reconciled = Vec::new();
        let mut reconciliation_events = Vec::new();
        for row in rows {
            let Some(previous) = execution_ledger::ActionState::parse(&row.state_after) else {
                continue;
            };
            let Some(effect_id) = row.effect_id else {
                continue;
            };
            // Строки типизированного ledger всегда несут run_scope/run_id —
            // их проставляет append_ledger_event; отсутствие означает, что
            // это не typed row (не должно случиться при action_id IS NOT
            // NULL, но пропускаем, а не паникуем).
            let (Some(run_scope), Some(run_id)) = (row.run_scope, row.run_id) else {
                continue;
            };
            let Some(run_scope) = execution_ledger::RunScope::parse(&run_scope) else {
                continue;
            };
            let marker: Option<(Option<String>, Option<String>)> = self
                .connection
                .query_row(
                    "SELECT started_at, completed_at FROM run_effects WHERE effect_id = ?1",
                    [&effect_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let status = match marker {
                None => execution_ledger::DispatchMarkerStatus::Absent,
                Some((_, Some(_))) => execution_ledger::DispatchMarkerStatus::Completed,
                Some((Some(_), None)) => {
                    execution_ledger::DispatchMarkerStatus::StartedNotCompleted
                }
                Some((None, None)) => execution_ledger::DispatchMarkerStatus::Absent,
            };
            let Some(new_state) = execution_ledger::reconcile_action_state(previous, status) else {
                continue;
            };
            let event = execution_ledger::ExecutionEventV1 {
                schema_version: 1,
                event_id: uuid::Uuid::now_v7().to_string(),
                sequence_id: None,
                run_scope,
                run_id,
                session_id: row.session_id,
                task_id: row.task_id,
                created_at_ms: 0,
                state_after: Some(new_state),
                action_id: Some(row.action_id.clone()),
                tool_call_id: None,
                observation_id: None,
                receipt_id: None,
                failure_id: None,
                workflow_run_id: row.workflow_run_id,
                node_id: None,
                attempt_id: None,
                effect_id: Some(effect_id),
                model_request_id: None,
                body: execution_ledger::ExecutionEventBody::RecoveryDecision {
                    decision: "startup_reconciliation".to_string(),
                    evidence_digest: row.action_id.clone(),
                },
                redaction: execution_ledger::RedactionMeta::default(),
            };
            reconciliation_events.push(event);
            reconciled.push((row.action_id, new_state));
        }
        self.append_ledger_events(&reconciliation_events)?;
        Ok(reconciled)
    }
}
