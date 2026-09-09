use super::*;

pub(super) fn ambient_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join(format!("{name}.db"))).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal, coordinator)
        .with_ambient_data_dir(directory.path().to_path_buf());
    (bridge, directory)
}

pub(super) async fn ambient_call(
    bridge: &IpcBridge,
    command: generated::command_envelope::Command,
) -> (String, serde_json::Value) {
    let (mut client, server) = duplex(256 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let envelope = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: "ambient-request".into(),
        client_id: "ambient-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(command),
    };
    transport::write_frame(&mut client, &envelope.encode_to_vec())
        .await
        .expect("request writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("request serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("response reads");
    let event = generated::EventEnvelope::decode(response.as_slice()).expect("event decodes");
    let payload = serde_json::from_slice(&event.payload).unwrap_or(serde_json::Value::Null);
    (event.event_type, payload)
}

async fn typed_checkpoint_call(
    bridge: &IpcBridge,
    command: generated::command_envelope::Command,
) -> generated::EventEnvelope {
    let (mut client, server) = duplex(256 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let envelope = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: uuid::Uuid::now_v7().to_string(),
        client_id: "checkpoint-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(command),
    };
    transport::write_frame(&mut client, &envelope.encode_to_vec())
        .await
        .expect("typed checkpoint request writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("typed checkpoint request serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("typed checkpoint response reads");
    generated::EventEnvelope::decode(response.as_slice()).expect("typed checkpoint decodes")
}

async fn typed_goal_call(
    bridge: &IpcBridge,
    request_id: &str,
    command: generated::command_envelope::Command,
) -> generated::EventEnvelope {
    let (mut client, server) = duplex(256 * 1024);
    let (mut server_reader, mut server_writer) = tokio::io::split(server);
    let envelope = generated::CommandEnvelope {
        protocol: Some(protocol()),
        request_id: request_id.into(),
        client_id: "goal-client".into(),
        core_instance_id: String::new(),
        session_epoch: 1,
        command: Some(command),
    };
    transport::write_frame(&mut client, &envelope.encode_to_vec())
        .await
        .expect("typed goal request writes");
    bridge
        .process_once(&mut server_reader, &mut server_writer)
        .await
        .expect("typed goal request serves");
    let response = transport::read_frame(&mut client)
        .await
        .expect("typed goal response reads");
    generated::EventEnvelope::decode(response.as_slice()).expect("typed goal decodes")
}

#[tokio::test]
async fn persistent_goal_ipc_is_typed_bounded_and_recoverable() {
    let path = std::env::temp_dir().join(format!("evohime-ipc-goal-{}.db", uuid::Uuid::new_v4()));
    let _ = std::fs::remove_file(&path);
    let journal = EventJournal::open(&path).expect("journal opens");
    let bridge = IpcBridge::new(journal);
    let workspace = std::env::temp_dir().join("evohime-goal-workspace");
    std::fs::create_dir_all(&workspace).expect("goal workspace creates");
    let create = generated::CreateGoal {
        goal_id: "goal-ipc-1".into(),
        workspace_path: workspace.to_string_lossy().into_owned(),
        chat_id: "chat-1".into(),
        objective: "Проверить typed Goal".into(),
        success_criteria: vec![generated::GoalCriterionInput {
            id: "criterion-1".into(),
            kind: "manual".into(),
            statement: "Core evidence сохранено".into(),
        }],
        idempotency_key: "goal-create-1".into(),
        ..Default::default()
    };
    let created = typed_goal_call(
        &bridge,
        "goal-create-request",
        generated::command_envelope::Command::CreateGoal(create.clone()),
    )
    .await;
    let created = match created.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed GoalAction, got {other:?}"),
    };
    assert!(
        created.applied,
        "create error={} message={}",
        created.error_code, created.error_message
    );
    assert_eq!(created.goal_version, 1);
    let projection = created.goal.expect("create carries projection");
    assert_eq!(projection.status, "active");
    assert_eq!(projection.remaining_criteria, vec!["criterion-1"]);
    assert!(!projection.workspace_id.contains("evohime-goal-workspace"));

    let replay = typed_goal_call(
        &bridge,
        "goal-create-request",
        generated::command_envelope::Command::CreateGoal(create),
    )
    .await;
    let replay = match replay.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed replay GoalAction, got {other:?}"),
    };
    assert!(replay.deduplicated);
    assert_eq!(replay.goal_version, 1);

    let listed = typed_goal_call(
        &bridge,
        "goal-list-request",
        generated::command_envelope::Command::ListGoals(generated::ListGoals {
            workspace_path: workspace.to_string_lossy().into_owned(),
            limit: 16,
        }),
    )
    .await;
    let listed = match listed.event {
        Some(generated::event_envelope::Event::GoalList(result)) => result,
        other => panic!("expected typed GoalList, got {other:?}"),
    };
    assert_eq!(listed.goals.len(), 1);
    assert_eq!(listed.goals[0].objective, "Проверить typed Goal");

    let fetched = typed_goal_call(
        &bridge,
        "goal-get-request",
        generated::command_envelope::Command::GetGoal(generated::GetGoal {
            goal_id: "goal-ipc-1".into(),
        }),
    )
    .await;
    let fetched = match fetched.event {
        Some(generated::event_envelope::Event::Goal(goal)) => goal,
        other => panic!("expected typed Goal projection, got {other:?}"),
    };
    assert_eq!(fetched.objective, "Проверить typed Goal");

    let updated = typed_goal_call(
        &bridge,
        "goal-update-request",
        generated::command_envelope::Command::UpdateGoal(generated::UpdateGoal {
            goal_id: "goal-ipc-1".into(),
            expected_version: 1,
            objective: "Проверить typed Goal и историю".into(),
            idempotency_key: "goal-update-key".into(),
            ..Default::default()
        }),
    )
    .await;
    let updated = match updated.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed update result, got {other:?}"),
    };
    assert!(updated.applied);
    assert_eq!(updated.goal_version, 2);

    let paused = typed_goal_call(
        &bridge,
        "goal-pause-request",
        generated::command_envelope::Command::PauseGoal(generated::GoalAction {
            goal_id: "goal-ipc-1".into(),
            expected_version: 2,
            idempotency_key: "goal-pause-key".into(),
        }),
    )
    .await;
    let paused = match paused.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed pause result, got {other:?}"),
    };
    assert_eq!(
        paused.goal.as_ref().map(|goal| goal.status.as_str()),
        Some("paused")
    );

    let resumed = typed_goal_call(
        &bridge,
        "goal-resume-request",
        generated::command_envelope::Command::ResumeGoal(generated::GoalAction {
            goal_id: "goal-ipc-1".into(),
            expected_version: 3,
            idempotency_key: "goal-resume-key".into(),
        }),
    )
    .await;
    let resumed = match resumed.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed resume result, got {other:?}"),
    };
    assert_eq!(
        resumed.goal.as_ref().map(|goal| goal.status.as_str()),
        Some("active")
    );

    let checkpoint = crate::task_checkpoint::TaskCheckpointRuntime::new(bridge.journal.clone())
        .capture(
            "goal-checkpoint-task",
            &workspace,
            crate::task_checkpoint::CheckpointStatus::Blocked,
            crate::task_checkpoint::CheckpointCaptureReason::RecoveryBlocked,
            None,
        )
        .await
        .expect("goal checkpoint persists");
    let linked = typed_goal_call(
        &bridge,
        "goal-link-checkpoint-request",
        generated::command_envelope::Command::LinkGoalReference(generated::LinkGoalReference {
            goal_id: "goal-ipc-1".into(),
            expected_version: 4,
            kind: "checkpoint".into(),
            reference_id: checkpoint.id,
            idempotency_key: "goal-link-checkpoint-key".into(),
        }),
    )
    .await;
    let linked = match linked.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed checkpoint link result, got {other:?}"),
    };
    assert!(linked.applied);
    assert_eq!(linked.goal_version, 5);

    let missing_link = typed_goal_call(
        &bridge,
        "goal-link-missing-request",
        generated::command_envelope::Command::LinkGoalReference(generated::LinkGoalReference {
            goal_id: "goal-ipc-1".into(),
            expected_version: 5,
            kind: "workflow".into(),
            reference_id: "missing-workflow".into(),
            idempotency_key: "goal-link-missing-key".into(),
        }),
    )
    .await;
    let missing_link = match missing_link.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed link result, got {other:?}"),
    };
    assert_eq!(missing_link.error_code, "reference_not_found");

    let stale = typed_goal_call(
        &bridge,
        "goal-pause-stale",
        generated::command_envelope::Command::PauseGoal(generated::GoalAction {
            goal_id: "goal-ipc-1".into(),
            expected_version: 99,
            idempotency_key: "goal-pause-stale-key".into(),
        }),
    )
    .await;
    let stale = match stale.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed stale GoalAction, got {other:?}"),
    };
    assert_eq!(stale.error_code, "stale_version");

    let verified = typed_goal_call(
        &bridge,
        "goal-verify-request",
        generated::command_envelope::Command::VerifyGoalCriterion(generated::VerifyGoalCriterion {
            goal_id: "goal-ipc-1".into(),
            expected_version: 5,
            criterion_id: "criterion-1".into(),
            idempotency_key: "goal-verify-key".into(),
        }),
    )
    .await;
    let verified = match verified.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed verified GoalAction, got {other:?}"),
    };
    assert!(verified.applied);
    assert_eq!(
        verified.goal.as_ref().map(|goal| goal.status.as_str()),
        Some("completed")
    );
    let verified_criterion = &verified
        .goal
        .as_ref()
        .expect("verified projection")
        .success_criteria[0];
    assert_eq!(verified_criterion.provenance, "core");
    assert!(verified_criterion
        .evidence_ref
        .starts_with("core:user-decision:"));

    let cancelled = typed_goal_call(
        &bridge,
        "goal-cancel-completed",
        generated::command_envelope::Command::CancelGoal(generated::GoalAction {
            goal_id: "goal-ipc-1".into(),
            expected_version: 6,
            idempotency_key: "goal-cancel-completed-key".into(),
        }),
    )
    .await;
    let cancelled = match cancelled.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected typed cancel result, got {other:?}"),
    };
    assert_eq!(cancelled.error_code, "invalid_state_transition");

    let cancel_target = typed_goal_call(
        &bridge,
        "goal-create-cancel-target",
        generated::command_envelope::Command::CreateGoal(generated::CreateGoal {
            goal_id: "goal-ipc-cancel-target".into(),
            workspace_path: workspace.to_string_lossy().into_owned(),
            objective: "Отменяемая цель".into(),
            success_criteria: vec![generated::GoalCriterionInput {
                id: "criterion-1".into(),
                kind: "manual".into(),
                statement: "Не требуется подтверждение".into(),
            }],
            idempotency_key: "goal-create-cancel-target-key".into(),
            ..Default::default()
        }),
    )
    .await;
    let cancel_target = match cancel_target.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected cancel target creation, got {other:?}"),
    };
    assert!(cancel_target.applied);
    let cancelled = typed_goal_call(
        &bridge,
        "goal-cancel-active",
        generated::command_envelope::Command::CancelGoal(generated::GoalAction {
            goal_id: "goal-ipc-cancel-target".into(),
            expected_version: 1,
            idempotency_key: "goal-cancel-active-key".into(),
        }),
    )
    .await;
    let cancelled = match cancelled.event {
        Some(generated::event_envelope::Event::GoalAction(result)) => result,
        other => panic!("expected successful cancel result, got {other:?}"),
    };
    assert_eq!(
        cancelled.goal.as_ref().map(|goal| goal.status.as_str()),
        Some("cancelled")
    );
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&workspace);
}

