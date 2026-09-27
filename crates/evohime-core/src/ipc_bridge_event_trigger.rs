use super::*;

const EVENT_TRIGGER_OWNER_SCOPE: &str = "settings";
const EVENT_TRIGGER_REPLAY_PAGE: usize = 256;
const MAX_EVENT_TRIGGER_WATCH_ROOTS: usize = 32;

#[derive(Debug)]
enum WorkspaceWatchMessage {
    Changed {
        root: std::path::PathBuf,
        path: std::path::PathBuf,
        change_kind: &'static str,
    },
    Closed(std::path::PathBuf),
}

#[cfg(windows)]
struct WorkspaceWatcher {
    thread_handle: isize,
    join: tokio::task::JoinHandle<()>,
}

#[cfg(windows)]
impl WorkspaceWatcher {
    async fn start(
        root: std::path::PathBuf,
        sender: tokio::sync::mpsc::Sender<WorkspaceWatchMessage>,
    ) -> Result<Self, String> {
        let (thread_id_sender, thread_id_receiver) = tokio::sync::oneshot::channel();
        let (continue_sender, continue_receiver) = std::sync::mpsc::sync_channel(1);
        let worker_root = root.clone();
        let join = tokio::task::spawn_blocking(move || {
            let thread_id = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
            let _ = thread_id_sender.send(thread_id);
            if !continue_receiver.recv().unwrap_or(false) {
                return;
            }
            watch_workspace_directory(worker_root, sender);
        });
        let thread_id = thread_id_receiver
            .await
            .map_err(|_| "watcher_thread_stopped".to_owned())?;
        let thread_handle = unsafe {
            windows_sys::Win32::System::Threading::OpenThread(
                windows_sys::Win32::System::Threading::THREAD_TERMINATE,
                0,
                thread_id,
            )
        };
        if thread_handle.is_null() {
            let _ = continue_sender.send(false);
            let _ = join.await;
            return Err("watcher_thread_open_failed".into());
        }
        if continue_sender.send(true).is_err() {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(thread_handle) };
            let _ = join.await;
            return Err("watcher_thread_stopped".into());
        }
        Ok(Self {
            thread_handle: thread_handle as isize,
            join,
        })
    }

    fn stop(self) {
        tokio::spawn(async move {
            while !self.join.is_finished() {
                unsafe {
                    windows_sys::Win32::System::IO::CancelSynchronousIo(
                        self.thread_handle as windows_sys::Win32::Foundation::HANDLE,
                    )
                };
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(
                    self.thread_handle as windows_sys::Win32::Foundation::HANDLE,
                )
            };
            let _ = self.join.await;
        });
    }
}

