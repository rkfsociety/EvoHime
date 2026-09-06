use super::ambient::{ambient_bridge, ambient_call};
use super::*;

fn workflow_bridge(name: &str) -> (IpcBridge, tempfile::TempDir) {
    ambient_bridge(name)
}

/// Каталог отдаёт версии, входы и пригодность к расписанию, но не граф
/// целиком: renderer не должен получать материал для собственного
/// планирования.
#[tokio::test]
async fn the_template_catalog_is_bounded_and_versioned() {
    let (bridge, _directory) = workflow_bridge("workflow-templates");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListWorkflowTemplates(
            generated::ListWorkflowTemplates {},
        ),
    )
    .await;
    assert_eq!(event_type, "workflow.templates");
    let templates = payload["templates"].as_array().expect("список шаблонов");
    assert_eq!(templates.len(), 3);
    let ids: Vec<&str> = templates
        .iter()
        .map(|item| item["template_id"].as_str().unwrap_or_default())
        .collect();
    assert!(ids.contains(&"repository-research"));
    assert!(ids.contains(&"plan-implement-review"));
    assert!(ids.contains(&"parallel-security-review"));
    for template in templates {
        assert!(template["version"].as_u64().unwrap_or_default() >= 1);
        assert!(!template["schedule_eligibility"]
            .as_str()
            .unwrap_or_default()
            .is_empty());
        assert!(template.get("graph").is_none(), "граф целиком не уходит");
    }
    let approval_bearing = templates
        .iter()
        .find(|item| item["template_id"] == "plan-implement-review")
        .expect("шаблон с подтверждением");
    assert_eq!(approval_bearing["schedule_eligibility"], "unavailable");
}

/// Неизвестный шаблон получает typed-код, а не пустой успешный ответ.
#[tokio::test]
async fn an_unknown_template_definition_is_named_not_faked() {
    let (bridge, _directory) = workflow_bridge("workflow-definition");
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowDefinition(
            generated::GetWorkflowDefinition {
                template_id: "does-not-exist".into(),
            },
        ),
    )
    .await;
    assert_eq!(payload["error_code"], "unknown_template");
    assert!(payload["nodes"].as_array().expect("узлы").is_empty());

    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowDefinition(
            generated::GetWorkflowDefinition {
                template_id: "parallel-security-review".into(),
            },
        ),
    )
    .await;
    assert_eq!(payload["error_code"], "");
    assert_eq!(payload["nodes"].as_array().expect("узлы").len(), 4);
    assert_eq!(payload["graph_hash"].as_str().unwrap_or_default().len(), 64);
}

/// Пропущенный обязательный вход не запускает граф.
#[tokio::test]
async fn a_template_input_contract_violation_never_starts_a_run() {
    let (bridge, directory) = workflow_bridge("workflow-start-invalid");
    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "repository-research".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![],
            idempotency_key: "key-1".into(),
        }),
    )
    .await;
    assert_eq!(event_type, "workflow.started");
    assert_eq!(payload["error_code"], "missing_input");
    assert_eq!(payload["run_id"], "");
    assert!(bridge
        .journal()
        .list_workflow_runs(10)
        .await
        .expect("список запусков")
        .is_empty());
}

/// Один и тот же ключ идемпотентности возвращает первый запуск.
#[tokio::test]
async fn the_same_idempotency_key_returns_the_first_run() {
    let (bridge, directory) = workflow_bridge("workflow-idempotency");
    let command = || {
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "parallel-security-review".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![generated::WorkflowInput {
                name: "scope".into(),
                value: "crates/evohime-core".into(),
            }],
            idempotency_key: "key-1".into(),
        })
    };
    let (_, first) = ambient_call(&bridge, command()).await;
    assert_eq!(first["error_code"], "");
    let run_id = first["run_id"].as_str().expect("идентификатор").to_string();
    assert!(!run_id.is_empty());
    assert_eq!(first["deduplicated"], false);

    let (_, second) = ambient_call(&bridge, command()).await;
    assert_eq!(second["run_id"], run_id);
    assert_eq!(second["deduplicated"], true);
    assert_eq!(
        bridge
            .journal()
            .list_workflow_runs(10)
            .await
            .expect("список запусков")
            .len(),
        1
    );
}

/// Проекция запуска несёт состояния и роли, но не цель child, не prompt и
/// не сырой вывод.
#[tokio::test]
async fn a_run_projection_carries_no_prompt_goal_or_raw_output() {
    let (bridge, directory) = workflow_bridge("workflow-projection");
    let (_, started) = ambient_call(
        &bridge,
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "repository-research".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![generated::WorkflowInput {
                name: "question".into(),
                value: "секретная формулировка вопроса".into(),
            }],
            idempotency_key: "key-1".into(),
        }),
    )
    .await;
    let run_id = started["run_id"]
        .as_str()
        .expect("идентификатор")
        .to_string();

    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowRun(generated::GetWorkflowRun {
            run_id: run_id.clone(),
        }),
    )
    .await;
    assert_eq!(event_type, "workflow.run");
    assert_eq!(payload["error_code"], "");
    assert_eq!(payload["run_id"], run_id);
    let rendered = payload.to_string();
    assert!(
        !rendered.contains("секретная формулировка вопроса"),
        "цель узла не должна доходить до renderer: {rendered}"
    );
    let nodes = payload["nodes"].as_array().expect("узлы");
    assert_eq!(nodes.len(), 4);
    for node in nodes {
        assert!(node.get("node_id").is_some());
        assert!(node.get("state").is_some());
        assert!(node.get("output").is_none(), "сырой вывод наружу не уходит");
    }
}