#[tokio::test]
async fn task_checkpoint_ipc_is_typed_bounded_and_idempotent() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("checkpoint-ipc.db")).expect("journal opens");
    let runtime = crate::task_checkpoint::TaskCheckpointRuntime::new(journal.clone());
    let checkpoint = runtime
        .capture(
            "task-1",
            directory.path(),
            crate::task_checkpoint::CheckpointStatus::Blocked,
            crate::task_checkpoint::CheckpointCaptureReason::RecoveryBlocked,
            None,
        )
        .await
        .expect("checkpoint persists");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);

    let projection_event = typed_checkpoint_call(
        &bridge,
        generated::command_envelope::Command::GetTaskCheckpoint(generated::GetTaskCheckpoint {
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().into_owned(),
            max_replay_events: 64,
        }),
    )
    .await;
    assert!(projection_event.payload.is_empty());
    let Some(generated::event_envelope::Event::TaskCheckpoint(projection)) = projection_event.event
    else {
        panic!("expected typed checkpoint projection");
    };
    assert_eq!(projection.checkpoint_id, checkpoint.id);
    assert_eq!(projection.recovery_disposition, "blocked");
    assert!(projection
        .refs
        .iter()
        .all(|reference| reference.content_hash.len() <= 128));

    let action = generated::ResolveTaskCheckpoint {
        task_id: "task-1".into(),
        workspace_path: directory.path().to_string_lossy().into_owned(),
        checkpoint_id: checkpoint.id.clone(),
        expected_source_event_seq: checkpoint.source_event_seq,
        action: "acknowledge_recovery".into(),
        idempotency_key: "ack-1".into(),
    };
    let first_action = typed_checkpoint_call(
        &bridge,
        generated::command_envelope::Command::ResolveTaskCheckpoint(action.clone()),
    )
    .await;
    let Some(generated::event_envelope::Event::TaskCheckpointActionResult(first_result)) =
        first_action.event
    else {
        panic!("expected typed checkpoint action result");
    };
    assert!(first_result.applied);
    assert!(!first_result.deduplicated);
    assert!(first_action.payload.is_empty());

    let repeated_action = typed_checkpoint_call(
        &bridge,
        generated::command_envelope::Command::ResolveTaskCheckpoint(action),
    )
    .await;
    let Some(generated::event_envelope::Event::TaskCheckpointActionResult(repeated_result)) =
        repeated_action.event
    else {
        panic!("expected deduplicated checkpoint action result");
    };
    assert!(repeated_result.applied);
    assert!(repeated_result.deduplicated);
    let action_events = journal
        .task_history("task-1", 32)
        .await
        .expect("checkpoint history reads")
        .into_iter()
        .filter(|event| event.event_type == "task.checkpoint.action")
        .count();
    assert_eq!(action_events, 1);
}