impl IpcBridge {
    /// Processes newly journalled Core task outcomes as event triggers.
    ///
    /// The durable journal is the source of truth; the watch channel is only
    /// a wake-up signal. Filesystem and webhook sources remain independent.
    pub async fn run_event_trigger_event_source(
        &self,
        cancellation: tokio_util::sync::CancellationToken,
    ) {
        let Some(mut journalled) = self.journalled() else {
            return;
        };
        let mut sequence = self.journal.latest_sequence().await.max(0);
        let mut runtime = crate::event_trigger_runtime::Runtime::default();
        let (watch_sender, mut watch_receiver) =
            tokio::sync::mpsc::channel::<WorkspaceWatchMessage>(128);
        #[cfg(windows)]
        let mut watchers: std::collections::HashMap<std::path::PathBuf, WorkspaceWatcher> =
            std::collections::HashMap::new();
        let mut watcher_refresh = tokio::time::interval(std::time::Duration::from_secs(2));
        watcher_refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                changed = journalled.changed() => {
                    if changed.is_err() { break; }
                    let announced = (*journalled.borrow_and_update()).min(i64::MAX as u64) as i64;
                    while sequence < announced {
                        let page = match self.journal.replay(sequence, EVENT_TRIGGER_REPLAY_PAGE).await {
                            Ok(page) => page,
                            Err(error) => {
                                tracing::error!(%error, "event trigger journal replay failed");
                                break;
                            }
                        };
                        if page.is_empty() { break; }
                        for record in page {
                            sequence = record.sequence_id;
                            if let Err(error) = self.dispatch_journalled_event_trigger(&record, &mut runtime).await {
                                tracing::error!(sequence = record.sequence_id, %error, "event trigger dispatch failed");
                            }
                        }
                    }
                }
                Some(message) = watch_receiver.recv() => {
                    match message {
                        WorkspaceWatchMessage::Changed { root, path, change_kind } => {
                            if let Err(error) = self.dispatch_workspace_event_trigger(&root, &path, change_kind, &mut runtime).await {
                                tracing::error!(root = %root.display(), path = %path.display(), %error, "workspace event trigger dispatch failed");
                            }
                        }
                        WorkspaceWatchMessage::Closed(root) => {
                            #[cfg(windows)]
                            if let Some(watcher) = watchers.remove(&root) { watcher.stop(); }
                        }
                    }
                }
                _ = watcher_refresh.tick() => {
                    #[cfg(windows)]
                    if let Err(error) = self.refresh_workspace_watchers(&mut watchers, watch_sender.clone()).await {
                        tracing::warn!(%error, "workspace event watcher refresh failed");
                    }
                }
            }
        }
        #[cfg(windows)]
        for (_, watcher) in watchers {
            watcher.stop();
        }
    }

    pub(super) async fn dispatch_journalled_event_trigger(
        &self,
        record: &evohime_local_storage::EventRecord,
        runtime: &mut crate::event_trigger_runtime::Runtime,
    ) -> Result<(), String> {
        let event_kind = match record.event_type.as_str() {
            "task.completed" => "task_completed",
            "task.failed" => "task_failed",
            _ => return Ok(()),
        };
        if record.task_id.starts_with("event-trigger-") {
            return Ok(());
        }
        let triggers = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::list_event_trigger_definitions::<
                crate::event_trigger_runtime::TriggerDefinition,
            >(database.connection(), EVENT_TRIGGER_OWNER_SCOPE)
            .map_err(|error| error.to_string())?
        };
        for stored in triggers {
            let definition = stored.definition;
            if definition.state != crate::event_trigger_runtime::TriggerState::Active
                || definition.source_kind != crate::event_trigger_runtime::SourceKind::SystemEvent
                || definition.event_kind != event_kind
            {
                continue;
            }
            let event_key = format!("journal:{}", record.sequence_id);
            let event_id = trigger_event_id(&definition.trigger_id, &event_key);
            let payload = serde_json::json!({
                "task_id": record.task_id,
                "outcome": event_kind,
                "workspace": definition.workspace_path,
            });
            let payload_hash = crate::event_trigger_runtime::canonical_hash(&payload)
                .map_err(|error| error.to_string())?;
            let envelope = crate::event_trigger_runtime::EventEnvelope {
                contract_version: crate::event_trigger_runtime::CONTRACT_VERSION.into(),
                event_id: event_id.clone(),
                trigger_id: definition.trigger_id.clone(),
                source_kind: definition.source_kind,
                event_kind: definition.event_kind.clone(),
                schema_version: 1,
                received_at_ms: crate::task_memory::now_millis() as i64,
                payload,
                payload_hash,
                authenticity: "core_local".into(),
                origin: "core_journal".into(),
                correlation_id: record.sequence_id.to_string(),
                provider_event_key: Some(event_key),
                chain_depth: 0,
            };
            let admission = runtime
                .ingest(&definition, &envelope, envelope.received_at_ms)
                .map_err(|error| format!("trigger admission failed: {error:?}"))?;
            if admission.outcome != crate::event_trigger_runtime::EventOutcome::Pending {
                self.record_trigger_outcome(&envelope, admission.outcome, admission.error_code)
                    .await?;
                continue;
            }
            let result = self
                .start_trigger_workflow(&definition, record.sequence_id, admission.mapped_input)
                .await;
            runtime.complete(&definition.trigger_id);
            match result {
                Ok(()) => {
                    self.record_trigger_outcome(
                        &envelope,
                        crate::event_trigger_runtime::EventOutcome::Dispatched,
                        None,
                    )
                    .await?;
                }
                Err(code) => {
                    self.record_trigger_outcome(
                        &envelope,
                        crate::event_trigger_runtime::EventOutcome::Rejected,
                        Some(code.clone()),
                    )
                    .await?;
                    tracing::warn!(trigger_id = %definition.trigger_id, %code, "event trigger workflow was not started");
                }
            }
        }
        Ok(())
    }

    pub(super) async fn dispatch_workspace_event_trigger(
        &self,
        root: &std::path::Path,
        path: &std::path::Path,
        change_kind: &str,
        runtime: &mut crate::event_trigger_runtime::Runtime,
    ) -> Result<(), String> {
        let triggers = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::list_event_trigger_definitions::<
                crate::event_trigger_runtime::TriggerDefinition,
            >(database.connection(), EVENT_TRIGGER_OWNER_SCOPE)
            .map_err(|error| error.to_string())?
        };
        let root = root
            .canonicalize()
            .map_err(|error| format!("workspace_root_unavailable:{error}"))?;
        let path = match path.canonicalize() {
            Ok(path) => path,
            Err(_error) if !path.exists() => path.to_path_buf(),
            Err(error) => return Err(format!("changed_path_unavailable:{error}")),
        };
        let relative_path = path
            .strip_prefix(&root)
            .map_err(|_| "changed_path_outside_workspace".to_owned())?;
        let event_key = crate::event_trigger_runtime::canonical_hash(&serde_json::json!({
            "root": root.to_string_lossy().to_string(),
            "path": relative_path.to_string_lossy().to_string(),
            "change_kind": change_kind,
            "debounce_bucket": crate::task_memory::now_millis() as i64 / 750,
        }))
        .map_err(|error| error.to_string())?;
        let suppress_recursive_events = self.has_running_trigger_workflow(&root).await?;
        for stored in triggers {
            let definition = stored.definition;
            if definition.state != crate::event_trigger_runtime::TriggerState::Active
                || definition.source_kind
                    != crate::event_trigger_runtime::SourceKind::LocalWorkspaceEvent
                || definition.event_kind != "file_changed"
                || std::path::Path::new(&definition.workspace_path)
                    .canonicalize()
                    .map(|candidate| candidate != root)
                    .unwrap_or(true)
            {
                continue;
            }
            let payload = serde_json::json!({
                "path": path.to_string_lossy(),
                "change_kind": change_kind,
                "workspace": root.to_string_lossy(),
            });
            let envelope = crate::event_trigger_runtime::EventEnvelope {
                contract_version: crate::event_trigger_runtime::CONTRACT_VERSION.into(),
                event_id: trigger_event_id(&definition.trigger_id, &event_key),
                trigger_id: definition.trigger_id.clone(),
                source_kind: definition.source_kind,
                event_kind: definition.event_kind.clone(),
                schema_version: 1,
                received_at_ms: crate::task_memory::now_millis() as i64,
                payload,
                payload_hash: event_key.clone(),
                authenticity: "core_local".into(),
                origin: "workspace_file_system".into(),
                correlation_id: event_key.clone(),
                provider_event_key: Some(event_key.clone()),
                chain_depth: 0,
            };
            if suppress_recursive_events {
                self.record_trigger_outcome(
                    &envelope,
                    crate::event_trigger_runtime::EventOutcome::DroppedWithAudit,
                    Some("trigger_workflow_active_in_workspace".into()),
                )
                .await?;
                continue;
            }
            let admission = runtime
                .ingest(&definition, &envelope, envelope.received_at_ms)
                .map_err(|error| format!("trigger admission failed: {error:?}"))?;
            if admission.outcome != crate::event_trigger_runtime::EventOutcome::Pending {
                self.record_trigger_outcome(&envelope, admission.outcome, admission.error_code)
                    .await?;
                continue;
            }
            let source_sequence = i64::from_str_radix(&event_key[..15], 16)
                .map_err(|error| format!("invalid_workspace_event_key:{error}"))?;
            let result = self
                .start_trigger_workflow(&definition, source_sequence, admission.mapped_input)
                .await;
            runtime.complete(&definition.trigger_id);
            match result {
                Ok(()) => {
                    self.record_trigger_outcome(
                        &envelope,
                        crate::event_trigger_runtime::EventOutcome::Dispatched,
                        None,
                    )
                    .await?
                }
                Err(code) => {
                    self.record_trigger_outcome(
                        &envelope,
                        crate::event_trigger_runtime::EventOutcome::Rejected,
                        Some(code.clone()),
                    )
                    .await?;
                    tracing::warn!(trigger_id = %definition.trigger_id, %code, "workspace event trigger workflow was not started");
                }
            }
        }
        Ok(())
    }

    async fn has_running_trigger_workflow(
        &self,
        workspace: &std::path::Path,
    ) -> Result<bool, String> {
        let runs = self
            .journal
            .list_workflow_runs(512)
            .await
            .map_err(|error| error.to_string())?;
        for run in runs {
            if !run.task_id.starts_with("event-trigger-") || run.state.is_terminal() {
                continue;
            }
            let policy: serde_json::Value = serde_json::from_str(&run.policy_json)
                .map_err(|error| format!("trigger_workflow_policy_corrupt:{error}"))?;
            let Some(workspace_path) = policy.get("workspace_path").and_then(|v| v.as_str()) else {
                return Err("trigger_workflow_workspace_missing".into());
            };
            if std::path::Path::new(workspace_path)
                .canonicalize()
                .is_ok_and(|active_workspace| active_workspace == workspace)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    #[cfg(windows)]
    async fn refresh_workspace_watchers(
        &self,
        watchers: &mut std::collections::HashMap<std::path::PathBuf, WorkspaceWatcher>,
        sender: tokio::sync::mpsc::Sender<WorkspaceWatchMessage>,
    ) -> Result<(), String> {
        let triggers = {
            let database = self.journal.database().lock().await;
            evohime_local_storage::list_event_trigger_definitions::<
                crate::event_trigger_runtime::TriggerDefinition,
            >(database.connection(), EVENT_TRIGGER_OWNER_SCOPE)
            .map_err(|error| error.to_string())?
        };
        let desired: std::collections::HashSet<std::path::PathBuf> = triggers
            .into_iter()
            .filter(|stored| {
                stored.definition.state == crate::event_trigger_runtime::TriggerState::Active
                    && stored.definition.source_kind
                        == crate::event_trigger_runtime::SourceKind::LocalWorkspaceEvent
            })
            .filter_map(|stored| {
                std::path::Path::new(&stored.definition.workspace_path)
                    .canonicalize()
                    .ok()
                    .filter(|path| path.is_dir())
            })
            .collect();
        let stale: Vec<_> = watchers
            .iter()
            .filter(|(path, watcher)| !desired.contains(*path) || watcher.join.is_finished())
            .map(|(path, _)| path)
            .cloned()
            .collect();
        for root in stale {
            if let Some(watcher) = watchers.remove(&root) {
                watcher.stop();
            }
        }
        let active_roots: std::collections::HashSet<_> = watchers.keys().cloned().collect();
        for root in desired.difference(&active_roots) {
            if watchers.len() >= MAX_EVENT_TRIGGER_WATCH_ROOTS {
                return Err("workspace_watch_root_limit".into());
            }
            match WorkspaceWatcher::start(root.clone(), sender.clone()).await {
                Ok(watcher) => {
                    watchers.insert(root.clone(), watcher);
                }
                Err(error) => {
                    tracing::warn!(root = %root.display(), %error, "workspace watcher could not start")
                }
            }
        }
        Ok(())
    }

    async fn record_trigger_outcome(
        &self,
        envelope: &crate::event_trigger_runtime::EventEnvelope,
        outcome: crate::event_trigger_runtime::EventOutcome,
        error_code: Option<String>,
    ) -> Result<(), String> {
        let outcome_name = match outcome {
            crate::event_trigger_runtime::EventOutcome::Pending => "pending",
            crate::event_trigger_runtime::EventOutcome::Dispatched => "dispatched",
            crate::event_trigger_runtime::EventOutcome::Rejected => "rejected",
            crate::event_trigger_runtime::EventOutcome::DuplicateIgnored => "duplicate_ignored",
            crate::event_trigger_runtime::EventOutcome::Throttled => "throttled",
            crate::event_trigger_runtime::EventOutcome::DroppedWithAudit => "dropped_with_audit",
            _ => "rejected",
        };
        let mut payload = serde_json::to_value(envelope).map_err(|error| error.to_string())?;
        if let Some(object) = payload.as_object_mut() {
            object.remove("payload");
            object.insert("error_code".into(), serde_json::json!(error_code));
        }
        let now = crate::task_memory::now_millis() as i64;
        let mut database = self.journal.database().lock().await;
        evohime_local_storage::record_event_trigger_event(
            database.connection_mut(),
            &payload,
            &evohime_local_storage::EventTriggerEventRecordMeta {
                event_id: &envelope.event_id,
                trigger_id: &envelope.trigger_id,
                outcome: outcome_name,
                correlation_id: &envelope.correlation_id,
                accepted_at_ms: now,
                expires_at_ms: now.saturating_add(crate::event_trigger_runtime::DEDUP_TTL_MS),
            },
        )
        .map_err(|error| error.to_string())
    }

    async fn start_trigger_workflow(
        &self,
        definition: &crate::event_trigger_runtime::TriggerDefinition,
        source_sequence: i64,
        mapped: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let template = crate::workflow_templates::template(&definition.workflow.workflow_id)
            .ok_or_else(|| "unknown_workflow".to_owned())?;
        if u64::from(template.version) != definition.workflow.workflow_version
            || crate::event_trigger_runtime::canonical_hash(template.graph())
                .map_err(|error| error.to_string())?
                != definition.workflow.execution_hash
        {
            return Err("workflow_binding_invalid".into());
        }
        let mapped = mapped.ok_or_else(|| "mapping_rejected".to_owned())?;
        let object = mapped
            .as_object()
            .ok_or_else(|| "mapping_rejected".to_owned())?;
        let inputs = object
            .iter()
            .map(|(key, value)| {
                value
                    .as_str()
                    .map(|value| (key.clone(), value.to_owned()))
                    .ok_or_else(|| "mapping_rejected".to_owned())
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let graph = template
            .instantiate(&inputs)
            .map_err(|error| error.code().to_owned())?;
        let run_id = trigger_run_id(&definition.trigger_id, source_sequence);
        if self
            .journal
            .workflow_run(&run_id)
            .await
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(());
        }
        let workspace_path = definition.workspace_path.clone();
        let start = crate::workflow_runtime::StartWorkflowRequest {
            run_id: run_id.clone(),
            task_id: run_id.clone(),
            workspace_path: workspace_path.clone(),
            template_id: template.template_id.clone(),
            template_version: template.version,
            inputs,
            graph,
            parent: workflow_parent_capabilities(),
        };
        let runtime = self.workflow_runtime(&workspace_path);
        runtime
            .start(start)
            .await
            .map_err(|error| error.code().to_string())?;
        if !self.spawn_workflow_drive(run_id, workspace_path).await {
            return Err("background_capacity_exhausted".into());
        }
        Ok(())
    }
}

fn trigger_event_id(trigger_id: &str, event_key: &str) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(format!("{trigger_id}:{event_key}").as_bytes());
    format!("event-{}", hex::encode(&hash[..16]))
}

pub(super) fn trigger_run_id(trigger_id: &str, sequence: i64) -> String {
    use sha2::Digest;
    let hash = sha2::Sha256::digest(format!("{trigger_id}:{sequence}").as_bytes());
    format!("event-trigger-{}", hex::encode(&hash[..16]))
}

#[cfg(windows)]
fn watch_workspace_directory(
    root: std::path::PathBuf,
    sender: tokio::sync::mpsc::Sender<WorkspaceWatchMessage>,
) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, ReadDirectoryChangesW, FILE_ACTION_ADDED, FILE_ACTION_MODIFIED,
            FILE_ACTION_REMOVED, FILE_ACTION_RENAMED_NEW_NAME, FILE_ACTION_RENAMED_OLD_NAME,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_NOTIFY_CHANGE_DIR_NAME,
            FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
    };
    let wide_path: Vec<u16> = root
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let directory = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            FILE_LIST_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if directory == INVALID_HANDLE_VALUE || directory.is_null() {
        let _ = sender.blocking_send(WorkspaceWatchMessage::Closed(root));
        return;
    }
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let mut bytes_returned = 0_u32;
        let success = unsafe {
            ReadDirectoryChangesW(
                directory,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                1,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_LAST_WRITE
                    | FILE_NOTIFY_CHANGE_SIZE,
                &mut bytes_returned,
                std::ptr::null_mut(),
                None,
            )
        };
        if success == 0 {
            break;
        }
        if bytes_returned == 0 {
            if sender
                .blocking_send(WorkspaceWatchMessage::Changed {
                    root: root.clone(),
                    path: root.clone(),
                    change_kind: "overflow",
                })
                .is_err()
            {
                break;
            }
            continue;
        }
        let parsed = parse_directory_change_buffer(&buffer[..bytes_returned as usize]);
        let Ok(changes) = parsed else {
            if sender
                .blocking_send(WorkspaceWatchMessage::Changed {
                    root: root.clone(),
                    path: root.clone(),
                    change_kind: "overflow",
                })
                .is_err()
            {
                break;
            }
            continue;
        };
        for (relative_path, change_kind) in changes {
            if relative_path.is_absolute()
                || relative_path.components().any(|part| {
                    matches!(
                        part,
                        std::path::Component::ParentDir
                            | std::path::Component::RootDir
                            | std::path::Component::Prefix(_)
                    )
                })
            {
                continue;
            }
            if sender
                .blocking_send(WorkspaceWatchMessage::Changed {
                    root: root.clone(),
                    path: root.join(relative_path),
                    change_kind,
                })
                .is_err()
            {
                unsafe { CloseHandle(directory) };
                let _ = sender.blocking_send(WorkspaceWatchMessage::Closed(root));
                return;
            }
        }
    }
    unsafe { CloseHandle(directory) };
    let _ = sender.blocking_send(WorkspaceWatchMessage::Closed(root));
}

