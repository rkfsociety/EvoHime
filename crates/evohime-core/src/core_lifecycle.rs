/// Подключает permission-аудит к локальному append-only журналу Core.
///
/// PermissionEngine сохраняет короткий bounded-журнал для быстрых проверок,
/// а этот sink делает те же переходы durable и доступными через историю задачи.
pub async fn attach_permission_audit_sink(
    journal: EventJournal,
    tools: &std::sync::Arc<ToolRegistry>,
) -> tokio::task::JoinHandle<()> {
    let (sender, mut receiver) = mpsc::unbounded_channel();
    tools.permissions().attach_audit_sender(sender).await;
    tokio::spawn(async move {
        while let Some(entry) = receiver.recv().await {
            let Ok(payload) = serde_json::to_vec(&entry) else {
                continue;
            };
            let _ = journal
                .record_audit(&entry.task_id.to_string(), "approval.audit", &payload)
                .await;
        }
    })
}

/// Periodically purges terminal `receipt_approval_intents` rows past their
/// retention window (01.3 ApprovalGC). `ReceiptRuntime::approval_gc` already
/// re-checks the recovery guard phase/generation inside its own short
/// transaction on every call, so calling it unconditionally on a timer is
/// safe even while Recovery is still running — it will simply no-op.
pub fn spawn_approval_gc(
    journal: EventJournal,
    keys: Arc<ReceiptKeyManager>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            let now_ms = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_millis() as i64)
                .unwrap_or_default();
            let mut database = journal.database().lock().await;
            let signer = CoreReceiptSigner(Arc::clone(&keys));
            if let Ok(runtime) = ReceiptRuntime::new(database.connection_mut(), &signer) {
                let _ = runtime.approval_gc(now_ms);
            }
        }
    })
}

/// Stage 01.4 retention v1: periodically compacts a per-key prefix once it
/// is both past the 90-day/100,000-row bound and free of any pending
/// action, signing a `ReceiptCheckpointV1` before deleting anything.
/// `retention_candidates` never returns a cutoff that would delete a
/// pending row, and `compact_chain` re-checks that guard itself inside the
/// same transaction as the delete — this loop only decides *when* to try,
/// never bypasses either check.
pub fn spawn_receipt_retention(
    journal: EventJournal,
    keys: Arc<ReceiptKeyManager>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(6 * 60 * 60)).await;
            let now_ms = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_millis() as i64)
                .unwrap_or_default();
            let mut database = journal.database().lock().await;
            let signer = CoreReceiptSigner(Arc::clone(&keys));
            let Ok(mut runtime) = ReceiptRuntime::new(database.connection_mut(), &signer) else {
                continue;
            };
            let Ok(candidates) = runtime.retention_candidates(now_ms) else {
                continue;
            };
            for (key_id, cutoff_sequence) in candidates {
                let _ = runtime.compact_chain(&key_id, cutoff_sequence);
            }
        }
    })
}

/// Model-request retention runs once at startup and then every six hours.
/// The repository performs the policy and closure checks transactionally, so
/// this task may safely overlap with a new request checkpoint.
pub fn spawn_model_provenance_retention(journal: EventJournal) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let now_ms = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|value| value.as_millis() as i64)
                .unwrap_or_default();
            let cutoff =
                now_ms - evohime_model_provenance::PROVENANCE_RETENTION_DAYS * 24 * 60 * 60 * 1000;
            let _ = journal.retain_model_provenance(cutoff).await;
            tokio::time::sleep(std::time::Duration::from_secs(6 * 60 * 60)).await;
        }
    })
}

/// Этап 04.2 ambient retention: истёкший текст транскриптов, истёкшие
/// метаданные эпизодов, истёкшие tombstone и состарившиеся ambient-строки
/// durable journal.
///
/// В отличие от `spawn_approval_gc` и `spawn_receipt_retention`, стартовый
/// прогон выполняется **до** первого `sleep`. Там `sleep` стоит перед
/// работой, поэтому копия того же цикла не почистила бы ничего при запуске:
/// база, открытая с просроченными строками, оставалась бы грязной ещё час.
/// Отмену эти задачи сегодня не используют, и ambient не вводит её в
/// одиночку: `CancellationToken` здесь появится тогда же, когда у остальных.
pub fn spawn_ambient_retention(journal: EventJournal) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let now_ms = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_millis() as u64)
                .unwrap_or_default();
            let _ = journal.purge_ambient(now_ms).await;
            tokio::time::sleep(std::time::Duration::from_secs(
                crate::ambient::PURGE_INTERVAL_SECONDS,
            ))
            .await;
        }
    })
}