#[tokio::test]
async fn agent_skills_ipc_is_typed_metadata_first_and_non_durable() {
    let directory = tempfile::tempdir().expect("temp dir");
    let skill_dir = directory.path().join(".agents/skills/reviewer");
    std::fs::create_dir_all(skill_dir.join("references")).expect("skill dir creates");
    std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: reviewer\ndescription: bounded review\nversion: 1.0.0\n---\nsecretly never persisted\n",
        )
        .expect("skill writes");
    std::fs::write(skill_dir.join("references/guide.md"), "bounded guide")
        .expect("reference writes");
    let journal =
        EventJournal::open(directory.path().join("skills-ipc.db")).expect("journal opens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator);
    let workspace = directory.path().to_string_lossy().into_owned();

    let catalog_event = typed_checkpoint_call(
        &bridge,
        generated::command_envelope::Command::ListSkills(generated::ListSkills {
            workspace_path: workspace.clone(),
            limit: 10,
        }),
    )
    .await;
    assert!(catalog_event.payload.is_empty());
    let Some(generated::event_envelope::Event::SkillCatalog(catalog)) = catalog_event.event else {
        panic!("expected typed skill catalog");
    };
    assert_eq!(catalog.skills.len(), 1);
    assert_eq!(catalog.skills[0].skill_id, "reviewer");
    assert!(catalog.skills[0].content_hash.len() <= 128);

    let content_event = typed_checkpoint_call(
        &bridge,
        generated::command_envelope::Command::LoadSkill(generated::LoadSkill {
            workspace_path: workspace,
            skill_id: "reviewer".into(),
            max_bytes: 4096,
        }),
    )
    .await;
    let Some(generated::event_envelope::Event::SkillContent(content)) = content_event.event else {
        panic!("expected typed skill content");
    };
    assert_eq!(content.error_code, "");
    assert!(content.content.contains("secretly never persisted"));
    assert!(content_event.payload.is_empty());
    let history = journal
        .task_history("skill:reviewer", 16)
        .await
        .expect("skill trace reads");
    assert_eq!(history.len(), 1);
    assert!(!String::from_utf8_lossy(&history[0].payload).contains("secretly never persisted"));
}

