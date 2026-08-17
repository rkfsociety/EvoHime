use evohime_desktop_ipc::{generated, transport, FrameError};
use prost::Message;
use serde::Serialize;
use std::{collections::HashMap, time::{SystemTime, UNIX_EPOCH}};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::sync::CancellationToken;

use crate::{
    ApprovalCoordinator, CoreCommand, CoreEvent, EventJournal, SelectedModel, TaskCoordinator,
};
use evohime_local_storage::WorkItemRecord;
use evohime_model_gateway::ModelGatewayConfig;
use evohime_permissions::{Permission, PermissionMode};
use evohime_receipts::{key_lifecycle::{ReceiptKeyManager, VerificationStatus}, runtime::{ProtectedActionRow, ReceiptSigner}};
use evohime_tool_runtime::{ToolContext, ToolRegistry};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

const PROTOCOL_MAJOR: u32 = 1;
/// Number of tools `ToolRegistry::bootstrap()` is expected to register.
/// Used only as a Doctor health signal (fewer than expected => Warn), never
/// to gate functionality.
const EXPECTED_TOOL_COUNT: u32 = 23;
const PROTOCOL_MINOR: u32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum IpcBridgeError {
    #[error("IPC frame failed: {0}")]
    Frame(#[from] FrameError),
    #[error("protobuf message failed: {0}")]
    Protobuf(#[from] prost::DecodeError),
    #[error("JSON payload failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelConfigSnapshot {
    pub provider: String,
    pub route: String,
    pub model: String,
    pub configured: bool,
}

pub struct IpcBridge {
    journal: EventJournal,
    receipt_keys: Arc<ReceiptKeyManager>,
    coordinator: Option<TaskCoordinator>,
    approvals: Option<ApprovalCoordinator>,
    tools: Option<Arc<ToolRegistry>>,
    model_config: Option<ModelConfigSnapshot>,
    gateway_config: Option<ModelGatewayConfig>,
    selected_model: SelectedModel,
    core_instance_id: String,
    session_epoch: u64,
    review_tasks: Arc<tokio::sync::Mutex<HashMap<String, CancellationToken>>>,
    review_results: Arc<tokio::sync::Mutex<HashMap<String, crate::plan_review::ReviewResult>>>,
}

fn runtime_identity() -> (String, u64) {
    (
        uuid::Uuid::new_v4().to_string(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or_default(),
    )
}

impl IpcBridge {
    fn manager_for(journal: &EventJournal) -> Arc<ReceiptKeyManager> {
        let data_dir = journal
            .database_path()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        Arc::new(ReceiptKeyManager::new(data_dir))
    }
    /// Записывает лимиты каталога в локальную базу. Это подсказка для
    /// планировщика контекста, а не условие работы: провайдер может не сообщить
    /// окно, а база — быть занята другим писателем, и ни то, ни другое не повод
    /// проваливать запрос каталога.
    async fn remember_model_limits(
        &self,
        provider: &str,
        entries: &[evohime_model_gateway::ModelCatalogEntry],
    ) {
        if entries.is_empty() {
            return;
        }
        let records = entries
            .iter()
            .map(
                |entry| evohime_local_storage::model_limit_store::ModelLimitRecord {
                    model: entry.id.clone(),
                    provider: provider.to_string(),
                    context_tokens: entry.context_tokens,
                    max_output_tokens: entry.max_output_tokens,
                },
            )
            .collect::<Vec<_>>();
        let database = self.journal.database().lock().await;
        if let Err(error) = evohime_local_storage::model_limit_store::ModelLimitStoreSql::upsert_all(
            database.connection(),
            &records,
        ) {
            tracing::warn!(target: "model.catalog", %error, "model context limits were not stored");
        }
    }

    pub fn new(journal: EventJournal) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: None,
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn with_coordinator(journal: EventJournal, coordinator: TaskCoordinator) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: None,
            tools: None,
            model_config: None,
            gateway_config: None,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn with_coordinator_and_approvals(
        journal: EventJournal,
        coordinator: TaskCoordinator,
        approvals: ApprovalCoordinator,
        tools: Arc<ToolRegistry>,
        model_config: Option<ModelConfigSnapshot>,
        gateway_config: Option<ModelGatewayConfig>,
    ) -> Self {
        let (core_instance_id, session_epoch) = runtime_identity();
        let receipt_keys = Self::manager_for(&journal);
        Self {
            journal,
            receipt_keys,
            coordinator: Some(coordinator),
            approvals: Some(approvals),
            tools: Some(tools),
            model_config,
            gateway_config,
            selected_model: SelectedModel::default(),
            core_instance_id,
            session_epoch,
            review_tasks: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            review_results: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    /// Shares the agent's model selection so `SelectModelRequest` can change it.
    pub fn with_selected_model(mut self, selected: SelectedModel) -> Self {
        self.selected_model = selected;
        self
    }

    /// Streams journal entries newer than `after_sequence` to a connected
    /// client and returns the sequence it has now seen.
    ///
    /// Task progress reaches the shell this way rather than straight from the
    /// in-memory broadcast: the journal is what assigns sequence numbers, and
    /// the shell relies on them for resync after a reconnect.
    pub async fn push_journal_tail<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        after_sequence: u64,
    ) -> Result<u64, IpcBridgeError> {
        let batch = self
            .journal
            .replay_bounded(after_sequence as i64, 256)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let mut last_sequence = after_sequence;
        for record in batch.events {
            last_sequence = record.sequence_id as u64;
            let event = generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: record.sequence_id as u64,
                task_id: record.task_id,
                event_type: record.event_type,
                payload: record.payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: None,
            };
            transport::write_frame(writer, &event.encode_to_vec()).await?;
        }
        Ok(last_sequence)
    }

    /// Sequence the journal has already durably recorded.
    pub async fn latest_sequence(&self) -> u64 {
        self.journal.latest_sequence().await.max(0) as u64
    }

    /// Listener that fires whenever a task emits, so the server knows there is
    /// a journal tail worth flushing.
    /// Signal that fires once an event is durably journalled. The pipe server
    /// pushes the journal tail on this instead of on the broadcast itself,
    /// which used to overtake the writer and strand the last event of a task.
    pub fn journalled(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        self.coordinator
            .as_ref()
            .map(|coordinator| coordinator.journalled())
    }

    fn receipt_status(&self) -> serde_json::Value {
        let manager = &self.receipt_keys;
        let active = manager.active_path().exists();
        let history = manager.history_path().exists();
        let status = if !active && !history {
            "not_initialized".to_string()
        } else if !active || !history {
            "key.recovery_required".to_string()
        } else if manager.journal_path().exists() {
            "key.rotation_incomplete".to_string()
        } else {
            match manager.verify_history(None) {
                Ok(VerificationStatus::Verified) => "verified_unpinned".to_string(),
                Ok(VerificationStatus::Untrusted) => {
                    let loaded = manager.load_history().ok();
                    if loaded.as_ref().is_some_and(|items| {
                        items.iter().any(|item| {
                            matches!(item.continuity.as_str(), "broken" | "compromised")
                        })
                    }) {
                        return serde_json::json!({
                            "status": "key.trust_required",
                            "key_id": manager.load_signer().ok().map(|(metadata, _)| metadata.key_id),
                            "history_present": history,
                            "active_present": active,
                            "rotation_journal_present": manager.journal_path().exists(),
                        });
                    }
                    let genesis =
                        loaded.and_then(|items| items.first().map(|item| item.new_key_id.clone()));
                    match genesis.and_then(|key| manager.trusted_genesis(&key).ok()) {
                        Some(true) => "trusted".to_string(),
                        _ => "key.trust_required".to_string(),
                    }
                }
                Ok(VerificationStatus::Broken) => "key.history_incomplete".to_string(),
                Ok(VerificationStatus::Unsupported) => "unsupported".to_string(),
                Err(error) => error.to_string(),
            }
        };
        let key_id = std::fs::read(manager.active_path())
            .ok()
            .and_then(|bytes| {
                serde_json::from_slice::<evohime_receipts::key_lifecycle::ActiveKeyMetadata>(&bytes)
                    .ok()
            })
            .map(|metadata| metadata.key_id);
        serde_json::json!({"status": status, "key_id": key_id, "history_present": history, "active_present": active, "rotation_journal_present": manager.journal_path().exists()})
    }

    async fn take_receipt_approval<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        approval_id: &str,
        operation: &str,
    ) -> Result<bool, IpcBridgeError> {
        let Some(approvals) = &self.approvals else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        let Ok(id) = uuid::Uuid::parse_str(approval_id) else {
            self.write_response(
                writer,
                "key.approval_required",
                serde_json::to_vec(
                    &serde_json::json!({"operation": operation, "error_code":"approval.required"}),
                )?,
            )
            .await?;
            return Ok(false);
        };
        if approvals.consume_approved(id).await {
            Ok(true)
        } else {
            self.write_response(writer, "key.approval_required", serde_json::to_vec(&serde_json::json!({"operation": operation, "approval_id": id.to_string(), "error_code":"approval.required"}))?).await?;
            Ok(false)
        }
    }

    pub async fn process_once<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        &self,
        reader: &mut R,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let payload = transport::read_frame(reader).await?;
        let command = generated::CommandEnvelope::decode(payload.as_slice())?;
        let request_id = command.request_id.clone();
        let client_id = command.client_id.clone();
        let command_hash = hex_encode(&payload);
        match command.command {
            Some(generated::command_envelope::Command::Handshake(_)) => {
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "core.ready".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: Some(generated::event_envelope::Event::Ready(generated::Ready {
                        protocol: Some(protocol()),
                        core_version: env!("CARGO_PKG_VERSION").into(),
                    })),
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::GetReceiptKeyStatus(_)) => {
                self.write_response(
                    writer,
                    "key.status",
                    serde_json::to_vec(&self.receipt_status())?,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ClosePendingReceiptAction(request)) => {
                if !request.operator_confirmed || request.action_id.is_empty() || request.input_json.len() > evohime_receipts::runtime::MAX_CALL_INPUT_BYTES {
                    self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.schema_violation"}))?).await?;
                    return Ok(());
                }
                let action_id = uuid::Uuid::parse_str(&request.action_id).map_err(|error| FrameError::Io(error.to_string()))?;
                let input: serde_json::Value = serde_json::from_str(&request.input_json).map_err(|error| FrameError::Io(error.to_string()))?;
                let mut database = self.journal.database().lock().await;
                let (task_id, run_id, tool_name, normalized_scope, policy_id, decision, state): (String,String,String,String,String,String,String) = database.connection().query_row(
                    "SELECT task_id,run_id,tool_name,normalized_scope,policy_id,policy_decision,state FROM receipt_actions WHERE action_id=?1",
                    [action_id.to_string()], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                if state != "pending_recovery" {
                    self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":false,"error_code":"receipt.pending_recovery"}))?).await?;
                    return Ok(());
                }
                let policy_decision = match decision.as_str() {
                    "allow" => evohime_receipts::runtime::PolicyDecision::Allow,
                    "approval_required" => evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                    _ => evohime_receipts::runtime::PolicyDecision::Deny,
                };
                let receipt_request = evohime_receipts::runtime::ActionRequest {
                    action_id, task_id, run_id, tool_name, policy_id, normalized_scope,
                    input, policy_decision, approval_id: None, parent_approval_ref: None, preview: "unknown result closure".into(),
                };
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let receipt_hash = runtime.refuse(&receipt_request, "recovery_pending")
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                self.write_response(writer, "receipt.pending_close", serde_json::to_vec(&serde_json::json!({"ok":true,"action_id":request.action_id,"receipt_hash":receipt_hash,"completion_source":"reconciliation"}))?).await?;
            }
            Some(generated::command_envelope::Command::TrustReceiptGenesis(request)) => {
                if !self
                    .take_receipt_approval(writer, &request.approval_id, "TrustReceiptGenesis")
                    .await?
                {
                    return Ok(());
                }
                let result = self
                    .receipt_keys
                    .trust_genesis(&request.genesis_key_id, &request.source);
                let payload = match result {
                    Ok(()) => {
                        serde_json::json!({"status": "trusted", "genesis_key_id": request.genesis_key_id})
                    }
                    Err(error) => {
                        serde_json::json!({"status": error.to_string(), "error_code": error.to_string()})
                    }
                };
                self.write_response(writer, "key.trust", serde_json::to_vec(&payload)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateNewReceiptGenesis(request)) => {
                if !self
                    .take_receipt_approval(writer, &request.approval_id, "CreateNewReceiptGenesis")
                    .await?
                {
                    return Ok(());
                }
                let manager = self.receipt_keys.clone();
                let database = self.journal.database().clone();
                let result = tokio::task::spawn_blocking(move || {
                    let mut database = database.blocking_lock();
                    manager.create_new_genesis_with_database(database.connection_mut(), "user")
                })
                .await
                .map_err(|error| FrameError::Io(error.to_string()))?;
                let payload = match result {
                    Ok(key_id) => {
                        serde_json::json!({"status":"recovered", "key_id":key_id, "trust_required":true})
                    }
                    Err(error) => {
                        serde_json::json!({"status":"failed", "error_code":error.to_string()})
                    }
                };
                self.write_response(writer, "key.recovery", serde_json::to_vec(&payload)?)
                    .await?;
            }
            Some(generated::command_envelope::Command::RotateReceiptKey(request)) => {
                if !self
                    .take_receipt_approval(writer, &request.approval_id, "RotateReceiptKey")
                    .await?
                {
                    return Ok(());
                }
                let reason = request.reason.trim().to_string();
                if !matches!(reason.as_str(), "manual" | "compromise") {
                    self.write_response(
                        writer,
                        "key.rotation_failed",
                        br#"{"error_code":"key.rotation_failed"}"#.to_vec(),
                    )
                    .await?;
                } else {
                    let manager = self.receipt_keys.clone();
                    let database = self.journal.database().clone();
                    let rotation_reason = reason.clone();
                    let result = tokio::task::spawn_blocking(move || {
                        let mut database = database.blocking_lock();
                        manager.rotate_with_database(
                            database.connection_mut(),
                            &rotation_reason,
                            "user",
                        )
                    })
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let payload = match result {
                        Ok(key_id) => {
                            serde_json::json!({"status":"rotated", "key_id":key_id, "reason":reason})
                        }
                        Err(error) => {
                            serde_json::json!({"status":"failed", "error_code":error.to_string()})
                        }
                    };
                    self.write_response(writer, "key.rotation", serde_json::to_vec(&payload)?)
                        .await?;
                }
            }
            Some(generated::command_envelope::Command::ResyncRequest(request)) => {
                evohime_desktop_ipc::validate_resync_request(&request)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let limit = if request.max_events == 0 {
                    evohime_desktop_ipc::DEFAULT_RESYNC_MAX_EVENTS
                } else {
                    request.max_events
                } as usize;
                let batch = self
                    .journal
                    .replay_bounded(request.after_sequence as i64, limit)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let last_sequence = batch
                    .events
                    .last()
                    .map(|record| record.sequence_id as u64)
                    .unwrap_or(request.after_sequence);
                if batch.gap_detected {
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "after_sequence": request.after_sequence,
                        "first_available_sequence": batch.first_available_sequence,
                        "reason": "replay_gap",
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "replay.gap", payload).await?;
                }
                if request.include_full_snapshot {
                    let snapshot_json = serde_json::to_vec(&serde_json::json!({
                        "after_sequence": request.after_sequence,
                        "last_sequence": last_sequence,
                        "events": batch.events.iter().map(|record| serde_json::json!({
                            "sequence_id": record.sequence_id,
                            "task_id": record.task_id,
                            "event_type": record.event_type,
                            "payload": record.payload,
                            "created_at": record.created_at,
                        })).collect::<Vec<_>>(),
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    let snapshot = generated::FullSnapshot {
                        sequence_id: last_sequence,
                        snapshot_json,
                    };
                    evohime_desktop_ipc::validate_full_snapshot(&snapshot)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: last_sequence,
                        task_id: String::new(),
                        event_type: "replay.full_snapshot".into(),
                        payload: Vec::new(),
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: Some(generated::event_envelope::Event::FullSnapshot(snapshot)),
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                } else {
                    for record in batch.events {
                        let event = generated::EventEnvelope {
                            protocol: Some(protocol()),
                            sequence_id: record.sequence_id as u64,
                            task_id: record.task_id,
                            event_type: record.event_type,
                            payload: record.payload,
                            core_instance_id: self.core_instance_id.clone(),
                            session_epoch: self.session_epoch,
                            event: None,
                        };
                        transport::write_frame(writer, &event.encode_to_vec()).await?;
                    }
                }
                let end = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: last_sequence,
                    task_id: String::new(),
                    event_type: "resync.end".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ReplayEvents(replay)) => {
                let batch = self
                    .journal
                    .replay_bounded(replay.after_sequence as i64, 1_000)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                let mut last_sequence = batch.last_sequence as u64;
                if batch.gap_detected {
                    let payload = serde_json::to_vec(&serde_json::json!({
                        "after_sequence": replay.after_sequence,
                        "first_available_sequence": batch.first_available_sequence,
                        "reason": "replay_gap",
                    }))
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                    self.write_response(writer, "replay.gap", payload).await?;
                }
                for record in batch.events {
                    last_sequence = record.sequence_id as u64;
                    let event = generated::EventEnvelope {
                        protocol: Some(protocol()),
                        sequence_id: record.sequence_id as u64,
                        task_id: record.task_id,
                        event_type: record.event_type,
                        payload: record.payload,
                        core_instance_id: self.core_instance_id.clone(),
                        session_epoch: self.session_epoch,
                        event: None,
                    };
                    transport::write_frame(writer, &event.encode_to_vec()).await?;
                }
                let end = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: last_sequence,
                    task_id: String::new(),
                    event_type: "replay.end".into(),
                    payload: Vec::new(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &end.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::SelectModel(request)) => {
                // Bounded: a model identifier is a short single-line token.
                let model = request.model.trim();
                if model.len() > 128 || model.contains(char::is_whitespace) {
                    self.write_response(
                        writer,
                        "model.select.rejected",
                        serde_json::to_vec(&serde_json::json!({ "reason": "invalid_model" }))
                            .unwrap_or_default(),
                    )
                    .await?;
                    return Ok(());
                }
                self.selected_model.set(model);
                let payload = serde_json::to_vec(&self.current_model_config())
                    .unwrap_or_else(|_| b"null".to_vec());
                self.write_response(writer, "model.config", payload).await?;
            }
            Some(generated::command_envelope::Command::ModelConfig(_)) => {
                let payload = serde_json::to_vec(&self.current_model_config())
                    .unwrap_or_else(|_| b"null".to_vec());
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.config".into(),
                    payload,
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::ModelCatalog(request)) => {
                let mode = if request.mode == "paid" {
                    "paid"
                } else {
                    "free"
                };
                let provider = self
                    .gateway_config
                    .as_ref()
                    .and_then(|config| config.routes.get(&config.default_route))
                    .map(|route| route.provider.as_str().to_string())
                    .unwrap_or_else(|| "unknown".into());
                let result = self
                    .gateway_config
                    .as_ref()
                    .and_then(|config| config.routes.get(&config.default_route))
                    .map(|route| async move {
                        evohime_model_gateway::fetch_model_catalog(route)
                            .await
                            .map(|entries| {
                                entries
                                    .into_iter()
                                    .filter(|entry| {
                                        if mode == "free" {
                                            entry.id.ends_with(":free")
                                        } else {
                                            !entry.id.ends_with(":free")
                                        }
                                    })
                                    .collect::<Vec<_>>()
                            })
                    });
                let (entries, error) = match result {
                    Some(request) => request.await,
                    None => Err(evohime_model_gateway::providers::ProviderError::Config(
                        "provider is not configured".into(),
                    )),
                }
                .map_or_else(
                    |error| (Vec::new(), Some(error.to_string())),
                    |entries| (entries, None),
                );
                // Лимиты переживают сессию: планировщик контекста и ревью
                // должны знать окно модели ещё до первого обновления каталога,
                // а неудачный запрос не должен стирать то, что уже известно.
                self.remember_model_limits(&provider, &entries).await;
                let models = entries
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<Vec<_>>();
                let limits = entries
                    .iter()
                    .map(|entry| {
                        (
                            entry.id.clone(),
                            serde_json::json!({
                                "context": entry.context_tokens,
                                "maxOutput": entry.max_output_tokens,
                            }),
                        )
                    })
                    .collect::<serde_json::Map<_, _>>();
                let payload = serde_json::json!({
                    "mode": mode,
                    "models": models,
                    "limits": limits,
                    "error": error,
                });
                let event = generated::EventEnvelope {
                    protocol: Some(protocol()),
                    sequence_id: 0,
                    task_id: String::new(),
                    event_type: "model.catalog".into(),
                    payload: serde_json::to_vec(&payload).unwrap_or_default(),
                    core_instance_id: self.core_instance_id.clone(),
                    session_epoch: self.session_epoch,
                    event: None,
                };
                transport::write_frame(writer, &event.encode_to_vec()).await?;
            }
            Some(generated::command_envelope::Command::StartPlanReview(request)) => {
                self.start_plan_review(request, writer).await?;
            }
            Some(generated::command_envelope::Command::StopPlanReview(request)) => {
                let cancelled = self
                    .review_tasks
                    .lock()
                    .await
                    .get(&request.review_id)
                    .cloned();
                if let Some(ref token) = cancelled {
                    token.cancel();
                }
                self.write_response(
                    writer,
                    "review.stop.accepted",
                    serde_json::to_vec(&serde_json::json!({
                        "review_id": request.review_id,
                        "accepted": cancelled.is_some(),
                    }))
                    .unwrap_or_default(),
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ListPlanReviews(request)) => {
                let limit = (request.limit as usize).clamp(1, 50);
                let results = self.review_results.lock().await;
                let mut items: Vec<_> = results.values().cloned().collect();
                drop(results);
                if let Ok(events) = self.journal.review_history(limit).await {
                    for event in events {
                        if let Some(result) = review_result_from_event(&event.payload) {
                            if !items.iter().any(|item| item.review_id == result.review_id) {
                                items.push(result);
                            }
                        }
                    }
                }
                items.sort_by(|left, right| left.review_id.cmp(&right.review_id));
                items.truncate(limit);
                self.write_response(
                    writer,
                    "review.list",
                    serde_json::to_vec(&serde_json::json!({ "reviews": items }))
                        .unwrap_or_default(),
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ClearPlanReviewHistory(_)) => {
                // Running reviews keep their own state; only what the history
                // lists is dropped, and the marker is what listing reads.
                self.review_results.lock().await.clear();
                let marker_id = format!("review-history-{}", self.latest_sequence().await);
                // Recorded directly rather than published: the shell lists again
                // as soon as this response arrives, and a marker still travelling
                // through the coordinator's broadcast would not be in the journal
                // yet, so that listing would return the reviews just cleared.
                // Nothing subscribes to the marker, so no push is lost.
                let _ = self
                    .journal
                    .record(&CoreEvent::ReviewHistoryCleared { marker_id })
                    .await;
                self.write_response(
                    writer,
                    "review.historyCleared",
                    serde_json::to_vec(&serde_json::json!({ "cleared": true })).unwrap_or_default(),
                )
                .await?;
            }
            Some(generated::command_envelope::Command::GetPlanReview(request)) => {
                let mut result = self
                    .review_results
                    .lock()
                    .await
                    .get(&request.review_id)
                    .cloned();
                if result.is_none() {
                    if let Ok(events) = self.journal.task_history(&request.review_id, 10).await {
                        result = events
                            .iter()
                            .rev()
                            .find_map(|event| review_result_from_event(&event.payload));
                    }
                }
                self.write_response(
                    writer,
                    "review.result",
                    serde_json::to_vec(&serde_json::json!({
                        "review_id": request.review_id,
                        "result": result,
                    }))
                    .unwrap_or_default(),
                )
                .await?;
            }
            Some(generated::command_envelope::Command::ExportPlanReview(request)) => {
                let mut result = self
                    .review_results
                    .lock()
                    .await
                    .get(&request.review_id)
                    .cloned();
                if result.is_none() {
                    if let Ok(events) = self.journal.task_history(&request.review_id, 10).await {
                        result = events
                            .iter()
                            .rev()
                            .find_map(|event| review_result_from_event(&event.payload));
                    }
                }
                let result = result.ok_or_else(|| FrameError::Io("review not found".into()))?;
                let destination = std::path::PathBuf::from(&request.destination_path);
                if destination.extension().and_then(|value| value.to_str()) != Some("md") {
                    return Err(
                        FrameError::Io("review export must be a Markdown file".into()).into(),
                    );
                }
                let content = if request.include_reviewers {
                    serde_json::to_string_pretty(&result).unwrap_or_default()
                } else {
                    result.final_markdown.clone()
                };
                tokio::fs::write(&destination, content)
                    .await
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                self.write_response(
                    writer,
                    "review.exported",
                    serde_json::to_vec(&serde_json::json!({
                        "review_id": request.review_id,
                        "destination_path": request.destination_path,
                    }))
                    .unwrap_or_default(),
                )
                .await?;
            }
            Some(generated::command_envelope::Command::PermissionMode(request)) => {
                if let Some(tools) = &self.tools {
                    let mode = match request.mode.as_str() {
                        "full" => PermissionMode::Allow,
                        "read_only" => PermissionMode::Deny,
                        _ => PermissionMode::Ask,
                    };
                    tools.permissions().set_all_modes(mode).await;
                    if request.mode == "read_only" {
                        tools
                            .permissions()
                            .set_mode(Permission::FilesystemRead, PermissionMode::Allow)
                            .await;
                        tools
                            .permissions()
                            .set_mode(Permission::GitRead, PermissionMode::Allow)
                            .await;
                    }
                }
            }
            Some(generated::command_envelope::Command::CreateProject(request)) => {
                let result = self
                    .dispatch_create_project(client_id, request_id, command_hash, request)
                    .await?;
                self.write_response(writer, "project.created", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateTask(request)) => {
                let item = WorkItemRecord {
                    id: request.task_id,
                    project_id: request.project_id,
                    parent_id: (!request.parent_id.is_empty()).then_some(request.parent_id),
                    title: request.title,
                    description: request.description,
                    source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                    acceptance_criteria: request.acceptance_criteria,
                    non_goals: request.non_goals,
                    status: if request.status.is_empty() {
                        "backlog".into()
                    } else {
                        request.status
                    },
                    priority: request.priority,
                    estimate: (request.estimate != 0).then_some(request.estimate),
                    complexity: (!request.complexity.is_empty()).then_some(request.complexity),
                    attempt_count: 0,
                    version: 1,
                };
                let result = self
                    .dispatch_create_task(client_id, request_id, command_hash, item)
                    .await?;
                self.write_response(writer, "task.created", result).await?;
            }
            Some(generated::command_envelope::Command::UpdateTaskStatus(request)) => {
                let result = self
                    .dispatch_update_status(
                        client_id,
                        request_id,
                        command_hash,
                        request.task_id,
                        request.expected_version,
                        request.status,
                    )
                    .await?;
                self.write_response(writer, "task.status_updated", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::AddTaskEdge(request)) => {
                let result = self
                    .dispatch_add_edge(
                        client_id,
                        request_id,
                        command_hash,
                        request.from_task_id,
                        request.to_task_id,
                        request.kind,
                    )
                    .await?;
                self.write_response(writer, "task.edge_added", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetTaskGraph(request)) => {
                let result = self.dispatch_get_task_graph(request.project_id).await?;
                self.write_response(writer, "task.graph", result).await?;
            }
            Some(generated::command_envelope::Command::NextReadyTask(request)) => {
                let result = self.dispatch_next_ready_task(request.project_id).await?;
                self.write_response(writer, "task.next_ready", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ImportPrd(request)) => {
                let result = self
                    .dispatch_import_prd(client_id, request_id, command_hash, request)
                    .await?;
                self.write_response(writer, "prd.imported", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskHistory(request)) => {
                let result = self
                    .dispatch_get_task_history(request.task_id, request.limit as usize)
                    .await?;
                self.write_response(writer, "task.history", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskContext(request)) => {
                let result = self
                    .dispatch_get_task_context(
                        request.project_id,
                        request.task_id,
                        request.max_chars as usize,
                    )
                    .await?;
                self.write_response(writer, "task.context", result).await?;
            }
            Some(generated::command_envelope::Command::GetTaskPlanSpec(request)) => {
                let result = self
                    .dispatch_get_task_plan_spec(
                        request.project_id,
                        request.task_id,
                        request.max_chars as usize,
                    )
                    .await?;
                self.write_response(writer, "task.plan_spec", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ApplyApprovedBuild(request)) => {
                let result = self
                    .dispatch_apply_approved_build(
                        request.project_id,
                        request.run_id,
                        request.task_id,
                        request.approved_build_json,
                    )
                    .await?;
                self.write_response(writer, "build.applied", result).await?;
            }
            Some(generated::command_envelope::Command::PrepareBuild(request)) => {
                let result = self
                    .dispatch_prepare_build(request.project_id, request.proposal_json)
                    .await?;
                self.write_response(writer, "build.prepared", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetTaskSnapshot(request)) => {
                let result = self
                    .dispatch_get_task_snapshot(request.project_id, request.task_id)
                    .await?;
                self.write_response(writer, "task.snapshot", result).await?;
            }
            Some(generated::command_envelope::Command::RestoreTaskSnapshot(request)) => {
                let result = self
                    .dispatch_restore_task_snapshot(
                        request.project_id,
                        request.task_id,
                        request.snapshot_id,
                    )
                    .await?;
                self.write_response(writer, "snapshot.restored", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetBuildPolicy(request)) => {
                let result = self.dispatch_get_build_policy(request.project_id).await?;
                self.write_response(writer, "build.policy", result).await?;
            }
            Some(generated::command_envelope::Command::SaveBuildPolicy(request)) => {
                let result = self
                    .dispatch_save_build_policy(
                        request.project_id,
                        request.policy_json,
                        request.expected_version,
                    )
                    .await?;
                self.write_response(writer, "build.policy.saved", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::StartTask(start)) => {
                if let Some(coordinator) = &self.coordinator {
                    coordinator
                        .dispatch(CoreCommand::StartTask {
                            task_id: start.task_id,
                            prompt: start.prompt,
                            workspace_root: (!start.workspace_path.is_empty())
                                .then(|| std::path::PathBuf::from(start.workspace_path)),
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                }
            }
            Some(generated::command_envelope::Command::StopTask(stop)) => {
                if let Some(coordinator) = &self.coordinator {
                    coordinator
                        .dispatch(CoreCommand::StopTask {
                            task_id: stop.task_id,
                        })
                        .await
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                }
            }
            Some(generated::command_envelope::Command::ListWorkspace(request)) => {
                let listing = crate::workspace::list_directory(
                    request.workspace_path,
                    if request.relative_path.is_empty() {
                        "."
                    } else {
                        &request.relative_path
                    },
                    if request.max_entries == 0 {
                        crate::workspace::MAX_LIST_ENTRIES
                    } else {
                        request.max_entries as usize
                    },
                )
                .map_err(|error| FrameError::Io(error.to_string()))?;
                let payload = serde_json::to_vec(&listing)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                self.write_response(writer, "workspace.list", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::ReadWorkspaceFile(request)) => {
                let content = crate::workspace::read_text_file(
                    request.workspace_path,
                    &request.relative_path,
                    if request.max_bytes == 0 {
                        crate::workspace::MAX_READ_BYTES
                    } else {
                        request.max_bytes as usize
                    },
                )
                .map_err(|error| FrameError::Io(error.to_string()))?;
                let payload = serde_json::to_vec(&serde_json::json!({
                    "path": request.relative_path,
                    "content": content,
                }))
                .map_err(|error| FrameError::Io(error.to_string()))?;
                self.write_response(writer, "workspace.file", payload)
                    .await?;
            }
            Some(generated::command_envelope::Command::GitStatus(request)) => {
                let payload = self
                    .dispatch_git_read(
                        request.workspace_path,
                        "git.status",
                        serde_json::Value::Null,
                        request.max_bytes,
                    )
                    .await?;
                self.write_response(writer, "git.status", payload).await?;
            }
            Some(generated::command_envelope::Command::GitDiff(request)) => {
                let input = if request.relative_path.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::json!({"path": request.relative_path})
                };
                let payload = self
                    .dispatch_git_read(request.workspace_path, "git.diff", input, request.max_bytes)
                    .await?;
                self.write_response(writer, "git.diff", payload).await?;
            }
            Some(generated::command_envelope::Command::TerminalExecute(request)) => {
                self.dispatch_terminal_execute(request, writer).await?;
            }
            Some(generated::command_envelope::Command::RunDoctor(request)) => {
                let result = self
                    .dispatch_run_doctor(
                        request.project_id,
                        request.detail_level,
                        command.protocol.clone(),
                    )
                    .await?;
                self.write_response(writer, "doctor.report", result).await?;
            }
            Some(generated::command_envelope::Command::ExportDoctorLogs(request)) => {
                let result = self
                    .dispatch_export_doctor_logs(request.destination_path)
                    .await?;
                self.write_response(writer, "doctor.export.completed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateDatabaseBackup(request)) => {
                self.dispatch_create_database_backup(request_id, request.destination_path, writer)
                    .await?;
            }
            Some(generated::command_envelope::Command::PrepareDatabaseRestore(request)) => {
                let result = self
                    .dispatch_prepare_database_restore(request_id, request.backup_path)
                    .await?;
                self.write_response(writer, "storage.restore.preview", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RestoreDatabase(request)) => {
                self.dispatch_restore_database(
                    request_id,
                    request.backup_path,
                    request.approval_id,
                    writer,
                )
                .await?;
            }
            Some(generated::command_envelope::Command::CancelDatabaseOperation(request)) => {
                let result = self
                    .dispatch_cancel_database_operation(request.operation_id)
                    .await?;
                self.write_response(writer, "storage.cancel.requested", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SaveResearchEvidence(request)) => {
                let result = self.dispatch_save_research_evidence(request).await?;
                self.write_response(writer, "research.evidence.saved", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListResearchEvidence(request)) => {
                let result = self
                    .dispatch_list_research_evidence(request.work_item_id)
                    .await?;
                self.write_response(writer, "research.evidence.list", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RunResearchFetch(request)) => {
                let result = self.dispatch_run_research_fetch(request).await?;
                self.write_response(writer, "research.fetch.completed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CreateMemory(request)) => {
                let result = self.dispatch_create_memory(request).await?;
                self.write_response(writer, "memory.created", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListMemory(request)) => {
                let result = self.dispatch_list_memory(request).await?;
                self.write_response(writer, "memory.list", result).await?;
            }
            Some(generated::command_envelope::Command::SearchMemory(request)) => {
                let result = self.dispatch_search_memory(request).await?;
                self.write_response(writer, "memory.search", result).await?;
            }
            Some(generated::command_envelope::Command::ArchiveMemory(request)) => {
                let result = self.dispatch_archive_memory(request).await?;
                self.write_response(writer, "memory.archived", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetMemory(request)) => {
                let result = self.dispatch_get_memory(request).await?;
                self.write_response(writer, "memory.record", result).await?;
            }
            Some(generated::command_envelope::Command::ListMemoryPending(request)) => {
                let result = self.dispatch_list_memory_pending(request).await?;
                self.write_response(writer, "memory.pending", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetMemoryConflicts(request)) => {
                let result = self.dispatch_get_memory_conflicts(request).await?;
                self.write_response(writer, "memory.conflicts", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ConfirmMemory(request)) => {
                let result = self.dispatch_confirm_memory(request).await?;
                self.write_response(writer, "memory.confirmed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RejectMemory(request)) => {
                let result = self.dispatch_reject_memory(request).await?;
                self.write_response(writer, "memory.rejected", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ReviseMemoryCandidate(request)) => {
                let result = self.dispatch_revise_memory_candidate(request).await?;
                self.write_response(writer, "memory.revised", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SupersedeMemory(request)) => {
                let result = self.dispatch_supersede_memory(request).await?;
                self.write_response(writer, "memory.superseded", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ForgetMemory(request)) => {
                let result = self.dispatch_forget_memory(request).await?;
                self.write_response(writer, "memory.forgotten", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::InstallCapability(request)) => {
                let result = self.dispatch_install_capability(request).await?;
                self.write_response(writer, "capability.installed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListCapabilities(request)) => {
                let result = self.dispatch_list_capabilities(request).await?;
                self.write_response(writer, "capability.list", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::MatchCapabilities(request)) => {
                let result = self.dispatch_match_capabilities(request).await?;
                self.write_response(writer, "capability.match", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RemoveCapability(request)) => {
                let result = self.dispatch_remove_capability(request).await?;
                self.write_response(writer, "capability.removed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetCapabilitySelection(request)) => {
                let result = self.dispatch_get_capability_selection(request).await?;
                self.write_response(writer, "capability.selection", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::PinCapabilitySelection(request)) => {
                let result = self.dispatch_pin_capability_selection(request).await?;
                self.write_response(writer, "capability.selection.pinned", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ReplaceCapabilitySelection(request)) => {
                let result = self.dispatch_replace_capability_selection(request).await?;
                self.write_response(writer, "capability.selection.replaced", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RequestChildHandoff(request)) => {
                let result = self.dispatch_request_child_handoff(request).await?;
                self.write_response(writer, "child.handoff.requested", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListChildHandoffs(request)) => {
                let result = self.dispatch_list_child_handoffs(request).await?;
                self.write_response(writer, "child.handoff.list", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SubmitChildRequest(request)) => {
                let result = self.dispatch_submit_child_request(request).await?;
                self.write_response(writer, "child.request.submitted", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SubmitChildReport(request)) => {
                let result = self.dispatch_submit_child_report(request).await?;
                self.write_response(writer, "child.report.accepted", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SubmitFeedback(request)) => {
                let result = self.dispatch_submit_feedback(request).await?;
                self.write_response(writer, "feedback.submitted", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::IndexWorkspace(request)) => {
                let result = self.dispatch_index_workspace(request, false).await?;
                self.write_response(writer, "workspace.indexed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::RebuildIndex(request)) => {
                let result = self.dispatch_rebuild_index(request).await?;
                self.write_response(writer, "workspace.indexed", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SearchWorkspaceKnowledge(request)) => {
                let result = self.dispatch_search_workspace_knowledge(request).await?;
                self.write_response(writer, "workspace.knowledge", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetIndexStatus(request)) => {
                let result = self.dispatch_get_index_status(request).await?;
                self.write_response(writer, "workspace.index_status", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::CancelWorkspaceIndex(request)) => {
                let result = self.dispatch_cancel_workspace_index(request).await?;
                self.write_response(writer, "workspace.index_cancelled", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::GetContextLedger(request)) => {
                let result = self.dispatch_get_context_ledger(request).await?;
                self.write_response(writer, "context.ledger", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListTaskScratchpad(request)) => {
                let result = self.dispatch_list_task_scratchpad(request).await?;
                self.write_response(writer, "context.scratchpad", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ClearTaskScratchpad(request)) => {
                let result = self.dispatch_clear_task_scratchpad(request).await?;
                self.write_response(writer, "context.scratchpad_cleared", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::SummarizeContextNow(request)) => {
                let result = self.dispatch_summarize_context_now(request).await?;
                self.write_response(writer, "context.summarize_requested", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::PinContextItem(request)) => {
                let result = self.dispatch_pin_context_item(request).await?;
                self.write_response(writer, "context.item_pinned", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ReadContextArtifact(request)) => {
                let result = self.dispatch_read_context_artifact(request).await?;
                self.write_response(writer, "context.artifact", result)
                    .await?;
            }
            Some(generated::command_envelope::Command::ListFeedback(request)) => {
                let result = self.dispatch_list_feedback(request).await?;
                self.write_response(writer, "feedback.list", result).await?;
            }
            Some(generated::command_envelope::Command::ResolveApproval(resolve)) => {
                let approval_id = uuid::Uuid::parse_str(&resolve.approval_id)
                    .map_err(|error| FrameError::Io(format!("invalid approval id: {error}")))?;
                if let Some(tools) = &self.tools {
                    let _ = tools
                        .permissions()
                        .resolve(approval_id, resolve.granted)
                        .await;
                }
                if let Some(approvals) = &self.approvals {
                    let _ = approvals.resolve(approval_id, resolve.granted).await;
                }
            }
            None => {}
        }
        Ok(())
    }

    async fn dispatch_create_project(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        request: generated::CreateProject,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateProject {
                client_id,
                request_id,
                command_hash,
                project_id: request.project_id,
                title: request.title,
                workspace_path: request.workspace_path,
                source_ref: (!request.source_ref.is_empty()).then_some(request.source_ref),
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_create_task(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        item: WorkItemRecord,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateTask {
                client_id,
                request_id,
                command_hash,
                item,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_update_status(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        task_id: String,
        expected_version: i64,
        status: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::UpdateTaskStatus {
                client_id,
                request_id,
                command_hash,
                task_id,
                expected_version,
                status,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_add_edge(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        from_task_id: String,
        to_task_id: String,
        kind: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::AddTaskEdge {
                client_id,
                request_id,
                command_hash,
                from_task_id,
                to_task_id,
                kind,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_graph(&self, project_id: String) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskGraph { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_next_ready_task(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::NextReadyTask { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_import_prd(
        &self,
        client_id: String,
        request_id: String,
        command_hash: String,
        request: generated::ImportPrd,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ImportPrd {
                client_id,
                request_id,
                command_hash,
                import_id: request.import_id,
                project_id: request.project_id,
                origin: request.origin,
                version: request.version,
                source_text: request.source_text,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_history(
        &self,
        task_id: String,
        limit: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskHistory {
                task_id,
                limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_context(
        &self,
        project_id: String,
        task_id: String,
        max_chars: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskContext {
                project_id,
                task_id,
                max_chars,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_plan_spec(
        &self,
        project_id: String,
        task_id: String,
        max_chars: usize,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskPlanSpec {
                project_id,
                task_id,
                max_chars,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_apply_approved_build(
        &self,
        project_id: String,
        run_id: String,
        task_id: String,
        approved_build_json: Vec<u8>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ApplyApprovedBuild {
                project_id,
                run_id,
                task_id,
                approved_build_json,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_prepare_build(
        &self,
        project_id: String,
        proposal_json: Vec<u8>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PrepareBuild {
                project_id,
                proposal_json,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_task_snapshot(
        &self,
        project_id: String,
        task_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetTaskSnapshot {
                project_id,
                task_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_restore_task_snapshot(
        &self,
        project_id: String,
        task_id: String,
        snapshot_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RestoreTaskSnapshot {
                project_id,
                task_id,
                snapshot_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_build_policy(
        &self,
        project_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetBuildPolicy { project_id, reply })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_save_build_policy(
        &self,
        project_id: String,
        policy_json: Vec<u8>,
        expected_version: i64,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveBuildPolicy {
                project_id,
                policy_json,
                expected_version,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_run_doctor(
        &self,
        project_id: String,
        detail_level: i32,
        protocol: Option<generated::ProtocolVersion>,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let approval_required = match &self.tools {
            Some(tools) => !matches!(
                tools.permissions().mode(Permission::FilesystemWrite).await,
                PermissionMode::Allow
            ),
            None => true,
        };
        let (registered_tools, expected_tools, unavailable_tools) = match &self.tools {
            Some(tools) => {
                let names = tools.list();
                (names.len() as u32, EXPECTED_TOOL_COUNT, Vec::new())
            }
            None => (0, EXPECTED_TOOL_COUNT, Vec::new()),
        };
        let detail_level = if detail_level == 1 {
            crate::doctor::DetailLevel::Detailed
        } else {
            crate::doctor::DetailLevel::Summary
        };
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunDoctor {
                project_id,
                protocol_major: protocol.map(|version| version.major),
                expected_protocol_major: PROTOCOL_MAJOR,
                provider: self.provider_probe(),
                approval_required,
                registered_tools,
                expected_tools,
                unavailable_tools,
                detail_level,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_export_doctor_logs(
        &self,
        destination_path: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ExportDoctorLogs {
                destination_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_create_database_backup<W: AsyncWrite + Unpin>(
        &self,
        operation_id: String,
        destination_path: String,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        let (reply, _response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateDatabaseBackup {
                operation_id,
                destination_path,
                progress,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let payload = serde_json::to_vec(&serde_json::json!({"accepted": true}))?;
        self.write_response(writer, "storage.backup.started", payload)
            .await?;
        Ok(())
    }

    async fn dispatch_prepare_database_restore(
        &self,
        operation_id: String,
        backup_path: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PrepareDatabaseRestore {
                operation_id,
                backup_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_restore_database<W: AsyncWrite + Unpin>(
        &self,
        operation_id: String,
        backup_path: String,
        approval_id: String,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (progress, _progress_rx) = mpsc::unbounded_channel();
        let (reply, _response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RestoreDatabase {
                operation_id,
                backup_path,
                approval_id,
                progress,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let payload = serde_json::to_vec(&serde_json::json!({"accepted": true}))?;
        self.write_response(writer, "storage.restore.started", payload)
            .await?;
        Ok(())
    }

    async fn dispatch_cancel_database_operation(
        &self,
        operation_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CancelDatabaseOperation {
                operation_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_save_research_evidence(
        &self,
        request: generated::SaveResearchEvidence,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SaveResearchEvidence {
                work_item_id: request.work_item_id,
                source_kind: request.source_kind,
                source_ref: request.source_ref,
                title: request.title,
                publisher: request.publisher,
                content_type: request.content_type,
                raw_excerpt: request.raw_excerpt,
                retrieved_at_ms: request.retrieved_at_ms,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_research_evidence(
        &self,
        work_item_id: String,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListResearchEvidence {
                work_item_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_run_research_fetch(
        &self,
        request: generated::RunResearchFetch,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RunResearchFetch {
                work_item_id: request.work_item_id,
                url: request.url,
                title: request.title,
                allowed_domains: request.allowed_domains,
                max_bytes: request.max_bytes,
                max_latency_ms: request.max_latency_ms,
                max_cost_micros: request.max_cost_micros,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_create_memory(
        &self,
        request: generated::CreateMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::CreateMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                title: request.title,
                content: request.content,
                provenance_kind: request.provenance_kind,
                provenance_id: request.provenance_id,
                provenance_locator: request.provenance_locator,
                privacy: request.privacy,
                ttl_ms: request.ttl_ms,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_memory(
        &self,
        request: generated::ListMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                include_archived: request.include_archived,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_search_memory(
        &self,
        request: generated::SearchMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SearchMemory {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                query: request.query,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_archive_memory(
        &self,
        request: generated::ArchiveMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ArchiveMemory {
                id: request.id,
                approval_id: request.approval_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_forget_memory(
        &self,
        request: generated::ForgetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ForgetMemory {
                id: request.id,
                approval_id: request.approval_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_memory(
        &self,
        request: generated::GetMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetMemory {
                id: request.id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_memory_pending(
        &self,
        request: generated::ListMemoryPending,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListMemoryPending {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                limit: request.limit,
                workspace_path: request.workspace_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_memory_conflicts(
        &self,
        request: generated::GetMemoryConflicts,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetMemoryConflicts {
                scope_kind: request.scope_kind,
                project_id: request.project_id,
                secondary_id: request.secondary_id,
                limit: request.limit,
                workspace_path: request.workspace_path,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_confirm_memory(
        &self,
        request: generated::ConfirmMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ConfirmMemory {
                ids: request.ids,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_reject_memory(
        &self,
        request: generated::RejectMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RejectMemory {
                ids: request.ids,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_revise_memory_candidate(
        &self,
        request: generated::ReviseMemoryCandidate,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReviseMemoryCandidate {
                id: request.id,
                statement: request.statement,
                session_only: request.session_only,
                session_id: request.session_id,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_supersede_memory(
        &self,
        request: generated::SupersedeMemory,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SupersedeMemory {
                old_id: request.old_id,
                new_id: request.new_id,
                reason: request.reason,
                approval_id: request.approval_id,
                idempotency_key: request.idempotency_key,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_install_capability(
        &self,
        request: generated::InstallCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::InstallCapability {
                manifest_json: request.manifest_json,
                install_source: request.install_source,
                source_path: request.source_path,
                expected_content_hash: request.expected_content_hash,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_capabilities(
        &self,
        request: generated::ListCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListCapabilities {
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_match_capabilities(
        &self,
        request: generated::MatchCapabilities,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::MatchCapabilities {
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_remove_capability(
        &self,
        request: generated::RemoveCapability,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RemoveCapability {
                id: request.id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_get_capability_selection(
        &self,
        request: generated::GetCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::GetCapabilitySelection {
                task_id: request.task_id,
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_pin_capability_selection(
        &self,
        request: generated::PinCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::PinCapabilitySelection {
                task_id: request.task_id,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_replace_capability_selection(
        &self,
        request: generated::ReplaceCapabilitySelection,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ReplaceCapabilitySelection {
                task_id: request.task_id,
                manifest_name: request.manifest_name,
                intent: request.intent,
                required_tools: request.required_tools,
                required_domains: request.required_domains,
                requested_risk: request.requested_risk,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_request_child_handoff(
        &self,
        request: generated::RequestChildHandoff,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::RequestChildHandoff {
                handoff_id: request.handoff_id,
                task_id: request.task_id,
                kind: request.kind,
                from_role: request.from_role,
                from_name: request.from_name,
                to_role: request.to_role,
                to_name: request.to_name,
                purpose: request.purpose,
                payload: request.payload,
                sequence: request.sequence,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_child_handoffs(
        &self,
        request: generated::ListChildHandoffs,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListChildHandoffs {
                task_id: request.task_id,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_submit_child_request(
        &self,
        request: generated::SubmitChildRequest,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildRequest {
                child_task_id: request.child_task_id,
                parent_task_id: request.parent_task_id,
                role: request.role,
                kind: request.kind,
                reduced_context: request.reduced_context,
                max_output_bytes: request.max_output_bytes,
                requested_capabilities: request.requested_capabilities,
                parent_is_child: request.parent_is_child,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_submit_child_report(
        &self,
        request: generated::SubmitChildReport,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitChildReport {
                child_task_id: request.child_task_id,
                status: request.status,
                summary: request.summary,
                findings: request.findings,
                sources: request.sources,
                confidence_percent: request.confidence_percent,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_submit_feedback(
        &self,
        request: generated::SubmitFeedback,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::SubmitFeedback {
                run_id: request.run_id,
                task_id: (!request.task_id.trim().is_empty()).then_some(request.task_id),
                subject_ref: (!request.subject_ref.trim().is_empty())
                    .then_some(request.subject_ref),
                signal: request.signal,
                correction: (!request.correction.trim().is_empty()).then_some(request.correction),
                rejection_reason: (!request.rejection_reason.trim().is_empty())
                    .then_some(request.rejection_reason),
                outcome: (!request.outcome.trim().is_empty()).then_some(request.outcome),
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    async fn dispatch_list_feedback(
        &self,
        request: generated::ListFeedback,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(CoreCommand::ListFeedback {
                run_id: request.run_id,
                limit: request.limit,
                reply,
            })
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    /// План 01.5: bounded projection состава контекста.
    async fn dispatch_index_workspace(
        &self,
        request: generated::IndexWorkspace,
        rebuild: bool,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| {
            if rebuild {
                CoreCommand::RebuildIndex {
                    workspace_path: request.workspace_path.clone(),
                    enable_embeddings: request.enable_embeddings,
                    reply,
                }
            } else {
                CoreCommand::IndexWorkspace {
                    workspace_path: request.workspace_path.clone(),
                    enable_embeddings: request.enable_embeddings,
                    reply,
                }
            }
        })
        .await
    }

    async fn dispatch_rebuild_index(
        &self,
        request: generated::RebuildIndex,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::RebuildIndex {
            workspace_path: request.workspace_path.clone(),
            enable_embeddings: request.enable_embeddings,
            reply,
        })
        .await
    }

    async fn dispatch_search_workspace_knowledge(
        &self,
        request: generated::SearchWorkspaceKnowledge,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::SearchWorkspaceKnowledge {
            workspace_path: request.workspace_path.clone(),
            query: request.query.clone(),
            path_filter: (!request.path_filter.trim().is_empty())
                .then(|| request.path_filter.clone()),
            language_filter: (!request.language_filter.trim().is_empty())
                .then(|| request.language_filter.clone()),
            hybrid: request.hybrid,
            reply,
        })
        .await
    }

    async fn dispatch_get_index_status(
        &self,
        request: generated::GetIndexStatus,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::GetIndexStatus {
            workspace_path: request.workspace_path.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_cancel_workspace_index(
        &self,
        request: generated::CancelWorkspaceIndex,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::CancelWorkspaceIndex {
            workspace_path: request.workspace_path.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_get_context_ledger(
        &self,
        request: generated::GetContextLedger,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::GetContextLedger {
            task_id: request.task_id.clone(),
            limit: request.limit,
            reply,
        })
        .await
    }

    async fn dispatch_list_task_scratchpad(
        &self,
        request: generated::ListTaskScratchpad,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ListTaskScratchpad {
            task_id: request.task_id.clone(),
            category: (!request.category.trim().is_empty()).then(|| request.category.clone()),
            status: (!request.status.trim().is_empty()).then(|| request.status.clone()),
            limit: request.limit,
            reply,
        })
        .await
    }

    async fn dispatch_clear_task_scratchpad(
        &self,
        request: generated::ClearTaskScratchpad,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ClearTaskScratchpad {
            task_id: request.task_id.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_summarize_context_now(
        &self,
        request: generated::SummarizeContextNow,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::SummarizeContextNow {
            task_id: request.task_id.clone(),
            reply,
        })
        .await
    }

    async fn dispatch_pin_context_item(
        &self,
        request: generated::PinContextItem,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::PinContextItem {
            task_id: request.task_id.clone(),
            item_id: request.item_id.clone(),
            pinned: request.pinned,
            reply,
        })
        .await
    }

    async fn dispatch_read_context_artifact(
        &self,
        request: generated::ReadContextArtifact,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        self.dispatch_context(|reply| CoreCommand::ReadContextArtifact {
            task_id: request.task_id.clone(),
            locator: request.locator.clone(),
            reply,
        })
        .await
    }

    /// Общая отправка команды контекста в очередь Core.
    async fn dispatch_context<F>(&self, build: F) -> Result<Vec<u8>, IpcBridgeError>
    where
        F: FnOnce(oneshot::Sender<Result<Vec<u8>, String>>) -> CoreCommand,
    {
        let coordinator = self
            .coordinator
            .as_ref()
            .ok_or_else(|| FrameError::Io("core command queue is not configured".into()))?;
        let (reply, response) = oneshot::channel();
        coordinator
            .dispatch(build(reply))
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        response
            .await
            .map_err(|_| FrameError::Io("core command queue dropped the response".into()))?
            .map_err(FrameError::Io)
            .map_err(IpcBridgeError::from)
    }

    /// Configuration as the shell should see it: the route's own model unless
    /// the shell selected another one for the next request.
    fn current_model_config(&self) -> Option<ModelConfigSnapshot> {
        let config = self.model_config.as_ref()?;
        let Some(model) = self.selected_model.get() else {
            return Some(config.clone());
        };
        Some(ModelConfigSnapshot {
            model,
            ..config.clone()
        })
    }

    /// Builds a Core Doctor provider probe from already-loaded, secret-free
    /// gateway configuration. Never exposes an API key value, only whether
    /// one is present.
    fn provider_probe(&self) -> crate::doctor::ProviderProbe {
        let (provider_id, model_id, configured) = match &self.model_config {
            Some(config) => (
                config.provider.clone(),
                config.model.clone(),
                config.configured,
            ),
            None => (String::new(), String::new(), false),
        };
        let key_present = self
            .gateway_config
            .as_ref()
            .and_then(|config| config.routes.get(&config.default_route))
            .map(|route| route.configured())
            .unwrap_or(false);
        let metadata_valid = !provider_id.is_empty() && !model_id.is_empty();
        crate::doctor::ProviderProbe {
            provider_id,
            model_id,
            configured,
            key_present,
            metadata_valid,
        }
    }

    async fn dispatch_git_read(
        &self,
        workspace_path: String,
        tool_name: &str,
        input: serde_json::Value,
        requested_max_bytes: u32,
    ) -> Result<Vec<u8>, IpcBridgeError> {
        const DEFAULT_MAX_BYTES: usize = 512 * 1024;
        let max_bytes = if requested_max_bytes == 0 {
            DEFAULT_MAX_BYTES
        } else {
            (requested_max_bytes as usize).min(DEFAULT_MAX_BYTES)
        };
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| FrameError::Io("Git tools are not configured".into()))?;
        let context = ToolContext {
            workspace_root: std::path::PathBuf::from(workspace_path),
            task_id: uuid::Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };
        let result = tools
            .execute(&context, tool_name, input)
            .await
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let bytes = result.output.as_bytes();
        let truncated = bytes.len() > max_bytes;
        let output = if truncated {
            String::from_utf8_lossy(&bytes[..max_bytes]).into_owned()
        } else {
            result.output
        };
        serde_json::to_vec(&serde_json::json!({
            "output": output,
            "structured": result.structured,
            "truncated": truncated,
            "max_bytes": max_bytes,
        }))
        .map_err(|error| FrameError::Io(error.to_string()).into())
    }

    async fn execute_terminal_with_receipt(
        &self,
        context: &ToolContext,
        input: serde_json::Value,
        cancellation: CancellationToken,
    ) -> Result<evohime_tool_runtime::ToolResult, evohime_tool_runtime::ToolError> {
        match self.tools.as_ref().ok_or_else(|| evohime_tool_runtime::ToolError::Execution("Terminal tools are not configured".into()))?.preflight(context, "shell.execute", &input).await? {
            evohime_tool_runtime::ToolPreflightDecision::Allowed { scope, preview } => {
                let request = evohime_receipts::runtime::ActionRequest { action_id: uuid::Uuid::now_v7(), task_id: context.task_id.to_string(), run_id: context.task_id.to_string(), tool_name: "shell.execute".into(), policy_id: "permission:ShellExecute".into(), normalized_scope: scope, input: input.clone(), policy_decision: evohime_receipts::runtime::PolicyDecision::Allow, approval_id: None, parent_approval_ref: None, preview: serde_json::to_string(&preview).unwrap_or_else(|_| "terminal".into()) };
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer).map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                if !matches!(runtime.prepare(request.clone()).map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?, evohime_receipts::runtime::PrepareOutcome::Prepared { .. }) { return Err(evohime_tool_runtime::ToolError::Execution("receipt.precondition_failed".into())); }
                runtime.mark_started(request.action_id).map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                drop(database);
                let result = self.tools.as_ref().unwrap().execute_with_cancellation(context, "shell.execute", input, cancellation).await;
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer).map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?;
                match &result {
                    Ok(value) => { runtime.mark_returned(request.action_id).map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?; let digest = evohime_receipts::sha256_hex(value.output.as_bytes()); runtime.complete(&request, "succeeded", &digest, None).map_err(|e| evohime_tool_runtime::ToolError::Execution(e.to_string()))?; }
                    Err(error) => {
                        let pre_hash = runtime.action(request.action_id).ok().flatten().and_then(|row| row.pre_receipt_hash).unwrap_or_default();
                        let row = ProtectedActionRow {
                            schema_version: 1,
                            action_id: request.action_id.to_string(),
                            pre_receipt_hash: pre_hash,
                            tool_args_hash: evohime_receipts::runtime::canonical_call_hash(&request.tool_name, &request.normalized_scope, &request.input).unwrap_or_default(),
                            result_status: "failed".into(),
                            result_hash: evohime_receipts::sha256_hex(error.to_string().as_bytes()),
                            recovery_code: "unknown".into(),
                            created_at_ms: SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis() as i64).unwrap_or_default(),
                            key_id: signer.key_id().unwrap_or_else(|_| "unavailable".into()),
                        };
                        if let Ok(plain) = serde_json::to_vec(&row) {
                            if let Ok(envelope) = self.receipt_keys.protect_storage(&plain) { let _ = runtime.store_protected_envelope(&row, envelope); }
                        }
                        let _ = runtime.mark_pending_recovery(request.action_id, "unknown");
                    }
                }
                result
            }
            evohime_tool_runtime::ToolPreflightDecision::Denied(permission) => Err(evohime_tool_runtime::ToolError::PermissionDenied(permission)),
            evohime_tool_runtime::ToolPreflightDecision::ApprovalRequired { .. } => self.tools.as_ref().unwrap().execute_with_cancellation(context, "shell.execute", input, cancellation).await,
        }
    }

    async fn dispatch_terminal_execute<W: AsyncWrite + Unpin>(
        &self,
        request: generated::TerminalExecute,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        const DEFAULT_TIMEOUT_MS: u32 = 30_000;
        const MAX_OUTPUT_BYTES: usize = 512 * 1024;
        let tools = self
            .tools
            .as_ref()
            .ok_or_else(|| FrameError::Io("Terminal tools are not configured".into()))?;
        let task_id = uuid::Uuid::parse_str(&request.task_id)
            .map_err(|error| FrameError::Io(format!("invalid terminal task id: {error}")))?;
        let workspace_root = std::path::PathBuf::from(request.workspace_path);
        let input = serde_json::json!({
            "program": request.program,
            "args": request.args,
            "cwd": (!request.cwd.is_empty()).then_some(request.cwd),
            "timeout_ms": if request.timeout_ms == 0 { DEFAULT_TIMEOUT_MS } else { request.timeout_ms.min(DEFAULT_TIMEOUT_MS) },
        });
        let context = ToolContext {
            workspace_root,
            task_id,
            session_id: None,
            progress_tx: None,
        };
        let cancellation = tokio_util::sync::CancellationToken::new();
        let result = if request.approval_id.is_empty() {
            match self.execute_terminal_with_receipt(&context, input.clone(), cancellation.clone()).await
            {
                Ok(result) => result,
                Err(evohime_tool_runtime::ToolError::NeedsApproval {
                    tool,
                    permission,
                    scope,
                    approval_id,
                    input,
                    preview,
                }) => {
                    let durable_action_id = uuid::Uuid::now_v7();
                    let receipt_request = evohime_receipts::runtime::ActionRequest {
                        action_id: durable_action_id,
                        task_id: task_id.to_string(),
                        run_id: task_id.to_string(),
                        tool_name: tool.clone(),
                        policy_id: format!("permission:{permission:?}"),
                        normalized_scope: scope.clone(),
                        input: input.clone(),
                        policy_decision: evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                        approval_id: Some(approval_id),
                        parent_approval_ref: None,
                        preview: serde_json::to_string(&preview).unwrap_or_else(|_| "approval".into()),
                    };
                    {
                        let mut database = self.journal.database().lock().await;
                        let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                        let runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                        runtime.prepare_existing_approval(receipt_request)
                            .map_err(|error| FrameError::Io(error.to_string()))?;
                    }
                    self.write_response(
                        writer,
                        "approval.required",
                        serde_json::to_vec(&serde_json::json!({
                            "task_id": task_id.to_string(),
                            "approval_id": approval_id.to_string(),
                            "tool_name": tool,
                            "permission": format!("{permission:?}"),
                            "scope": scope,
                            "preview": preview,
                        }))?,
                    )
                    .await?;
                    return Ok(());
                }
                Err(error) => {
                    return self
                        .write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error": error.to_string(),
                            }))?,
                        )
                        .await;
                }
            }
        } else {
            let approval_id = uuid::Uuid::parse_str(&request.approval_id).map_err(|error| {
                FrameError::Io(format!("invalid terminal approval id: {error}"))
            })?;
            let (action_id, receipt_request) = {
                let database = self.journal.database().lock().await;
                let (action_id, receipt_scope): (String, String) = database.connection().query_row(
                    "SELECT action_id,normalized_scope FROM receipt_approval_intents WHERE approval_id=?1",
                    [approval_id.to_string()], |row| Ok((row.get(0)?, row.get(1)?)),
                ).map_err(|error| FrameError::Io(error.to_string()))?;
                let action_id = uuid::Uuid::parse_str(&action_id).map_err(|error| FrameError::Io(error.to_string()))?;
                (action_id, evohime_receipts::runtime::ActionRequest {
                    action_id,
                    task_id: task_id.to_string(), run_id: task_id.to_string(), tool_name: "shell.execute".into(),
                    policy_id: "permission:ShellExecute".into(), normalized_scope: receipt_scope, input: input.clone(),
                    policy_decision: evohime_receipts::runtime::PolicyDecision::ApprovalRequired,
                    approval_id: Some(approval_id), parent_approval_ref: None, preview: "terminal approval".into(),
                })
            };
            {
                let mut database = self.journal.database().lock().await;
                let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                let mut runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                    .map_err(|error| FrameError::Io(error.to_string()))?;
                runtime.grant_approval(approval_id).map_err(|error| FrameError::Io(error.to_string()))?;
                runtime.claim_approval(&receipt_request, approval_id).map_err(|error| FrameError::Io(error.to_string()))?;
                runtime.mark_started(action_id).map_err(|error| FrameError::Io(error.to_string()))?;
            }
            match tools
                .execute_after_approval(&context, "shell.execute", input, approval_id, cancellation)
                .await
            {
                Ok(result) => {
                    let output_digest = evohime_receipts::sha256_hex(result.output.as_bytes());
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    let runtime = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime.mark_returned(action_id).map_err(|error| FrameError::Io(error.to_string()))?;
                    runtime.complete(&receipt_request, "succeeded", &output_digest, None)
                        .map_err(|error| FrameError::Io(error.to_string()))?;
                    result
                }
                Err(error) => {
                    let mut database = self.journal.database().lock().await;
                    let signer = super::CoreReceiptSigner(Arc::clone(&self.receipt_keys));
                    if let Ok(runtime) = evohime_receipts::runtime::ReceiptRuntime::new(database.connection_mut(), &signer) {
                        let _ = runtime.mark_pending_recovery(action_id, "external_error");
                    }
                    return self
                        .write_response(
                            writer,
                            "terminal.result",
                            serde_json::to_vec(&serde_json::json!({
                                "task_id": task_id.to_string(),
                                "ok": false,
                                "error": error.to_string(),
                            }))?,
                        )
                        .await;
                }
            }
        };
        let bytes = result.output.as_bytes();
        let truncated = bytes.len() > MAX_OUTPUT_BYTES;
        let output = if truncated {
            String::from_utf8_lossy(&bytes[..MAX_OUTPUT_BYTES]).into_owned()
        } else {
            result.output
        };
        self.write_response(
            writer,
            "terminal.result",
            serde_json::to_vec(&serde_json::json!({
                "task_id": task_id.to_string(),
                "ok": true,
                "output": output,
                "structured": result.structured,
                "truncated": truncated,
                "max_bytes": MAX_OUTPUT_BYTES,
            }))?,
        )
        .await
    }

    /// Builds a control envelope (challenge, ready, protocol error) that the
    /// transport layer sends outside the command/response loop. Sequence 0
    /// keeps these events out of the replayable event stream.
    pub fn control_event(
        &self,
        event_type: &str,
        event: Option<generated::event_envelope::Event>,
        payload: Vec<u8>,
    ) -> generated::EventEnvelope {
        generated::EventEnvelope {
            protocol: Some(protocol()),
            sequence_id: 0,
            task_id: String::new(),
            event_type: event_type.into(),
            payload,
            core_instance_id: self.core_instance_id.clone(),
            session_epoch: self.session_epoch,
            event,
        }
    }

    /// The `core.ready` envelope this bridge answers a verified handshake with.
    pub fn ready_event(&self) -> generated::EventEnvelope {
        self.control_event(
            "core.ready",
            Some(generated::event_envelope::Event::Ready(generated::Ready {
                protocol: Some(protocol()),
                core_version: env!("CARGO_PKG_VERSION").into(),
            })),
            Vec::new(),
        )
    }

    async fn start_plan_review<W: AsyncWrite + Unpin>(
        &self,
        request: generated::StartPlanReview,
        writer: &mut W,
    ) -> Result<(), IpcBridgeError> {
        if !request.file_name.to_ascii_lowercase().ends_with(".md") {
            return Err(FrameError::Io("review accepts Markdown files only".into()).into());
        }
        let review = crate::plan_review::ReviewRequest {
            review_id: request.review_id,
            file_name: request.file_name,
            file_names: request.file_names,
            source_markdown: request.source_markdown,
            reviewer_models: request.reviewer_models,
            synthesis_model: request.synthesis_model,
        };
        review
            .validate()
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let gateway_config = self
            .gateway_config
            .clone()
            .ok_or_else(|| FrameError::Io("provider is not configured".into()))?;
        let gateway = evohime_model_gateway::ModelGateway::from_config(&gateway_config)
            .map_err(|error| FrameError::Io(error.to_string()))?;
        let cancellation = CancellationToken::new();
        let review_id = review.review_id.clone();
        self.review_tasks
            .lock()
            .await
            .insert(review_id.clone(), cancellation.clone());
        let tasks = Arc::clone(&self.review_tasks);
        let results = Arc::clone(&self.review_results);
        let journal = self.journal.clone();
        let coordinator = self.coordinator.clone();
        let task_review_id = review_id.clone();
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel();
        let progress = Arc::new(move |progress: crate::plan_review::ReviewProgress| {
            let _ = progress_tx.send(progress);
        });
        tokio::spawn(async move {
            let progress_journal = journal.clone();
            let progress_coordinator = coordinator.clone();
            let progress_writer = tokio::spawn(async move {
                while let Some(progress) = progress_rx.recv().await {
                    publish_review_event(
                        &progress_coordinator,
                        &progress_journal,
                        CoreEvent::ReviewProgress {
                            review_id: progress.review_id,
                            stage: progress.stage,
                            status: progress.status,
                            model: progress.model,
                            completed: progress.completed,
                            total: progress.total,
                        },
                    )
                    .await;
                }
            });
            let event = match crate::plan_review::run_review_with_progress(
                Arc::new(gateway),
                review,
                cancellation,
                progress,
            )
            .await
            {
                Ok(result) => {
                    let payload = serde_json::to_string(&result).unwrap_or_default();
                    results
                        .lock()
                        .await
                        .insert(result.review_id.clone(), result.clone());
                    CoreEvent::TaskCompleted {
                        task_id: result.review_id,
                        final_message: payload,
                    }
                }
                Err(crate::plan_review::ReviewError::Cancelled) => CoreEvent::TaskStopped {
                    task_id: task_review_id.clone(),
                },
                Err(error) => CoreEvent::TaskFailed {
                    task_id: task_review_id.clone(),
                    error: error.to_string(),
                },
            };
            let _ = progress_writer.await;
            let terminal_progress = match &event {
                CoreEvent::TaskCompleted { .. } => Some(CoreEvent::ReviewProgress {
                    review_id: task_review_id.clone(),
                    stage: "completed".into(),
                    status: "completed".into(),
                    model: None,
                    completed: 1,
                    total: 1,
                }),
                CoreEvent::TaskFailed { .. } => Some(CoreEvent::ReviewProgress {
                    review_id: task_review_id.clone(),
                    stage: "failed".into(),
                    status: "failed".into(),
                    model: None,
                    completed: 0,
                    total: 1,
                }),
                _ => None,
            };
            if let Some(progress) = terminal_progress {
                publish_review_event(&coordinator, &journal, progress).await;
            }
            publish_review_event(&coordinator, &journal, event).await;
            tasks.lock().await.remove(&task_review_id);
        });
        self.write_response(
            writer,
            "review.started",
            serde_json::to_vec(&serde_json::json!({
                "review_id": review_id,
                "accepted": true,
            }))
            .unwrap_or_default(),
        )
        .await
    }

    async fn write_response<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        event_type: &str,
        payload: Vec<u8>,
    ) -> Result<(), IpcBridgeError> {
        transport::write_frame(
            writer,
            &generated::EventEnvelope {
                protocol: Some(protocol()),
                sequence_id: 0,
                task_id: String::new(),
                event_type: event_type.into(),
                payload,
                core_instance_id: self.core_instance_id.clone(),
                session_epoch: self.session_epoch,
                event: None,
            }
            .encode_to_vec(),
        )
        .await?;
        Ok(())
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

/// Publishes a review event on the path a connected shell actually reads.
///
/// The pipe server flushes its journal tail only on the coordinator's
/// `journalled` signal, and that signal is raised by the coordinator's own
/// journal writer. An event recorded straight into the journal is durable but
/// stays invisible until some later event wakes the pump, which left a running
/// review looking frozen in the UI. Recording directly is the fallback for a
/// bridge built without a coordinator.
async fn publish_review_event(
    coordinator: &Option<TaskCoordinator>,
    journal: &EventJournal,
    event: CoreEvent,
) {
    match coordinator {
        Some(coordinator) => coordinator.emit(event).await,
        None => {
            let _ = journal.record(&event).await;
        }
    }
}

fn review_result_from_event(payload: &[u8]) -> Option<crate::plan_review::ReviewResult> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let message = value
        .get("TaskCompleted")
        .and_then(|item| item.get("final_message"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            value
                .get("final_message")
                .and_then(serde_json::Value::as_str)
        })?;
    serde_json::from_str(message).ok()
}

fn protocol() -> generated::ProtocolVersion {
    generated::ProtocolVersion {
        major: PROTOCOL_MAJOR,
        minor: PROTOCOL_MINOR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CoreEvent;
    use tokio::io::duplex;

    /// Regression: the clear marker was published on the coordinator broadcast,
    /// so the journal writer recorded it a moment later. The listing that the
    /// panel sends right after the response still read the old marker and kept
    /// showing the reviews that had just been cleared.
    #[tokio::test]
    async fn clearing_history_hides_reviews_from_the_next_listing() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-clear-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "review-old".into(),
                final_message: serde_json::json!({
                    "review_id": "review-old",
                    "file_name": "plan.md",
                    "synthesis_model": "main",
                    "reviewers": [],
                    "final_markdown": "# Итог"
                })
                .to_string(),
            })
            .await
            .expect("review records");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let list_frame = || {
            generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: "review-list".into(),
                client_id: "test-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(generated::command_envelope::Command::ListPlanReviews(
                    generated::ListPlanReviews { limit: 20 },
                )),
            }
            .encode_to_vec()
        };

        transport::write_frame(&mut client, &list_frame())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("list response")
                .as_slice(),
        )
        .expect("list decodes");
        let before: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert_eq!(before["reviews"].as_array().expect("reviews").len(), 1);

        let clear = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "review-clear".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::ClearPlanReviewHistory(
                    generated::ClearPlanReviewHistory {},
                ),
            ),
        };
        transport::write_frame(&mut client, &clear.encode_to_vec())
            .await
            .expect("clear writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("clear serves");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("clear response");

        // The panel lists again as soon as the clear is acknowledged.
        transport::write_frame(&mut client, &list_frame())
            .await
            .expect("second list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("second list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("second list response")
                .as_slice(),
        )
        .expect("second list decodes");
        let after: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert!(
            after["reviews"].as_array().expect("reviews").is_empty(),
            "a cleared history must be empty in the very next listing"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn serves_replay_command_over_framed_transport() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-ipc".into(),
                final_message: "replayed".into(),
            })
            .await
            .expect("event records");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "request-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents { after_sequence: 0 },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("bridge serves replay");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.sequence_id, 1);
        assert_eq!(event.task_id, "task-ipc");
        assert_eq!(event.event_type, "task.completed");
        assert!(String::from_utf8(event.payload)
            .expect("payload utf8")
            .contains("replayed"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_bounded_workspace_list_and_file_read_over_ipc() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-workspace-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).expect("src directory");
        std::fs::write(root.join("README.md"), "hello from workspace").expect("readme");
        let journal_path =
            std::env::temp_dir().join(format!("evohime-ipc-workspace-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&journal_path);
        let bridge = IpcBridge::new(EventJournal::open(&journal_path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "workspace-list".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListWorkspace(
                generated::ListWorkspace {
                    workspace_path: root.display().to_string(),
                    relative_path: ".".into(),
                    max_entries: 10,
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("list reads")
                .as_slice(),
        )
        .expect("list event decodes");
        assert_eq!(response.event_type, "workspace.list");
        let listing: serde_json::Value =
            serde_json::from_slice(&response.payload).expect("list json");
        assert_eq!(listing["entries"][0]["name"], "src");

        let read = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "workspace-read".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReadWorkspaceFile(
                generated::ReadWorkspaceFile {
                    workspace_path: root.display().to_string(),
                    relative_path: "README.md".into(),
                    max_bytes: 100,
                },
            )),
        };
        transport::write_frame(&mut client, &read.encode_to_vec())
            .await
            .expect("read writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("read serves");
        let response = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("read response")
                .as_slice(),
        )
        .expect("read event decodes");
        assert_eq!(response.event_type, "workspace.file");
        let file: serde_json::Value = serde_json::from_slice(&response.payload).expect("file json");
        assert_eq!(file["content"], "hello from workspace");
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(journal_path);
    }

    #[tokio::test]
    async fn terminal_requires_approval_and_denied_retry_does_not_execute() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-terminal-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("terminal root");
        let data_root = std::env::temp_dir().join(format!("evohime-ipc-terminal-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data_root);
        std::fs::create_dir_all(&data_root).expect("terminal data root");
        let journal_path = data_root.join("events.db");
        let _ = std::fs::remove_file(&journal_path);
        let receipt_keys = ReceiptKeyManager::new(&data_root);
        receipt_keys.initialize().expect("receipt keys initialize");
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal,
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );
        let task_id = uuid::Uuid::new_v4().to_string();
        let make_terminal = |approval_id: String| generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "terminal-request".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::TerminalExecute(
                generated::TerminalExecute {
                    task_id: task_id.clone(),
                    workspace_path: root.display().to_string(),
                    program: "git".into(),
                    args: vec!["status".into()],
                    cwd: String::new(),
                    timeout_ms: 5_000,
                    approval_id,
                },
            )),
        };
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &make_terminal(String::new()).encode_to_vec())
            .await
            .expect("terminal writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("approval serves");
        let approval = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("approval reads")
                .as_slice(),
        )
        .expect("approval decodes");
        assert_eq!(approval.event_type, "approval.required");
        let approval_json =
            serde_json::from_slice::<serde_json::Value>(&approval.payload).expect("approval json");
        assert_eq!(approval_json["preview"]["kind"], "command");
        assert_eq!(approval_json["preview"]["command"], "git status");
        let approval_id = approval_json["approval_id"]
            .as_str()
            .expect("approval id")
            .to_string();

        let resolve = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "resolve-terminal".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ResolveApproval(
                generated::ResolveApproval {
                    approval_id: approval_id.clone(),
                    granted: false,
                },
            )),
        };
        transport::write_frame(&mut client, &resolve.encode_to_vec())
            .await
            .expect("resolve writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("resolve serves");

        transport::write_frame(&mut client, &make_terminal(approval_id).encode_to_vec())
            .await
            .expect("retry writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("retry serves");
        let result = generated::EventEnvelope::decode(
            transport::read_frame(&mut client)
                .await
                .expect("result reads")
                .as_slice(),
        )
        .expect("result decodes");
        assert_eq!(result.event_type, "terminal.result");
        let result_json: serde_json::Value =
            serde_json::from_slice(&result.payload).expect("result json");
        assert_eq!(result_json["ok"], false);
        assert_eq!(result_json["error"], "approval was denied for this call");
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(data_root);
    }

    #[tokio::test]
    async fn serves_bounded_git_status_and_diff_through_core_tools() {
        let root =
            std::env::temp_dir().join(format!("evohime-ipc-git-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("git root");
        let status = std::process::Command::new("git")
            .args(["init"])
            .current_dir(&root)
            .status()
            .expect("git init starts");
        assert!(status.success());
        std::fs::write(root.join("notes.txt"), "hello\n").expect("notes");
        let status = std::process::Command::new("git")
            .args(["add", "notes.txt"])
            .current_dir(&root)
            .status()
            .expect("git add starts");
        assert!(status.success());
        std::fs::write(root.join("notes.txt"), "hello\nworld\n").expect("changed notes");
        let journal_path =
            std::env::temp_dir().join(format!("evohime-ipc-git-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&journal_path);
        let journal = EventJournal::open(&journal_path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let tools = Arc::new(ToolRegistry::bootstrap());
        let bridge = IpcBridge::with_coordinator_and_approvals(
            journal,
            coordinator,
            ApprovalCoordinator::default(),
            tools,
            None,
            None,
        );

        let status_payload = bridge
            .dispatch_git_read(
                root.display().to_string(),
                "git.status",
                serde_json::Value::Null,
                128,
            )
            .await
            .expect("git status reads");
        let status_json: serde_json::Value =
            serde_json::from_slice(&status_payload).expect("status json");
        assert!(status_json["output"]
            .as_str()
            .unwrap()
            .contains("notes.txt"));
        assert_eq!(status_json["truncated"], false);

        let diff_payload = bridge
            .dispatch_git_read(
                root.display().to_string(),
                "git.diff",
                serde_json::json!({"path": "notes.txt"}),
                8,
            )
            .await
            .expect("git diff reads");
        let diff_json: serde_json::Value =
            serde_json::from_slice(&diff_payload).expect("diff json");
        assert_eq!(diff_json["max_bytes"], 8);
        assert_eq!(diff_json["truncated"], true);

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_file(journal_path);
    }

    #[tokio::test]
    async fn handshake_exposes_runtime_identity() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-handshake-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "handshake".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 9,
            command: Some(generated::command_envelope::Command::Handshake(
                generated::Handshake {
                    protocol: Some(protocol()),
                    client_id: "client".into(),
                    session_id: "session".into(),
                    session_epoch: 9,
                    last_event_sequence: 0,
                    capabilities: vec!["task.crud".into()],
                    client_role: "shell".into(),
                    nonce: String::new(),
                    proof: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("handshake writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("handshake serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert!(!event.core_instance_id.is_empty());
        assert!(event.session_epoch > 0);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn malformed_command_is_rejected_without_crashing_bridge() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-malformed-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let bridge = IpcBridge::new(EventJournal::open(&path).expect("journal opens"));
        let (mut client, server) = duplex(1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        transport::write_frame(&mut client, &[0xff, 0x00, 0x01])
            .await
            .expect("malformed frame writes");
        assert!(matches!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await,
            Err(IpcBridgeError::Protobuf(_))
        ));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn reconnect_replays_only_events_after_last_sequence() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-reconnect-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let first = journal
            .record(&CoreEvent::TaskStarted {
                task_id: "task-reconnect".into(),
                prompt: "one".into(),
            })
            .await
            .expect("first event");
        journal
            .record(&CoreEvent::TaskCompleted {
                task_id: "task-reconnect".into(),
                final_message: "two".into(),
            })
            .await
            .expect("second event");
        let bridge = IpcBridge::new(journal);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "reconnect".into(),
            client_id: "client".into(),
            core_instance_id: String::new(),
            session_epoch: 2,
            command: Some(generated::command_envelope::Command::ReplayEvents(
                generated::ReplayEvents {
                    after_sequence: first as u64,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("reconnect writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("reconnect serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("event reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "task.completed");
        assert_eq!(event.sequence_id, first as u64 + 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_task_crud_and_replays_deduplicated_create() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-task-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "create-project-1".into(),
            client_id: "test-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-1".into(),
                    title: "Demo".into(),
                    workspace_path: "C:\\Projects\\demo".into(),
                    source_ref: "plan:0a".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let first = transport::read_frame(&mut client)
            .await
            .expect("first response");

        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("duplicate writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("duplicate replays");
        let second = transport::read_frame(&mut client)
            .await
            .expect("second response");
        assert_eq!(first, second);

        let mut conflict = command.clone();
        if let Some(generated::command_envelope::Command::CreateProject(project)) =
            &mut conflict.command
        {
            project.title = "Different".into();
        }
        transport::write_frame(&mut client, &conflict.encode_to_vec())
            .await
            .expect("conflicting writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn imports_prd_without_touching_workspace_and_rejects_duplicate_import() {
        let path = std::env::temp_dir().join(format!("evohime-ipc-prd-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(32 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let project = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "project-prd".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::CreateProject(
                generated::CreateProject {
                    project_id: "project-prd".into(),
                    title: "PRD".into(),
                    workspace_path: "C:\\Projects\\prd".into(),
                    source_ref: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &project.encode_to_vec())
            .await
            .expect("project writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("project creates");
        let _ = transport::read_frame(&mut client)
            .await
            .expect("project response");

        let import = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "import-prd-1".into(),
            client_id: "prd-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ImportPrd(
                generated::ImportPrd {
                    import_id: "import-1".into(),
                    project_id: "project-prd".into(),
                    origin: "prd.md".into(),
                    version: "v1".into(),
                    source_text: "# Plan\n\n## Task\n- [ ] Pass\n".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &import.encode_to_vec())
            .await
            .expect("import writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("import succeeds");
        let response = transport::read_frame(&mut client)
            .await
            .expect("import response");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "prd.imported");
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );

        let mut duplicate = import;
        if let Some(generated::command_envelope::Command::ImportPrd(request)) =
            &mut duplicate.command
        {
            request.source_text.push_str("\n## Another");
            duplicate.request_id = "import-prd-2".into();
        }
        transport::write_frame(&mut client, &duplicate.encode_to_vec())
            .await
            .expect("duplicate writes");
        assert!(bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .is_err());
        assert_eq!(
            journal
                .list_task_graph("project-prd")
                .await
                .unwrap()
                .0
                .len(),
            1
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn serves_run_doctor_with_real_storage_and_pipe_state() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-doctor-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);
        let command = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "doctor-1".into(),
            client_id: "doctor-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunDoctor(
                generated::RunDoctor {
                    project_id: String::new(),
                    detail_level: 1,
                },
            )),
        };
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("doctor serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "doctor.report");
        let report: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("doctor report is valid json");
        assert_eq!(report["bounded"], serde_json::json!(true));
        let checks = report["checks"].as_array().expect("checks array");
        assert_eq!(checks.len(), 7);
        let storage_check = checks
            .iter()
            .find(|check| check["id"] == "storage")
            .expect("storage check present");
        // A freshly-opened journal exists, is writable, and is on the
        // current schema version, so this reflects real (not fabricated)
        // storage state.
        assert_eq!(storage_check["status"], serde_json::json!("OK"));
        let permissions_check = checks
            .iter()
            .find(|check| check["id"] == "permissions")
            .expect("permissions check present");
        // No project_id was supplied, so the permissions probe is honestly
        // fail-closed rather than fabricated as healthy.
        assert_ne!(permissions_check["status"], serde_json::json!("OK"));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn saves_and_lists_research_evidence_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-research-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        let save = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-save-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SaveResearchEvidence(
                generated::SaveResearchEvidence {
                    work_item_id: "task-42".into(),
                    source_kind: "url".into(),
                    source_ref: "https://example.test/article".into(),
                    title: "Example Article".into(),
                    publisher: "Example Org".into(),
                    content_type: "text/html".into(),
                    raw_excerpt: "Useful finding sk-secret alice@example.test".into(),
                    retrieved_at_ms: 1_700_000_000_000,
                    ttl_ms: 3_600_000,
                },
            )),
        };
        transport::write_frame(&mut client, &save.encode_to_vec())
            .await
            .expect("save writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("save serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("save response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.saved");
        let saved: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("save payload is valid json");
        assert_eq!(saved["work_item_id"], serde_json::json!("task-42"));
        let evidence_id = saved["id"].as_str().expect("id present").to_owned();
        assert_eq!(
            saved["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );

        let list = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "research-list-1".into(),
            client_id: "research-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ListResearchEvidence(
                generated::ListResearchEvidence {
                    work_item_id: "task-42".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &list.encode_to_vec())
            .await
            .expect("list writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("list serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("list response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.evidence.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"], serde_json::json!(evidence_id));
        assert_eq!(records[0]["source_kind"], serde_json::json!("url"));
        assert_eq!(
            records[0]["redacted_excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        assert_eq!(records[0]["provenance_link"], serde_json::json!("task-42"));

        let _ = std::fs::remove_file(path);
    }

    fn run_research_fetch_command(
        work_item_id: &str,
        url: String,
        allowed_domains: Vec<String>,
        max_bytes: u64,
    ) -> generated::CommandEnvelope {
        generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: format!("research-fetch-{work_item_id}"),
            client_id: "research-fetch-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RunResearchFetch(
                generated::RunResearchFetch {
                    work_item_id: work_item_id.into(),
                    url,
                    title: "Example Article".into(),
                    allowed_domains,
                    max_bytes,
                    max_latency_ms: 5_000,
                    max_cost_micros: 0,
                    ttl_ms: 3_600_000,
                },
            )),
        }
    }

    #[tokio::test]
    async fn run_research_fetch_persists_real_evidence_from_a_live_http_get() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/article"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .set_body_string("Useful finding sk-secret alice@example.test")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));
        let domain = reqwest::Url::parse(&server.uri())
            .expect("mock uri parses")
            .host_str()
            .expect("mock uri has host")
            .to_ascii_lowercase();

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-ok-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-ok",
            format!("{}/article", server.uri()),
            vec![domain],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await
            .expect("fetch serves");
        let response = transport::read_frame(&mut client)
            .await
            .expect("fetch response reads");
        let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
        assert_eq!(event.event_type, "research.fetch.completed");
        let payload: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("fetch payload is valid json");
        assert_eq!(payload["state"], serde_json::json!("completed"));
        assert_eq!(
            payload["evidence"]["excerpt"],
            serde_json::json!("Useful finding [REDACTED] [REDACTED]")
        );
        let evidence_id = payload["id"].as_str().expect("id present").to_owned();

        let records = journal
            .list_research_evidence("task-fetch-ok")
            .await
            .expect("evidence lists from real storage");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, evidence_id);
        assert_eq!(
            records[0].redacted_excerpt,
            "Useful finding [REDACTED] [REDACTED]"
        );

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_denies_domain_outside_allowlist_and_persists_nothing() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/article"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("should not fetch"))
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-denied-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-denied",
            format!("{}/article", server.uri()),
            vec!["not-the-mock-domain.example".into()],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            outcome.is_err(),
            "domain-denied fetch must fail the command"
        );
        assert_eq!(
            server
                .received_requests()
                .await
                .expect("requests tracked")
                .len(),
            0,
            "no network call should happen for a denied domain"
        );

        let records = journal
            .list_research_evidence("task-fetch-denied")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_blocks_ssrf_targets_and_persists_nothing() {
        let _private = evohime_tool_runtime::lock_private_override(Some(false));

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-ssrf-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-ssrf",
            "http://127.0.0.1:9/".into(),
            vec!["127.0.0.1".into()],
            4096,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(outcome.is_err(), "ssrf-blocked fetch must fail the command");

        let records = journal
            .list_research_evidence("task-fetch-ssrf")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn run_research_fetch_rejects_oversized_response_and_persists_nothing() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .and(wiremock::matchers::path("/big"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("x".repeat(4_096)))
            .mount(&server)
            .await;
        let _private = evohime_tool_runtime::lock_private_override(Some(true));
        let domain = reqwest::Url::parse(&server.uri())
            .expect("mock uri parses")
            .host_str()
            .expect("mock uri has host")
            .to_ascii_lowercase();

        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-research-fetch-oversized-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server_io) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server_io);

        let command = run_research_fetch_command(
            "task-fetch-oversized",
            format!("{}/big", server.uri()),
            vec![domain],
            16,
        );
        transport::write_frame(&mut client, &command.encode_to_vec())
            .await
            .expect("fetch command writes");
        let outcome = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(outcome.is_err(), "oversized response must fail the command");

        let records = journal
            .list_research_evidence("task-fetch-oversized")
            .await
            .expect("list succeeds");
        assert!(records.is_empty(), "no evidence should be persisted");

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_create_list_search_archive_forget_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-memory-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Create two memories in the same task scope, one containing a
        // secret that must come back redacted.
        let create_one = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-1",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Rust build notes".into(),
                content: "Rust build cache lives in target/".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-1".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_one.event_type, "memory.created");
        let created_one: serde_json::Value =
            serde_json::from_slice(&create_one.payload).expect("create payload is valid json");
        let memory_one_id = created_one["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        let create_two = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-create-2",
            generated::command_envelope::Command::CreateMemory(generated::CreateMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                title: "Deployment secret".into(),
                content: "Token is sk-secret, keep it safe".into(),
                provenance_kind: "event".into(),
                provenance_id: "evt-2".into(),
                provenance_locator: String::new(),
                privacy: "internal".into(),
                ttl_ms: 3_600_000,
            }),
        )
        .await;
        assert_eq!(create_two.event_type, "memory.created");
        let created_two: serde_json::Value =
            serde_json::from_slice(&create_two.payload).expect("create payload is valid json");
        assert_eq!(
            created_two["record"]["content"],
            serde_json::json!("Token is [REDACTED] keep it safe")
        );
        let memory_two_id = created_two["record"]["id"]
            .as_str()
            .expect("id present")
            .to_owned();

        // List returns both, newest first.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-1",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "memory.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let records = listed["records"].as_array().expect("records array");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["id"], serde_json::json!(memory_two_id));
        assert_eq!(records[1]["id"], serde_json::json!(memory_one_id));
        assert_eq!(records[0]["project_id"], serde_json::json!("proj-1"));
        assert_eq!(records[0]["secondary_id"], serde_json::json!("task-1"));
        // ListMemory is metadata-only: no statement, no provenance body.
        for record in records {
            assert!(
                record.get("statement").is_none(),
                "list must not carry body"
            );
            assert!(
                record.get("provenance").is_none(),
                "list must not carry provenance body"
            );
            assert_eq!(record["confirmation_state"], serde_json::json!("confirmed"));
            assert_eq!(record["kind"], serde_json::json!("entity"));
        }

        // The body is reachable only through an explicit GetMemory.
        let fetched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-get-1",
            generated::command_envelope::Command::GetMemory(generated::GetMemory {
                id: memory_one_id.clone(),
            }),
        )
        .await;
        assert_eq!(fetched.event_type, "memory.record");
        let body: serde_json::Value =
            serde_json::from_slice(&fetched.payload).expect("get payload is valid json");
        assert_eq!(body["record"]["body_redacted"], serde_json::json!(false));
        assert_eq!(
            body["record"]["statement"],
            serde_json::json!("Rust build cache lives in target/")
        );
        assert_eq!(
            body["supersession_chain"],
            serde_json::json!([memory_one_id])
        );

        // Search only matches the record with "rust" in it.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-1",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                query: "rust".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(search.event_type, "memory.search");
        let searched: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("search payload is valid json");
        let hits = searched["records"].as_array().expect("records array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["id"], serde_json::json!(memory_one_id));

        // Archive without an approval token is rejected.
        let archive_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-archive-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ArchiveMemory(
                generated::ArchiveMemory {
                    id: memory_one_id.clone(),
                    approval_id: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &archive_envelope.encode_to_vec())
            .await
            .expect("archive request writes");
        let denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(denied.is_err(), "archive without approval must fail");

        // Archive with an approval token succeeds and is audited.
        let archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-archive-1",
            generated::command_envelope::Command::ArchiveMemory(generated::ArchiveMemory {
                id: memory_one_id.clone(),
                approval_id: "approval-1".into(),
            }),
        )
        .await;
        assert_eq!(archive.event_type, "memory.archived");
        let archived: serde_json::Value =
            serde_json::from_slice(&archive.payload).expect("archive payload is valid json");
        assert_eq!(archived["archived"], serde_json::json!(true));

        // Archived record is hidden from default listing.
        let list_after_archive = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-2",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: false,
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value = serde_json::from_slice(&list_after_archive.payload)
            .expect("list payload is valid json");
        let records_after = listed_after["records"].as_array().expect("records array");
        assert_eq!(records_after.len(), 1);
        assert_eq!(records_after[0]["id"], serde_json::json!(memory_two_id));

        // Forget with an approval token erases title/content.
        let forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-forget-1",
            generated::command_envelope::Command::ForgetMemory(generated::ForgetMemory {
                id: memory_two_id.clone(),
                approval_id: "approval-2".into(),
            }),
        )
        .await;
        assert_eq!(forget.event_type, "memory.forgotten");
        let forgotten: serde_json::Value =
            serde_json::from_slice(&forget.payload).expect("forget payload is valid json");
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));

        let list_after_forget = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-list-3",
            generated::command_envelope::Command::ListMemory(generated::ListMemory {
                scope_kind: "task".into(),
                project_id: "proj-1".into(),
                secondary_id: "task-1".into(),
                include_archived: true,
                limit: 10,
            }),
        )
        .await;
        let listed_final: serde_json::Value =
            serde_json::from_slice(&list_after_forget.payload).expect("list payload is valid json");
        let records_final = listed_final["records"].as_array().expect("records array");
        // Forgotten records are excluded even with include_archived=true.
        assert!(records_final
            .iter()
            .all(|record| record["id"] != serde_json::json!(memory_two_id)));
        assert_eq!(forgotten["forgotten"], serde_json::json!(true));
        assert!(
            forgotten["tombstone_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty()),
            "forget must produce a tombstone id"
        );

        // A forgotten record still answers GetMemory, but only with metadata.
        let forgotten_body = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-get-forgotten",
            generated::command_envelope::Command::GetMemory(generated::GetMemory {
                id: memory_two_id.clone(),
            }),
        )
        .await;
        let forgotten_json: serde_json::Value =
            serde_json::from_slice(&forgotten_body.payload).expect("payload is valid json");
        assert_eq!(
            forgotten_json["record"]["body_redacted"],
            serde_json::json!(true)
        );
        assert!(forgotten_json["record"].get("statement").is_none());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_pending_conflict_confirm_reject_supersede_round_trip() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-memory-pending-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "memory-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Seed the store directly: extraction candidates are produced by
        // Core's policy gate, not by an IPC caller, so the IPC surface only
        // has to prove that pending records can be reviewed and resolved.
        let seed = |id: &str, state: &str, statement: &str| {
            let mut record = evohime_local_storage::memory_store::MemoryRecord::new(
                id,
                evohime_local_storage::memory_store::MemoryScope::Project,
                "proj-1",
                "Язык интерфейса",
                statement,
                "{\"message_id\":\"msg-1\"}",
                evohime_local_storage::memory_store::MemoryPrivacy::Internal,
                "1000",
                Some("99999999999999".to_owned()),
            )
            .expect("record builds");
            record.extraction = evohime_local_storage::memory_store::MemoryExtractionFields {
                kind: "preference".to_owned(),
                canonical_subject: Some("язык интерфейса".to_owned()),
                confirmation_state: state.to_owned(),
                model_confidence: 0.9,
                verification_confidence: 0.0,
                extractor_version: "extractor-v1".to_owned(),
                policy_version: "extraction-policy-v1".to_owned(),
                ..Default::default()
            };
            record
        };
        journal
            .save_memory(&seed("active-1", "confirmed", "UI на русском языке"))
            .await
            .expect("active memory saves");
        journal
            .save_memory(&seed(
                "pending-1",
                "pending_confirmation",
                "UI на английском языке",
            ))
            .await
            .expect("pending memory saves");
        journal
            .save_memory(&seed(
                "pending-2",
                "pending_confirmation",
                "UI на русском языке",
            ))
            .await
            .expect("duplicate pending saves");

        let pending = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-pending-1",
            generated::command_envelope::Command::ListMemoryPending(generated::ListMemoryPending {
                scope_kind: "project".into(),
                project_id: "proj-1".into(),
                secondary_id: String::new(),
                limit: 10,
                workspace_path: String::new(),
            }),
        )
        .await;
        assert_eq!(pending.event_type, "memory.pending");
        let pending_json: serde_json::Value =
            serde_json::from_slice(&pending.payload).expect("pending payload is valid json");
        assert_eq!(
            pending_json["counts"]["pending_confirmation"],
            serde_json::json!(2)
        );
        assert_eq!(pending_json["counts"]["confirmed"], serde_json::json!(1));
        for record in pending_json["records"].as_array().expect("records array") {
            assert!(
                record.get("statement").is_none(),
                "queue must stay metadata-only"
            );
        }

        // Only the incompatible statement is a conflict; the duplicate is not.
        let conflicts = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-conflicts-1",
            generated::command_envelope::Command::GetMemoryConflicts(
                generated::GetMemoryConflicts {
                    scope_kind: "project".into(),
                    project_id: "proj-1".into(),
                    secondary_id: String::new(),
                    limit: 10,
                    workspace_path: String::new(),
                },
            ),
        )
        .await;
        assert_eq!(conflicts.event_type, "memory.conflicts");
        let conflicts_json: serde_json::Value =
            serde_json::from_slice(&conflicts.payload).expect("conflicts payload is valid json");
        let conflict_list = conflicts_json["conflicts"].as_array().expect("conflicts");
        assert_eq!(conflict_list.len(), 1);
        assert_eq!(
            conflict_list[0]["pending"]["id"],
            serde_json::json!("pending-1")
        );
        assert_eq!(
            conflict_list[0]["active"]["id"],
            serde_json::json!("active-1")
        );

        // "Изменить": the user rewrites the statement before deciding. The
        // record becomes a user assertion but stays pending.
        let revised = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-revise-1",
            generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-1".into(),
                    statement: "UI строго на английском языке".into(),
                    session_only: false,
                    session_id: String::new(),
                    approval_id: "approval-revise".into(),
                    idempotency_key: "key-revise".into(),
                },
            ),
        )
        .await;
        assert_eq!(revised.event_type, "memory.revised");
        let revised_json: serde_json::Value =
            serde_json::from_slice(&revised.payload).expect("payload is valid json");
        assert_eq!(
            revised_json["record"]["confirmation_state"],
            serde_json::json!("pending_confirmation")
        );
        assert_eq!(
            revised_json["record"]["source_trust"],
            serde_json::json!("user")
        );
        assert_eq!(
            revised_json["record"]["extractor_version"],
            serde_json::json!("user_edited")
        );
        // Even the revision response stays metadata-only.
        assert!(revised_json["record"].get("statement").is_none());

        // "Только на эту сессию": no persistent memory survives.
        journal
            .save_memory(&seed(
                "pending-3",
                "pending_confirmation",
                "временное правило",
            ))
            .await
            .expect("third pending saves");
        let session_only = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-session-only-1",
            generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-3".into(),
                    statement: String::new(),
                    session_only: true,
                    session_id: "session-1".into(),
                    approval_id: "approval-session".into(),
                    idempotency_key: "key-session".into(),
                },
            ),
        )
        .await;
        let session_json: serde_json::Value =
            serde_json::from_slice(&session_only.payload).expect("payload is valid json");
        assert_eq!(session_json["session_only"], serde_json::json!(true));
        assert_eq!(session_json["state"], serde_json::json!("rejected"));
        let notes = journal
            .list_memory_session_notes("session-1", &0.to_string())
            .await
            .expect("session notes read");
        assert_eq!(notes.len(), 1, "the statement lives only as a session note");

        // A session-only note without a session id is refused.
        let no_session = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-session-only-bad".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ReviseMemoryCandidate(
                generated::ReviseMemoryCandidate {
                    id: "pending-2".into(),
                    statement: String::new(),
                    session_only: true,
                    session_id: String::new(),
                    approval_id: "approval-session-2".into(),
                    idempotency_key: "key-session-2".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &no_session.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "a session-only note needs a session id"
        );

        // Confirm without approval is rejected.
        let denied_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-confirm-denied".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ConfirmMemory(
                generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: String::new(),
                    idempotency_key: "key-1".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &denied_envelope.encode_to_vec())
            .await
            .expect("denied request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "confirm without approval must fail"
        );

        // Confirm without an idempotency key is rejected too.
        let no_key = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-confirm-no-key".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::ConfirmMemory(
                generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: "approval-1".into(),
                    idempotency_key: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &no_key.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "confirm without an idempotency key must fail"
        );

        // Approved confirm applies, and repeating it is safe.
        for request_id in ["memory-confirm-1", "memory-confirm-1-replay"] {
            let confirmed = send(
                &bridge,
                &mut client,
                &mut server_reader,
                &mut server_writer,
                request_id,
                generated::command_envelope::Command::ConfirmMemory(generated::ConfirmMemory {
                    ids: vec!["pending-1".into()],
                    approval_id: "approval-1".into(),
                    idempotency_key: "key-1".into(),
                }),
            )
            .await;
            assert_eq!(confirmed.event_type, "memory.confirmed");
            let json: serde_json::Value =
                serde_json::from_slice(&confirmed.payload).expect("payload is valid json");
            assert_eq!(json["results"][0]["state"], serde_json::json!("confirmed"));
        }

        // Batch reject is terminal: a later confirm reports the real state.
        let rejected = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-reject-1",
            generated::command_envelope::Command::RejectMemory(generated::RejectMemory {
                ids: vec!["pending-2".into()],
                approval_id: "approval-2".into(),
                idempotency_key: "key-2".into(),
            }),
        )
        .await;
        assert_eq!(rejected.event_type, "memory.rejected");
        let rejected_json: serde_json::Value =
            serde_json::from_slice(&rejected.payload).expect("payload is valid json");
        assert_eq!(
            rejected_json["results"][0]["state"],
            serde_json::json!("rejected")
        );

        let reopen = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-confirm-2",
            generated::command_envelope::Command::ConfirmMemory(generated::ConfirmMemory {
                ids: vec!["pending-2".into()],
                approval_id: "approval-3".into(),
                idempotency_key: "key-3".into(),
            }),
        )
        .await;
        let reopen_json: serde_json::Value =
            serde_json::from_slice(&reopen.payload).expect("payload is valid json");
        assert_eq!(
            reopen_json["results"][0]["state"],
            serde_json::json!("rejected")
        );
        assert_eq!(
            reopen_json["results"][0]["applied"],
            serde_json::json!(false)
        );

        // The conflict is resolved only by an explicit supersede.
        let superseded = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-supersede-1",
            generated::command_envelope::Command::SupersedeMemory(generated::SupersedeMemory {
                old_id: "active-1".into(),
                new_id: "pending-1".into(),
                reason: "user_choice".into(),
                approval_id: "approval-4".into(),
                idempotency_key: "key-4".into(),
            }),
        )
        .await;
        assert_eq!(superseded.event_type, "memory.superseded");
        let superseded_json: serde_json::Value =
            serde_json::from_slice(&superseded.payload).expect("payload is valid json");
        assert_eq!(
            superseded_json["supersession_chain"],
            serde_json::json!(["active-1", "pending-1"])
        );

        // An unsupported reason is refused rather than stored as free text.
        let bad_reason = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "memory-supersede-bad".into(),
            client_id: "memory-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SupersedeMemory(
                generated::SupersedeMemory {
                    old_id: "pending-1".into(),
                    new_id: "pending-2".into(),
                    reason: "because".into(),
                    approval_id: "approval-5".into(),
                    idempotency_key: "key-5".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_reason.encode_to_vec())
            .await
            .expect("request writes");
        assert!(
            bridge
                .process_once(&mut server_reader, &mut server_writer)
                .await
                .is_err(),
            "an unsupported supersession reason must fail"
        );

        // After resolution only the winning record is retrievable.
        let search = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "memory-search-final",
            generated::command_envelope::Command::SearchMemory(generated::SearchMemory {
                scope_kind: "project".into(),
                project_id: "proj-1".into(),
                secondary_id: String::new(),
                query: "ui".into(),
                limit: 10,
            }),
        )
        .await;
        let search_json: serde_json::Value =
            serde_json::from_slice(&search.payload).expect("payload is valid json");
        let hits = search_json["records"].as_array().expect("records array");
        assert_eq!(
            hits.iter()
                .map(|hit| hit["id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            ["pending-1"]
        );

        let _ = std::fs::remove_file(path);
    }

    fn capability_manifest_json(name: &str, version: &str, risk_class: &str) -> String {
        let content_hash = "0123456789abcdef0123456789abcdef";
        let signature =
            crate::capability_registry::test_sign_with_trusted_key(name, version, content_hash);
        serde_json::json!({
            "name": name,
            "version": version,
            "content_hash": content_hash,
            "signature": signature,
            "signing_key_id": "evohime-dev-1",
            "roles": [{
                "name": "reviewer",
                "version": "1",
                "content_hash": "abcdef0123456789abcdef0123456789"
            }],
            "skills": [],
            "allowed_tools": ["filesystem.read", "git.diff"],
            "allowed_domains": ["docs.example.com"],
            "protected_paths": ["src"],
            "risk_class": risk_class,
            "install": {
                "source": "local_archive",
                "allow_install_scripts": false,
                "allow_update": true,
                "rollback_on_failure": true
            }
        })
        .to_string()
    }

    #[tokio::test]
    async fn capability_install_list_match_remove_round_trip_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-capability-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // HTTPS installation requires a real URL and a trusted hash. A
        // request without those inputs must still be rejected before storage.
        let https_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-https".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                    install_source: "https_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &https_envelope.encode_to_vec())
            .await
            .expect("https install request writes");
        let https_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            https_denied.is_err(),
            "https_archive install source must be rejected in this pass"
        );

        // Installing a manifest with a malformed content_hash must be
        // rejected via the real RegistryError::InvalidHash path.
        let bad_hash_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-hash".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-hash", "1.0.0", "medium")
                        .replace("0123456789abcdef0123456789abcdef", "not-a-hex-hash"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_hash_envelope.encode_to_vec())
            .await
            .expect("bad hash install request writes");
        let bad_hash_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_hash_denied.is_err(),
            "manifest with a malformed content_hash must be rejected"
        );

        // Installing a manifest with an invalid risk_class must be rejected
        // before it ever reaches storage.
        let bad_risk_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-install-bad-risk".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::InstallCapability(
                generated::InstallCapability {
                    manifest_json: capability_manifest_json("bad-risk", "1.0.0", "extreme"),
                    install_source: "local_archive".into(),
                    source_path: String::new(),
                    expected_content_hash: String::new(),
                },
            )),
        };
        transport::write_frame(&mut client, &bad_risk_envelope.encode_to_vec())
            .await
            .expect("bad risk install request writes");
        let bad_risk_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            bad_risk_denied.is_err(),
            "manifest with an invalid risk_class must be rejected"
        );

        // A valid local-archive manifest installs successfully.
        let install = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-install-1",
            generated::command_envelope::Command::InstallCapability(generated::InstallCapability {
                manifest_json: capability_manifest_json("reviewer", "1.0.0", "medium"),
                install_source: "local_archive".into(),
                source_path: "C:/archives/reviewer.zip".into(),
                expected_content_hash: String::new(),
            }),
        )
        .await;
        assert_eq!(install.event_type, "capability.installed");
        let installed: serde_json::Value =
            serde_json::from_slice(&install.payload).expect("install payload is valid json");
        assert_eq!(installed["manifest"]["name"], serde_json::json!("reviewer"));

        // List returns the installed manifest.
        let list = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-1",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        assert_eq!(list.event_type, "capability.list");
        let listed: serde_json::Value =
            serde_json::from_slice(&list.payload).expect("list payload is valid json");
        let manifests = listed["manifests"].as_array().expect("manifests array");
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0]["name"], serde_json::json!("reviewer"));

        // Match selects the installed manifest for a fitting query.
        let matched = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-match-1",
            generated::command_envelope::Command::MatchCapabilities(generated::MatchCapabilities {
                intent: "review reviewer".into(),
                required_tools: vec!["git.diff".into()],
                required_domains: vec!["docs.example.com".into()],
                requested_risk: "low".into(),
            }),
        )
        .await;
        assert_eq!(matched.event_type, "capability.match");
        let matches: serde_json::Value =
            serde_json::from_slice(&matched.payload).expect("match payload is valid json");
        let hits = matches["matches"].as_array().expect("matches array");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["manifest_name"], serde_json::json!("reviewer"));

        // Remove deletes the manifest.
        let removed = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-remove-1",
            generated::command_envelope::Command::RemoveCapability(generated::RemoveCapability {
                id: "reviewer".into(),
            }),
        )
        .await;
        assert_eq!(removed.event_type, "capability.removed");
        let removed_payload: serde_json::Value =
            serde_json::from_slice(&removed.payload).expect("remove payload is valid json");
        assert_eq!(removed_payload["removed"], serde_json::json!(true));

        // Removing again is rejected: the manifest is already gone.
        let remove_again_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-remove-2".into(),
            client_id: "capability-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::RemoveCapability(
                generated::RemoveCapability {
                    id: "reviewer".into(),
                },
            )),
        };
        transport::write_frame(&mut client, &remove_again_envelope.encode_to_vec())
            .await
            .expect("second remove request writes");
        let remove_again = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            remove_again.is_err(),
            "removing a manifest that no longer exists must fail"
        );

        let list_after_remove = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-list-2",
            generated::command_envelope::Command::ListCapabilities(generated::ListCapabilities {
                limit: 10,
            }),
        )
        .await;
        let listed_after: serde_json::Value =
            serde_json::from_slice(&list_after_remove.payload).expect("list payload is valid json");
        assert!(listed_after["manifests"]
            .as_array()
            .expect("manifests array")
            .is_empty());

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn capability_selection_get_pin_replace_round_trip_against_real_storage() {
        let path = std::env::temp_dir().join(format!(
            "evohime-ipc-capability-selection-{}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "capability-selection-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // Install two candidate manifests so replace() has a real
        // alternative to switch to.
        for name in ["reviewer", "planner"] {
            let install = send(
                &bridge,
                &mut client,
                &mut server_reader,
                &mut server_writer,
                &format!("capability-selection-install-{name}"),
                generated::command_envelope::Command::InstallCapability(
                    generated::InstallCapability {
                        manifest_json: capability_manifest_json(name, "1.0.0", "medium"),
                        install_source: "local_archive".into(),
                        source_path: format!("C:/archives/{name}.zip"),
                        expected_content_hash: String::new(),
                    },
                ),
            )
            .await;
            assert_eq!(install.event_type, "capability.installed");
        }

        let query_fields = || generated::GetCapabilitySelection {
            task_id: "task-1".into(),
            intent: "review reviewer".into(),
            required_tools: vec!["git.diff".into()],
            required_domains: vec!["docs.example.com".into()],
            requested_risk: "low".into(),
        };

        // First GetCapabilitySelection: no prior state, so the matcher's
        // top-scoring manifest is auto-selected and persisted.
        let selected = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-1",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        assert_eq!(selected.event_type, "capability.selection");
        let selected_json: serde_json::Value =
            serde_json::from_slice(&selected.payload).expect("selection payload is valid json");
        assert_eq!(
            selected_json["selection"]["manifest_name"],
            serde_json::json!("reviewer")
        );
        assert_eq!(selected_json["origin"], serde_json::json!("auto"));

        // Pinning persists origin=pinned for the same task_id.
        let pinned = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-pin-1",
            generated::command_envelope::Command::PinCapabilitySelection(
                generated::PinCapabilitySelection {
                    task_id: "task-1".into(),
                },
            ),
        )
        .await;
        assert_eq!(pinned.event_type, "capability.selection.pinned");
        let pinned_json: serde_json::Value =
            serde_json::from_slice(&pinned.payload).expect("pin payload is valid json");
        assert_eq!(pinned_json["origin"], serde_json::json!("pinned"));
        assert!(pinned_json["selection"]["pinned"].as_bool().unwrap());

        // A subsequent GetCapabilitySelection must not silently override the
        // pin, even though the matcher would still pick "reviewer" here --
        // the persisted origin stays "pinned".
        let reconciled = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-2",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        let reconciled_json: serde_json::Value =
            serde_json::from_slice(&reconciled.payload).expect("selection payload is valid json");
        assert_eq!(reconciled_json["origin"], serde_json::json!("pinned"));

        // Explicitly replacing switches the persisted selection to
        // "planner" and marks origin=replaced.
        let replaced = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-replace-1",
            generated::command_envelope::Command::ReplaceCapabilitySelection(
                generated::ReplaceCapabilitySelection {
                    task_id: "task-1".into(),
                    manifest_name: "planner".into(),
                    intent: "review reviewer".into(),
                    required_tools: vec!["git.diff".into()],
                    required_domains: vec!["docs.example.com".into()],
                    requested_risk: "low".into(),
                },
            ),
        )
        .await;
        assert_eq!(replaced.event_type, "capability.selection.replaced");
        let replaced_json: serde_json::Value =
            serde_json::from_slice(&replaced.payload).expect("replace payload is valid json");
        assert_eq!(
            replaced_json["selection"]["manifest_name"],
            serde_json::json!("planner")
        );
        assert_eq!(replaced_json["origin"], serde_json::json!("replaced"));

        // A fresh GetCapabilitySelection still returns the replaced choice
        // -- proving persistence survives a new request (simulated
        // reconnect), matching the store's own round-trip contract.
        let after_replace = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "capability-selection-get-3",
            generated::command_envelope::Command::GetCapabilitySelection(query_fields()),
        )
        .await;
        let after_replace_json: serde_json::Value = serde_json::from_slice(&after_replace.payload)
            .expect("selection payload is valid json");
        assert_eq!(
            after_replace_json["selection"]["manifest_name"],
            serde_json::json!("planner")
        );
        assert_eq!(after_replace_json["origin"], serde_json::json!("replaced"));

        // Pinning for a task_id with no persisted selection must fail.
        let pin_missing_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "capability-selection-pin-missing".into(),
            client_id: "capability-selection-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(
                generated::command_envelope::Command::PinCapabilitySelection(
                    generated::PinCapabilitySelection {
                        task_id: "task-never-selected".into(),
                    },
                ),
            ),
        };
        transport::write_frame(&mut client, &pin_missing_envelope.encode_to_vec())
            .await
            .expect("pin-missing request writes");
        let pin_missing = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            pin_missing.is_err(),
            "pinning a task with no persisted selection must fail"
        );

        let _ = std::fs::remove_file(path);
    }

    /// Proves the read-only child delegation boundary holds end-to-end
    /// through the real IPC command path, not just at the pure-function
    /// level (`child_runtime::ChildTaskRequest::validate` /
    /// `child_runtime::accept_report` unit tests): a request naming a
    /// non-read-only capability is rejected, a nested-child request is
    /// rejected, a report with secret-like content is rejected, and a
    /// valid read-only request plus matching valid report round-trips
    /// through save -> submit -> list successfully. This test does not
    /// spawn or execute any child agent; it only proves the
    /// request/report validation and persistence boundary.
    #[tokio::test]
    async fn child_handoff_request_report_security_boundary_against_real_storage() {
        let path =
            std::env::temp_dir().join(format!("evohime-ipc-child-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let journal = EventJournal::open(&path).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator);
        let (mut client, server) = duplex(16 * 1024);
        let (mut server_reader, mut server_writer) = tokio::io::split(server);

        async fn send(
            bridge: &IpcBridge,
            client: &mut tokio::io::DuplexStream,
            server_reader: &mut (impl tokio::io::AsyncRead + Unpin),
            server_writer: &mut (impl tokio::io::AsyncWrite + Unpin),
            request_id: &str,
            command: generated::command_envelope::Command,
        ) -> generated::EventEnvelope {
            let envelope = generated::CommandEnvelope {
                protocol: Some(protocol()),
                request_id: request_id.into(),
                client_id: "child-client".into(),
                core_instance_id: String::new(),
                session_epoch: 1,
                command: Some(command),
            };
            transport::write_frame(client, &envelope.encode_to_vec())
                .await
                .expect("request writes");
            bridge
                .process_once(server_reader, server_writer)
                .await
                .expect("request serves");
            let response = transport::read_frame(client).await.expect("response reads");
            generated::EventEnvelope::decode(response.as_slice()).expect("event decodes")
        }

        // (a) A request naming a non-read-only capability (workspace.write)
        // must be rejected end-to-end, not just by the pure function.
        let write_capability_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-write-capability".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-write".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.write".into()],
                    parent_is_child: false,
                },
            )),
        };
        transport::write_frame(&mut client, &write_capability_envelope.encode_to_vec())
            .await
            .expect("write-capability request writes");
        let write_capability_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            write_capability_denied.is_err(),
            "a request naming a non-read-only capability must be rejected"
        );

        // (b) A nested child (parent_is_child = true) must be rejected.
        let nested_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-request-nested".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-nested".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into()],
                    parent_is_child: true,
                },
            )),
        };
        transport::write_frame(&mut client, &nested_envelope.encode_to_vec())
            .await
            .expect("nested request writes");
        let nested_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            nested_denied.is_err(),
            "a nested child request (parent_is_child = true) must be rejected"
        );

        // A valid read-only request submits successfully.
        let submitted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-request-valid",
            generated::command_envelope::Command::SubmitChildRequest(
                generated::SubmitChildRequest {
                    child_task_id: "child-1".into(),
                    parent_task_id: "task-1".into(),
                    role: "researcher".into(),
                    kind: "code_search".into(),
                    reduced_context: vec!["inspect src".into()],
                    max_output_bytes: 4096,
                    requested_capabilities: vec!["workspace.read".into(), "git.diff".into()],
                    parent_is_child: false,
                },
            ),
        )
        .await;
        assert_eq!(submitted.event_type, "child.request.submitted");
        let submitted_payload: serde_json::Value =
            serde_json::from_slice(&submitted.payload).expect("submit payload is valid json");
        assert_eq!(
            submitted_payload["request"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // (c) A report containing secret-like content must be rejected,
        // even though it matches a valid, already-persisted request.
        let secret_report_envelope = generated::CommandEnvelope {
            protocol: Some(protocol()),
            request_id: "child-report-secret".into(),
            client_id: "child-client".into(),
            core_instance_id: String::new(),
            session_epoch: 1,
            command: Some(generated::command_envelope::Command::SubmitChildReport(
                generated::SubmitChildReport {
                    child_task_id: "child-1".into(),
                    status: "complete".into(),
                    summary: "api_key=do-not-leak".into(),
                    findings: vec!["module is bounded".into()],
                    sources: vec!["src/lib.rs:10".into()],
                    confidence_percent: 90,
                },
            )),
        };
        transport::write_frame(&mut client, &secret_report_envelope.encode_to_vec())
            .await
            .expect("secret report writes");
        let secret_report_denied = bridge
            .process_once(&mut server_reader, &mut server_writer)
            .await;
        assert!(
            secret_report_denied.is_err(),
            "a report containing secret-like content must be rejected"
        );

        // (d) A matching, valid report round-trips through
        // save -> submit -> list successfully.
        let accepted = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-report-valid",
            generated::command_envelope::Command::SubmitChildReport(generated::SubmitChildReport {
                child_task_id: "child-1".into(),
                status: "complete".into(),
                summary: "found one relevant module".into(),
                findings: vec!["module is bounded".into()],
                sources: vec!["src/lib.rs:10".into()],
                confidence_percent: 90,
            }),
        )
        .await;
        assert_eq!(accepted.event_type, "child.report.accepted");
        let accepted_payload: serde_json::Value =
            serde_json::from_slice(&accepted.payload).expect("report payload is valid json");
        assert_eq!(
            accepted_payload["report"]["child_task_id"],
            serde_json::json!("child-1")
        );

        // A separately requested handoff persists and lists back through
        // the real command path too.
        let handoff = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-valid",
            generated::command_envelope::Command::RequestChildHandoff(
                generated::RequestChildHandoff {
                    handoff_id: "handoff-1".into(),
                    task_id: "task-1".into(),
                    kind: "delegate".into(),
                    from_role: "coordinator".into(),
                    from_name: String::new(),
                    to_role: "researcher".into(),
                    to_name: String::new(),
                    purpose: "investigate module bounds".into(),
                    payload: std::collections::HashMap::new(),
                    sequence: 1,
                },
            ),
        )
        .await;
        assert_eq!(handoff.event_type, "child.handoff.requested");

        let listed_handoffs = send(
            &bridge,
            &mut client,
            &mut server_reader,
            &mut server_writer,
            "child-handoff-list",
            generated::command_envelope::Command::ListChildHandoffs(generated::ListChildHandoffs {
                task_id: "task-1".into(),
                limit: 10,
            }),
        )
        .await;
        assert_eq!(listed_handoffs.event_type, "child.handoff.list");
        let listed_handoffs_payload: serde_json::Value =
            serde_json::from_slice(&listed_handoffs.payload)
                .expect("handoff list payload is valid json");
        let handoffs = listed_handoffs_payload["handoffs"]
            .as_array()
            .expect("handoffs array");
        assert_eq!(handoffs.len(), 1);
        assert_eq!(handoffs[0]["handoff_id"], serde_json::json!("handoff-1"));

        let _ = std::fs::remove_file(path);
    }
}