#[cfg(windows)]
fn parse_directory_change_buffer(
    buffer: &[u8],
) -> Result<Vec<(std::path::PathBuf, &'static str)>, ()> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ACTION_ADDED, FILE_ACTION_MODIFIED, FILE_ACTION_REMOVED, FILE_ACTION_RENAMED_NEW_NAME,
        FILE_ACTION_RENAMED_OLD_NAME,
    };
    let mut changes = Vec::new();
    let mut offset = 0_usize;
    loop {
        let header_end = offset.checked_add(12).ok_or(())?;
        if header_end > buffer.len() {
            return Err(());
        }
        let next =
            u32::from_le_bytes(buffer[offset..offset + 4].try_into().map_err(|_| ())?) as usize;
        let action = u32::from_le_bytes(buffer[offset + 4..offset + 8].try_into().map_err(|_| ())?);
        let name_bytes =
            u32::from_le_bytes(buffer[offset + 8..offset + 12].try_into().map_err(|_| ())?)
                as usize;
        let name_end = header_end.checked_add(name_bytes).ok_or(())?;
        if name_bytes == 0 || name_bytes % 2 != 0 || name_end > buffer.len() {
            return Err(());
        }
        let units: Vec<u16> = buffer[header_end..name_end]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let path = std::path::PathBuf::from(String::from_utf16_lossy(&units));
        let change_kind = match action {
            FILE_ACTION_ADDED => "added",
            FILE_ACTION_REMOVED => "removed",
            FILE_ACTION_MODIFIED => "modified",
            FILE_ACTION_RENAMED_OLD_NAME => "renamed_from",
            FILE_ACTION_RENAMED_NEW_NAME => "renamed_to",
            _ => return Err(()),
        };
        changes.push((path, change_kind));
        if next == 0 {
            return Ok(changes);
        }
        if next < 12 || next % 4 != 0 {
            return Err(());
        }
        offset = offset.checked_add(next).ok_or(())?;
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_file_name_and_change_kind_from_bounded_windows_record() {
        let name: Vec<u16> = "src/main.rs".encode_utf16().collect();
        let mut record = Vec::new();
        record.extend_from_slice(&0_u32.to_le_bytes());
        record.extend_from_slice(
            &windows_sys::Win32::Storage::FileSystem::FILE_ACTION_MODIFIED.to_le_bytes(),
        );
        record.extend_from_slice(&((name.len() * 2) as u32).to_le_bytes());
        for unit in name {
            record.extend_from_slice(&unit.to_le_bytes());
        }
        let changes = parse_directory_change_buffer(&record).expect("record parses");
        assert_eq!(
            changes,
            vec![(std::path::PathBuf::from("src/main.rs"), "modified")]
        );
    }

    #[test]
    fn rejects_truncated_or_odd_utf16_records() {
        assert!(parse_directory_change_buffer(&[0; 11]).is_err());
        let mut odd_name = vec![0_u8; 13];
        odd_name[4..8].copy_from_slice(
            &windows_sys::Win32::Storage::FileSystem::FILE_ACTION_ADDED.to_le_bytes(),
        );
        odd_name[8..12].copy_from_slice(&1_u32.to_le_bytes());
        assert!(parse_directory_change_buffer(&odd_name).is_err());
    }

    #[tokio::test]
    async fn directory_watcher_emits_bounded_file_change_events_and_stops() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        let root = workspace.path().canonicalize().expect("workspace path");
        let (sender, mut receiver) = tokio::sync::mpsc::channel(16);
        let watcher = WorkspaceWatcher::start(root.clone(), sender.clone())
            .await
            .expect("watcher starts");
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let file_path = root.join("trigger-test.txt");
        std::fs::File::create(&file_path)
            .expect("file creates")
            .write_all(b"event")
            .expect("file writes");
        let event = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Some(WorkspaceWatchMessage::Changed { path, .. }) = receiver.recv().await {
                    break path;
                }
            }
        })
        .await
        .expect("filesystem event arrives");
        assert_eq!(event, file_path);
        watcher.stop();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if matches!(
                    receiver.recv().await,
                    Some(WorkspaceWatchMessage::Closed(_))
                ) {
                    break;
                }
            }
        })
        .await
        .expect("watcher stops");
    }
}