#[cfg(target_os = "windows")]
#[tokio::test]
async fn a_voice_command_card_appears_and_is_declined_without_launching_anything() {
    let (bridge, _directory) = ambient_bridge("ambient-voice");
    let policy = evohime_listener_contract::AmbientPolicy::default();
    let now_ms = crate::task_memory::now_millis();
    let decision = crate::voice_command::decide(
        &bridge.voice_commands(),
        &policy,
        "Ева, открой блокнот",
        now_ms,
        "voice-1".to_owned(),
    );
    let crate::voice_command::Decision::Confirm(command) = decision else {
        panic!("услышанное обязано ждать клика");
    };
    assert_eq!(command.app_id, "notepad");

    let (event_type, listed) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListVoiceCommands(generated::ListVoiceCommands {
            limit: 10,
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.voice_commands");
    assert_eq!(listed["requires_confirmation"], true);
    assert_eq!(listed["commands"][0]["command_id"], "voice-1");
    assert_eq!(listed["commands"][0]["title"], "Блокнот");

    let (event_type, declined) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ResolveVoiceCommand(generated::ResolveVoiceCommand {
            command_id: "voice-1".into(),
            accepted: false,
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.voice_command_resolved");
    assert_eq!(declined["launched"], false);
    assert_eq!(declined["state"], "declined");

    // Второй клик по решённой карточке ничего не запускает: её больше нет.
    let (_, again) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ResolveVoiceCommand(generated::ResolveVoiceCommand {
            command_id: "voice-1".into(),
            accepted: true,
        }),
    )
    .await;
    assert_eq!(again["launched"], false);
    assert_eq!(again["error_code"], "not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn saving_a_policy_without_the_voice_fields_keeps_the_stored_value() {
    let (bridge, _directory) = ambient_bridge("ambient-voice-policy");
    let _control = attach_fake_listener(&bridge);
    let (_, saved) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
            policy: Some(generated::AmbientPolicy {
                quiet_hours: Vec::new(),
                blocklist_patterns: Vec::new(),
                retention_days: 7,
                window_title_blocklist: Vec::new(),
                voice_commands: Some(false),
                voice_commands_autorun: None,
            }),
        }),
    )
    .await;
    assert_eq!(saved["applied"], true);
    let (_, policy) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
    )
    .await;
    assert_eq!(policy["voice_commands"], false);
    assert_eq!(policy["voice_commands_autorun"], false);

    // Старый клиент не шлёт новых полей — и не выключает их своим молчанием.
    let (_, saved) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
            policy: Some(generated::AmbientPolicy {
                quiet_hours: Vec::new(),
                blocklist_patterns: vec!["zoom*.exe".into()],
                retention_days: 7,
                window_title_blocklist: Vec::new(),
                voice_commands: None,
                voice_commands_autorun: None,
            }),
        }),
    )
    .await;
    assert_eq!(saved["applied"], true);
    let (_, policy) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
    )
    .await;
    assert_eq!(policy["voice_commands"], false);
}

/// Подключает фиктивный листенер: команда уезжает в канал и остаётся там.
fn attach_fake_listener(
    bridge: &IpcBridge,
) -> tokio::sync::mpsc::Receiver<crate::ambient::ListenerControl> {
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let registry = bridge.ambient();
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(registry.attach_control(tx))
    });
    rx
}