/// Неизвестный запуск даёт `unknown_state`, а не выдуманный успех.
#[tokio::test]
async fn an_unknown_run_is_reported_as_unknown_state() {
    let (bridge, _directory) = workflow_bridge("workflow-unknown-run");
    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::GetWorkflowRun(generated::GetWorkflowRun {
            run_id: "missing".into(),
        }),
    )
    .await;
    assert_eq!(payload["error_code"], "unknown_run");
    assert_eq!(payload["state"], "unknown_state");

    let (_, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::CancelWorkflow(generated::CancelWorkflow {
            run_id: "missing".into(),
        }),
    )
    .await;
    assert_eq!(payload["cancelled"], false);
    assert_eq!(payload["error_code"], "not_cancellable");
}

/// События запуска durable, монотонны и доступны для replay с любой точки.
#[tokio::test]
async fn run_events_replay_from_any_sequence() {
    let (bridge, directory) = workflow_bridge("workflow-events");
    let (_, started) = ambient_call(
        &bridge,
        generated::command_envelope::Command::StartWorkflow(generated::StartWorkflow {
            template_id: "parallel-security-review".into(),
            task_id: "task-1".into(),
            workspace_path: directory.path().to_string_lossy().to_string(),
            inputs: vec![generated::WorkflowInput {
                name: "scope".into(),
                value: "crates".into(),
            }],
            idempotency_key: "key-1".into(),
        }),
    )
    .await;
    let run_id = started["run_id"]
        .as_str()
        .expect("идентификатор")
        .to_string();

    let (event_type, payload) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListWorkflowEvents(generated::ListWorkflowEvents {
            run_id: run_id.clone(),
            after_sequence: -1,
            limit: 100,
        }),
    )
    .await;
    assert_eq!(event_type, "workflow.events");
    let events = payload["events"].as_array().expect("события");
    assert!(!events.is_empty());
    assert_eq!(events[0]["event_type"], "workflow.run_started");
    let sequences: Vec<i64> = events
        .iter()
        .map(|event| event["sequence"].as_i64().unwrap_or_default())
        .collect();
    let mut sorted = sequences.clone();
    sorted.sort();
    assert_eq!(sequences, sorted);

    let (_, tail) = ambient_call(
        &bridge,
        generated::command_envelope::Command::ListWorkflowEvents(generated::ListWorkflowEvents {
            run_id,
            after_sequence: 0,
            limit: 100,
        }),
    )
    .await;
    let tail_events = tail["events"].as_array().expect("хвост");
    assert!(tail_events
        .iter()
        .all(|event| event["sequence"].as_i64().unwrap_or_default() > 0));
}

#[tokio::test]
async fn analysis_kernel_ipc_is_bounded_idempotent_and_version_checked() {
    let directory = tempfile::tempdir().expect("temp dir");
    let journal =
        EventJournal::open(directory.path().join("kernel-ipc.db")).expect("journal opens");
    let bridge = IpcBridge::new(journal);
    let created = bridge
        .dispatch_create_analysis_kernel(generated::CreateAnalysisKernel {
            task_id: "task-kernel-ipc".into(),
            workspace_id: "workspace-kernel-ipc".into(),
            runtime_version: "trusted-local-1".into(),
            package_manifest_hash: "a".repeat(64),
            policy_hash: "b".repeat(64),
            ..Default::default()
        })
        .await;
    assert_eq!(created.status, "running");
    assert_eq!(created.revision, 1);

    let put = generated::ExecuteAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            request_id: "object-put-request".into(),
            operation: "object_put".into(),
            args: br#"{"logical_name":"rows","type_hint":"json","value":[1,2,3],"sensitivity":"internal"}"#.to_vec(),
            correlation_id: "object-put-correlation".into(),
            idempotency_key: "object-put-idem".into(),
            ..Default::default()
        };
    let result = bridge.dispatch_execute_analysis_kernel(put.clone()).await;
    assert_eq!(result.status, "ok", "error={}", result.error_class);
    assert!(result.inline_result.is_empty());
    let object = result.object_ref.expect("metadata object ref");
    assert_eq!(object.logical_name, "rows");
    assert!(object.artifact_locator.is_empty());
    let duplicate = bridge.dispatch_execute_analysis_kernel(put).await;
    assert_eq!(duplicate.error_class, "duplicate_request");

    let denied = bridge
        .dispatch_execute_analysis_kernel(generated::ExecuteAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            request_id: "artifact-read-request".into(),
            operation: "artifact_read".into(),
            args: br#"{"locator":"artifact://missing"}"#.to_vec(),
            correlation_id: "artifact-read-correlation".into(),
            idempotency_key: "artifact-read-idem".into(),
            ..Default::default()
        })
        .await;
    assert_eq!(denied.error_class, "forbidden_capability");

    let stale = bridge
        .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            expected_revision: 0,
            idempotency_key: "reset-idem".into(),
        })
        .await;
    assert_eq!(stale.error_class, "stale_revision");
    let still_running = bridge
        .dispatch_get_analysis_kernel(generated::GetAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            ..Default::default()
        })
        .await;
    assert_eq!(still_running.status, "running");
    assert_eq!(still_running.object_count, 1);

    let reset = bridge
        .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
            kernel_id: created.kernel_id.clone(),
            expected_revision: 1,
            idempotency_key: "reset-idem".into(),
        })
        .await;
    assert_eq!(reset.status, "reset");
    let duplicate_reset = bridge
        .dispatch_reset_analysis_kernel(generated::ResetAnalysisKernel {
            kernel_id: created.kernel_id,
            expected_revision: 1,
            idempotency_key: "reset-idem".into(),
        })
        .await;
    assert_eq!(duplicate_reset.error_class, "duplicate_request");
}