/// Без листенера включение не притворяется успехом: намерение сохранено,
/// но состояние честно называется недоступным.
#[tokio::test]
async fn enabling_without_a_listener_reports_that_the_listener_is_missing() {
    let (bridge, directory) = ambient_bridge("ambient-no-listener");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SetAmbientListening(generated::SetAmbientListening {
            enabled: true,
            paused: false,
            device_id: String::new(),
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.listening");
    assert_eq!(payload["error_code"], "LISTENER_UNAVAILABLE");
    assert_eq!(payload["state"], "engine_unavailable");
    // Намерение всё равно сохранено: следующее подключение листенера его
    // применит, а не начнёт с выключенного микрофона.
    assert!(crate::ambient::load_control(directory.path()).enabled);
}

/// Движок не готов — включение отвечает `ENGINE_NOT_READY`, а не молчит.
#[tokio::test(flavor = "multi_thread")]
async fn enabling_without_an_engine_reports_engine_not_ready() {
    let (bridge, _directory) = ambient_bridge("ambient-engine");
    let mut control = attach_fake_listener(&bridge);
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SetAmbientListening(generated::SetAmbientListening {
            enabled: true,
            paused: false,
            device_id: String::new(),
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "ENGINE_NOT_READY");
    assert_eq!(payload["state"], "starting");
    assert!(matches!(
        control.try_recv(),
        Ok(crate::ambient::ListenerControl::Policy(_))
    ));
}

/// Занятое устройство называется своим кодом и не превращается в
/// «запускаюсь».
#[tokio::test(flavor = "multi_thread")]
async fn a_busy_device_reports_a_conflict() {
    let (bridge, _directory) = ambient_bridge("ambient-conflict");
    let _control = attach_fake_listener(&bridge);
    bridge
        .ambient()
        .set_state(
            ListeningState::DeviceConflict,
            ListeningReason::DeviceConflict,
            None,
        )
        .await;
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SetAmbientListening(generated::SetAmbientListening {
            enabled: true,
            paused: false,
            device_id: String::new(),
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "DEVICE_CONFLICT");
    assert_eq!(payload["state"], "device_conflict");
}

/// Неизвестное устройство не выбирается: подмена на умолчание означала бы
/// слушать не тем микрофоном, который выбрал пользователь.
#[tokio::test(flavor = "multi_thread")]
async fn selecting_a_missing_device_is_refused() {
    let (bridge, _directory) = ambient_bridge("ambient-device");
    let _control = attach_fake_listener(&bridge);
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SetAmbientListening(generated::SetAmbientListening {
            enabled: true,
            paused: false,
            device_id: "mic-that-left".into(),
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "DEVICE_DISCONNECTED");
}

/// Фраза в поле идентификатора устройства — это попытка протащить текст
/// через метаданные, и она отбивается контрактом 04.1.
#[tokio::test]
async fn a_phrase_in_a_device_id_is_refused() {
    let (bridge, _directory) = ambient_bridge("ambient-device-id");
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SetAmbientListening(generated::SetAmbientListening {
            enabled: true,
            paused: false,
            device_id: "позвони маме завтра".into(),
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "INVALID_ARGUMENT");
}

/// Снимок статуса отвечает всегда: панель открывается, не дожидаясь
/// события.
#[tokio::test]
async fn status_answers_before_any_event_arrives() {
    let (bridge, _directory) = ambient_bridge("ambient-status");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetAmbientStatus(generated::GetAmbientStatus {}),
    )
    .await;
    assert_eq!(event_type, "ambient.status");
    assert_eq!(payload["state"], "engine_unavailable");
    assert_eq!(payload["engine_ready"], false);
    assert!(payload["devices"].as_array().expect("devices").is_empty());
}

/// Список эпизодов не несёт текста; текст отдаётся только явным запросом
/// одного эпизода.
#[tokio::test]
async fn text_is_absent_from_the_listing_and_present_only_on_demand() {
    let (bridge, _directory) = ambient_bridge("ambient-episodes");
    let journal = bridge.journal();
    journal
        .open_ambient_episode(
            "ep-1",
            "whisper-base-q5_1",
            "whisper-base-q5_1",
            evohime_listener_contract::ExtractionState::Disabled,
            1_700_000_000_000,
        )
        .await
        .expect("episode opens");
    journal
        .insert_ambient_utterance(
            &crate::ambient::AmbientUtteranceInput {
                utterance_id: "ep-1-0".into(),
                episode_id: "ep-1".into(),
                sequence: 0,
                started_at_ms: 1_700_000_000_000,
                duration_ms: 1_200,
                text: "надо купить хлеб".into(),
                language: "ru".into(),
                avg_logprob: -0.2,
                redacted: false,
            },
            7,
            2_000,
        )
        .await
        .expect("utterance stored");

    let (event_type, listing) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListAmbientEpisodes(generated::ListAmbientEpisodes {
            since_ms: 0,
            limit: 10,
            cursor: String::new(),
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.episodes");
    let serialized = listing.to_string();
    assert!(
        !serialized.contains("надо купить хлеб"),
        "listing leaked transcript text"
    );
    assert_eq!(listing["episodes"][0]["episode_id"], "ep-1");
    assert_eq!(listing["episodes"][0]["utterance_count"], 1);

    let (event_type, detail) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetAmbientEpisode(generated::GetAmbientEpisode {
            episode_id: "ep-1".into(),
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.episode");
    assert_eq!(detail["utterances"][0]["text"], "надо купить хлеб");
}

/// Неподтверждённое удаление отвергается ядром, а не только модальным
/// окном оболочки: обход UI не даёт больше прав.
#[tokio::test]
async fn deleting_without_confirmation_is_refused_by_core() {
    let (bridge, _directory) = ambient_bridge("ambient-delete");
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::DeleteAmbientTranscripts(
            generated::DeleteAmbientTranscripts {
                episode_ids: vec!["ep-1".into()],
                all: false,
                confirmed: false,
            },
        ),
    )
    .await;
    assert_eq!(payload["error_code"], "CONFIRMATION_REQUIRED");
    assert_eq!(payload["deleted_count"], 0);

    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ForgetAmbientWindow(generated::ForgetAmbientWindow {
            window_ms: 5 * 60 * 1000,
            confirmed: false,
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "CONFIRMATION_REQUIRED");
}

/// Удаление действительно удаляет текст и вычищает ambient-строки
/// журнала: событие об эпизоде не переживает сам эпизод.
#[tokio::test]
async fn deleting_removes_the_text_and_its_journal_rows() {
    let (bridge, _directory) = ambient_bridge("ambient-delete-real");
    let journal = bridge.journal();
    journal
        .open_ambient_episode(
            "ep-2",
            "whisper-base-q5_1",
            "whisper-base-q5_1",
            evohime_listener_contract::ExtractionState::Disabled,
            1_700_000_000_000,
        )
        .await
        .expect("episode opens");
    journal
        .insert_ambient_utterance(
            &crate::ambient::AmbientUtteranceInput {
                utterance_id: "ep-2-0".into(),
                episode_id: "ep-2".into(),
                sequence: 0,
                started_at_ms: 1_700_000_000_000,
                duration_ms: 900,
                text: "это надо забыть".into(),
                language: "ru".into(),
                avg_logprob: -0.1,
                redacted: false,
            },
            7,
            2_000,
        )
        .await
        .expect("utterance stored");
    bridge
        .publish_ambient(&evohime_listener_contract::AmbientLogEvent::Transcript {
            episode_id: evohime_listener_contract::EpisodeId::new("ep-2").unwrap(),
            started_at_ms: 1_700_000_000_000,
            utterance_count: 1,
            extraction_state: evohime_listener_contract::ExtractionState::Disabled,
        })
        .await
        .expect("transcript event published");

    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::DeleteAmbientTranscripts(
            generated::DeleteAmbientTranscripts {
                episode_ids: vec!["ep-2".into()],
                all: false,
                confirmed: true,
            },
        ),
    )
    .await;
    assert_eq!(payload["deleted_count"], 1);
    assert!(journal
        .list_ambient_utterances("ep-2", 10)
        .await
        .expect("utterances read")
        .is_empty());
    let replay = journal
        .replay_bounded(0, 256)
        .await
        .expect("journal replays");
    assert!(
        !replay
            .events
            .iter()
            .any(|event| event.task_id == "ep-2" && event.event_type == "ambient.transcript"),
        "episode journal rows outlived the episode"
    );
}

/// Ни одно ambient-событие не несёт ни текста, ни его хеша.
#[tokio::test(flavor = "multi_thread")]
async fn ambient_events_never_carry_text_or_its_hash() {
    let (bridge, _directory) = ambient_bridge("ambient-events");
    let _control = attach_fake_listener(&bridge);
    let _ = ambient_call(
        &bridge,
        generated::command_envelope::Command::SetAmbientListening(generated::SetAmbientListening {
            enabled: true,
            paused: true,
            device_id: String::new(),
        }),
    )
    .await;
    let replay = bridge
        .journal()
        .replay_bounded(0, 256)
        .await
        .expect("journal replays");
    let ambient_rows: Vec<_> = replay
        .events
        .iter()
        .filter(|event| event.event_type.starts_with("ambient."))
        .collect();
    assert!(!ambient_rows.is_empty(), "no ambient event was published");
    for event in ambient_rows {
        let payload: serde_json::Value =
            serde_json::from_slice(&event.payload).expect("ambient payload is json");
        let object = payload.as_object().expect("ambient payload is an object");
        for forbidden in ["text", "text_hash", "transcript", "utterance"] {
            assert!(
                !object.contains_key(forbidden),
                "{} leaked {forbidden}",
                event.event_type
            );
        }
    }
}

/// Политика применяется целиком или не применяется вовсе.
#[tokio::test(flavor = "multi_thread")]
async fn an_invalid_policy_is_refused_whole() {
    let (bridge, directory) = ambient_bridge("ambient-policy");
    let mut control = attach_fake_listener(&bridge);

    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
            policy: Some(generated::AmbientPolicy {
                quiet_hours: vec![generated::QuietHours {
                    start_minute: 23 * 60,
                    end_minute: 7 * 60,
                }],
                blocklist_patterns: vec!["zoom*.exe".into()],
                retention_days: 14,
                window_title_blocklist: vec!["*банк*".into()],
                voice_commands: None,
                voice_commands_autorun: None,
            }),
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.policy_saved");
    assert_eq!(payload["applied"], true);
    assert!(matches!(
        control.try_recv(),
        Ok(crate::ambient::ListenerControl::Policy(_))
    ));

    let (_, refused) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
            policy: Some(generated::AmbientPolicy {
                quiet_hours: Vec::new(),
                blocklist_patterns: vec!["^bank.*$".into()],
                retention_days: 14,
                window_title_blocklist: Vec::new(),
                voice_commands: None,
                voice_commands_autorun: None,
            }),
        }),
    )
    .await;
    assert_eq!(refused["applied"], false);
    assert_eq!(refused["error_code"], "INVALID_ARGUMENT");

    let (_, over_retention) = ambient_call(
        &bridge,
        generated::command_envelope::Command::SaveAmbientPolicy(generated::SaveAmbientPolicy {
            policy: Some(generated::AmbientPolicy {
                quiet_hours: Vec::new(),
                blocklist_patterns: Vec::new(),
                retention_days: 365,
                window_title_blocklist: Vec::new(),
                voice_commands: None,
                voice_commands_autorun: None,
            }),
        }),
    )
    .await;
    assert_eq!(over_retention["error_code"], "POLICY_INVALID");

    // Отвергнутая политика не затёрла сохранённую.
    let stored = crate::ambient::load_policy(directory.path());
    assert_eq!(stored.retention_days, 14);
    assert_eq!(stored.process_blocklist, vec!["zoom*.exe".to_string()]);

    let (event_type, read_back) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetAmbientPolicy(generated::GetAmbientPolicy {}),
    )
    .await;
    assert_eq!(event_type, "ambient.policy");
    assert_eq!(read_back["retention_days"], 14);
    assert_eq!(read_back["quiet_hours"][0]["start_minute"], 23 * 60);
}

/// Кладёт готовое предложение в базу моста.
async fn seed_proposal(
    bridge: &IpcBridge,
    proposal_id: &str,
    kind: evohime_listener_contract::ProposalKind,
    subject: &str,
    episode_id: Option<&str>,
    now_ms: u64,
) {
    use crate::ambient_proactivity as proactivity;
    let subject_key = proactivity::subject_key(subject);
    let proposal_key = proactivity::proposal_key(kind, &subject_key, now_ms);
    let mute_key = proactivity::mute_key(kind, &subject_key);
    let record = crate::ambient::proposal_record(crate::ambient::ProposalRecordInput {
        proposal_id,
        proposal_key: &proposal_key,
        mute_key: &mute_key,
        kind,
        subject_key: &subject_key,
        subject,
        title: "Напомнить купить хлеб",
        source_episode_id: episode_id,
        now_ms,
    });
    bridge
        .journal()
        .record_ambient_proposal(&record)
        .await
        .expect("предложение записывается");
}

fn resolve_command(
    proposal_id: &str,
    accepted: bool,
    mute: bool,
    idempotency_key: &str,
) -> generated::command_envelope::Command {
    generated::command_envelope::Command::ResolveAmbientProposal(
        generated::ResolveAmbientProposal {
            proposal_id: proposal_id.into(),
            accepted,
            idempotency_key: idempotency_key.into(),
            mute,
        },
    )
}

/// Решения по несуществующему предложению не бывает: команда честно
/// отвечает «не применено», а не выдумывает успех. Пустой ключ
/// идемпотентности отвергается там же.
#[tokio::test]
async fn resolving_an_unknown_proposal_is_not_applied() {
    let (bridge, _directory) = ambient_bridge("ambient-proposal-unknown");
    let (event_type, payload) =
        ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
    assert_eq!(event_type, "ambient.proposal_resolved");
    assert_eq!(payload["applied"], false);
    assert_eq!(payload["error_code"], "INVALID_ARGUMENT");

    seed_proposal(
        &bridge,
        "prop-1",
        evohime_listener_contract::ProposalKind::Reminder,
        "хлеб",
        None,
        crate::task_memory::now_millis(),
    )
    .await;
    let (_, without_key) =
        ambient_call(&bridge, resolve_command("prop-1", true, false, "   ")).await;
    assert_eq!(
        without_key["applied"], false,
        "принятие без ключа идемпотентности не проходит"
    );
    assert_eq!(without_key["error_code"], "INVALID_ARGUMENT");
}

/// Повторный клик по карточке возвращает первое решение и не создаёт
/// вторую задачу.
#[tokio::test]
async fn a_repeated_resolve_with_the_same_key_creates_no_second_task() {
    let (bridge, _directory) = ambient_bridge("ambient-proposal-idempotent");
    seed_proposal(
        &bridge,
        "prop-1",
        evohime_listener_contract::ProposalKind::Suggestion,
        "отчёт",
        None,
        crate::task_memory::now_millis(),
    )
    .await;
    let (_, first) = ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
    assert_eq!(first["applied"], true);
    assert_eq!(first["state"], "accepted");
    let task_id = first["task_id"]
        .as_str()
        .expect("задача создана")
        .to_owned();
    assert!(!task_id.is_empty());

    let (_, second) = ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
    assert_eq!(second["applied"], true, "повтор отвечает первым решением");
    assert_eq!(second["task_id"], task_id);

    let tasks = bridge
        .journal()
        .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
        .await
        .expect("задачи читаются");
    assert_eq!(tasks.len(), 1, "двойной клик не породил вторую задачу");
    assert_eq!(tasks[0].status, "backlog", "принятое не запускается само");
}

/// Принятое напоминание — неисполняемая запись: это записано в данных, а
/// не подразумевается. Провенанс ведёт к эпизоду-источнику.
#[tokio::test]
async fn an_accepted_reminder_is_a_non_executable_row_with_provenance() {
    let (bridge, _directory) = ambient_bridge("ambient-proposal-reminder");
    let now_ms = crate::task_memory::now_millis();
    bridge
        .journal()
        .open_ambient_episode(
            "ep-1",
            "whisper-base-q5_1",
            "base-q5_1",
            evohime_listener_contract::ExtractionState::Done,
            now_ms,
        )
        .await
        .expect("эпизод открывается");
    seed_proposal(
        &bridge,
        "prop-1",
        evohime_listener_contract::ProposalKind::Reminder,
        "хлеб",
        Some("ep-1"),
        now_ms,
    )
    .await;
    let (_, payload) =
        ambient_call(&bridge, resolve_command("prop-1", true, false, "idem-1")).await;
    assert_eq!(payload["applied"], true);
    let tasks = bridge
        .journal()
        .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
        .await
        .expect("задачи читаются");
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].non_goals, AMBIENT_REMINDER_NON_GOAL);
    assert_eq!(tasks[0].source_ref.as_deref(), Some("ep-1"));
}

/// Отклонение задачу не создаёт, а mute переживает рестарт Core: он живёт
/// строкой таблицы, а не полем реестра в памяти процесса.
#[tokio::test]
async fn muting_a_subject_survives_a_core_restart() {
    let directory = tempfile::tempdir().expect("temp dir");
    let database = directory.path().join("ambient-proposal-mute.db");
    let now_ms = crate::task_memory::now_millis();
    {
        let journal = EventJournal::open(&database).expect("journal opens");
        let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
        let bridge = IpcBridge::with_coordinator(journal, coordinator)
            .with_ambient_data_dir(directory.path().to_path_buf());
        seed_proposal(
            &bridge,
            "prop-1",
            evohime_listener_contract::ProposalKind::Reminder,
            "хлеб",
            None,
            now_ms,
        )
        .await;
        let (_, payload) =
            ambient_call(&bridge, resolve_command("prop-1", false, true, "idem-1")).await;
        assert_eq!(payload["applied"], true);
        assert_eq!(payload["state"], "muted");
        assert_eq!(payload["task_id"], "", "заглушённое задач не создаёт");
        assert!(bridge
            .journal()
            .list_work_items(AMBIENT_PROPOSAL_PROJECT_ID)
            .await
            .expect("задачи читаются")
            .is_empty());
    }
    // Новый процесс: реестр пуст, единственный источник истины — база.
    let journal = EventJournal::open(&database).expect("journal reopens");
    let (coordinator, _events) = TaskCoordinator::new_with_journal(8, None, journal.clone());
    let bridge = IpcBridge::with_coordinator(journal.clone(), coordinator)
        .with_ambient_data_dir(directory.path().to_path_buf());
    let subject_key = crate::ambient_proactivity::subject_key("хлеб");
    let mute_key = crate::ambient_proactivity::mute_key(
        evohime_listener_contract::ProposalKind::Reminder,
        &subject_key,
    );
    assert!(
        bridge.proactivity().is_muted(&journal, &mute_key).await,
        "mute обязан пережить рестарт"
    );
    // И он глушит предложение из другой временной корзины — то есть с
    // другим `proposal_key`.
    let later_now_ms = now_ms + 5 * 60 * 60 * 1000;
    let later_key = crate::ambient_proactivity::proposal_key(
        evohime_listener_contract::ProposalKind::Reminder,
        &subject_key,
        later_now_ms,
    );
    let later = crate::ambient::proposal_record(crate::ambient::ProposalRecordInput {
        proposal_id: "prop-2",
        proposal_key: &later_key,
        mute_key: &mute_key,
        kind: evohime_listener_contract::ProposalKind::Reminder,
        subject_key: &subject_key,
        subject: "хлеб",
        title: "Напомнить купить хлеб",
        source_episode_id: None,
        now_ms: later_now_ms,
    });
    assert_eq!(
        journal.record_ambient_proposal(&later).await,
        Ok(evohime_local_storage::ambient_store::ProposalInsert::Muted)
    );
}

/// Список карточек — единственный путь для человекочитаемого текста, и он
/// не показывает просроченное как ждущее ответа.
#[tokio::test]
async fn the_proposal_list_carries_the_card_text_and_hides_expired_cards() {
    let (bridge, _directory) = ambient_bridge("ambient-proposal-list");
    let now_ms = crate::task_memory::now_millis();
    seed_proposal(
        &bridge,
        "prop-fresh",
        evohime_listener_contract::ProposalKind::Reminder,
        "хлеб",
        None,
        now_ms,
    )
    .await;
    seed_proposal(
        &bridge,
        "prop-stale",
        evohime_listener_contract::ProposalKind::Suggestion,
        "отчёт",
        None,
        now_ms - 2 * crate::ambient_proactivity::PROPOSAL_LIFETIME_MS,
    )
    .await;
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListAmbientProposals(
            generated::ListAmbientProposals { limit: 50 },
        ),
    )
    .await;
    assert_eq!(event_type, "ambient.proposals");
    let rows = payload["proposals"].as_array().expect("список карточек");
    assert_eq!(rows.len(), 1, "просроченная карточка снята со списка");
    assert_eq!(rows[0]["proposal_id"], "prop-fresh");
    assert_eq!(rows[0]["title"], "Напомнить купить хлеб");
    assert_eq!(payload["max_per_hour"], 3);
    assert_eq!(payload["max_per_day"], 10);
    assert_eq!(payload["min_interval_ms"], 600_000);
}

/// Ни при каких входных данных `ambient.proposal` в журнале не несёт ни
/// текста карточки, ни темы человеческими словами.
#[tokio::test]
async fn the_journalled_proposal_event_carries_no_card_text() {
    let (bridge, _directory) = ambient_bridge("ambient-proposal-privacy");
    let now_ms = crate::task_memory::now_millis();
    seed_proposal(
        &bridge,
        "prop-1",
        evohime_listener_contract::ProposalKind::Reminder,
        "секретный пароль от банка",
        None,
        now_ms,
    )
    .await;
    let (_, payload) =
        ambient_call(&bridge, resolve_command("prop-1", false, false, "idem-1")).await;
    assert_eq!(payload["applied"], true);
    assert_eq!(payload["state"], "declined");

    let journal = bridge.journal();
    let database = journal.database().lock().await;
    let events = database.read_events_after(0, 100).expect("журнал читается");
    let proposal_events: Vec<_> = events
        .into_iter()
        .filter(|event| event.event_type == "ambient.proposal")
        .collect();
    assert_eq!(proposal_events.len(), 1);
    for event in proposal_events {
        let body = String::from_utf8(event.payload).expect("payload is JSON");
        assert!(!body.contains("секретный"), "{body} несёт тему словами");
        assert!(
            !body.contains("Напомнить купить хлеб"),
            "{body} несёт текст карточки"
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("payload parses");
        for key in value.as_object().expect("object").keys() {
            assert!(
                !matches!(
                    key.as_str(),
                    "title" | "subject" | "canonical_subject" | "text"
                ),
                "ambient.proposal раскрывает {key}"
            );
        }
    }
}

/// «Забыть последние 5 минут» удаляет то, что попало в окно, и оставляет
/// то, что в него не попало.
#[tokio::test]
async fn forgetting_a_window_removes_only_that_window() {
    let (bridge, _directory) = ambient_bridge("ambient-forget");
    let journal = bridge.journal();
    let now_ms = crate::task_memory::now_millis();
    journal
        .open_ambient_episode(
            "ep-3",
            "whisper-base-q5_1",
            "whisper-base-q5_1",
            evohime_listener_contract::ExtractionState::Disabled,
            now_ms - 60 * 60 * 1000,
        )
        .await
        .expect("episode opens");
    for (sequence, offset_ms) in [(0i64, 60 * 60 * 1000u64), (1, 60 * 1000)] {
        journal
            .insert_ambient_utterance(
                &crate::ambient::AmbientUtteranceInput {
                    utterance_id: format!("ep-3-{sequence}"),
                    episode_id: "ep-3".into(),
                    sequence,
                    started_at_ms: now_ms - offset_ms,
                    duration_ms: 800,
                    text: format!("фраза {sequence}"),
                    language: "ru".into(),
                    avg_logprob: -0.1,
                    redacted: false,
                },
                7,
                2_000,
            )
            .await
            .expect("utterance stored");
    }

    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ForgetAmbientWindow(generated::ForgetAmbientWindow {
            window_ms: 5 * 60 * 1000,
            confirmed: true,
        }),
    )
    .await;
    assert_eq!(event_type, "ambient.forgotten");
    assert_eq!(payload["deleted_count"], 1);
    let left = journal
        .list_ambient_utterances("ep-3", 10)
        .await
        .expect("utterances read");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].sequence, 0);
}

// ------------------------------------------------------------------
// Workflow orchestration (план 06.3).
// ------------------------------------------------------------------
