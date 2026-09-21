use super::*;

fn run_async_test_with_large_stack<F>(test: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    std::thread::Builder::new()
        .name("evohime-tool-runtime-test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime builds")
                .block_on(test);
        })
        .expect("test thread starts")
        .join()
        .expect("test thread completes");
}

#[test]
fn bootstrap_registers_filesystem_read() {
    let registry = ToolRegistry::bootstrap();
    let tools = registry.list();
    assert_eq!(tools.len(), 66);
    for name in [
        "agent.run",
        "app.list",
        "app.open",
        "archive.create",
        "archive.extract",
        "archive.list",
        "browser.extract",
        "browser.open",
        "cargo.build",
        "cargo.check",
        "cargo.clippy",
        "cargo.fmt",
        "cargo.test",
        "filesystem.copy",
        "filesystem.delete",
        "filesystem.list",
        "filesystem.mkdir",
        "filesystem.move",
        "filesystem.patch",
        "filesystem.read",
        "filesystem.search",
        "filesystem.stat",
        "filesystem.write",
        "git.blame",
        "git.branch",
        "git.changed_files",
        "git.cherry_pick",
        "git.commit",
        "git.diff",
        "git.log",
        "git.merge",
        "git.pull",
        "git.push",
        "git.rebase",
        "git.remote",
        "git.reset",
        "git.revert",
        "git.show",
        "git.stash",
        "git.status",
        "git.tag",
        "http.fetch",
        "logs.grep",
        "logs.tail",
        "mcp.call",
        "memory.search",
        "process.run",
        "shell.execute",
    ] {
        assert!(
            tools.iter().any(|tool| tool.name == name),
            "missing tool {name}"
        );
    }
}

#[test]
fn parallel_calls_complete_independently() {
    run_async_test_with_large_stack(parallel_calls_complete_independently_inner());
}

async fn parallel_calls_complete_independently_inner() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.txt"), "a").expect("write a");
    std::fs::write(dir.path().join("b.txt"), "b").expect("write b");
    let registry = ToolRegistry::bootstrap();
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let results = registry
        .execute_parallel(
            &context,
            vec![
                (
                    "filesystem.read".into(),
                    serde_json::json!({"path":"a.txt"}),
                ),
                (
                    "filesystem.read".into(),
                    serde_json::json!({"path":"b.txt"}),
                ),
            ],
            tokio_util::sync::CancellationToken::new(),
        )
        .await;
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(Result::is_ok));
}

#[test]
fn cancellation_stops_call() {
    run_async_test_with_large_stack(cancellation_stops_call_inner());
}

async fn cancellation_stops_call_inner() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.txt"), "a").expect("write");
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::ShellExecute,
            evohime_permissions::PermissionMode::Allow,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let token = tokio_util::sync::CancellationToken::new();
    token.cancel();
    let result = registry
        .execute_cancellable(
            &context,
            "filesystem.read",
            serde_json::json!({"path":"a.txt"}),
            token,
        )
        .await;
    assert!(matches!(result, Err(ToolError::Execution(message)) if message == "tool cancelled"));
}

#[test]
fn cancellation_propagates_into_shell_execution() {
    std::thread::Builder::new()
        .name("evohime-shell-cancellation-test".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("test runtime builds")
                .block_on(cancellation_propagates_into_shell_execution_inner());
        })
        .expect("test thread starts")
        .join()
        .expect("test thread completes");
}

async fn cancellation_propagates_into_shell_execution_inner() {
    let dir = tempfile::tempdir().expect("tempdir");
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::ShellExecute,
            evohime_permissions::PermissionMode::Allow,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let token = CancellationToken::new();
    let (program, args) = if cfg!(windows) {
        ("ping", vec!["-n", "5", "127.0.0.1"])
    } else {
        ("sleep", vec!["2"])
    };
    let handle = tokio::spawn({
        let registry = registry.clone();
        let context = context.clone();
        let token = token.clone();
        async move {
            registry
                .execute_cancellable(
                    &context,
                    "shell.execute",
                    serde_json::json!({"program":program,"args":args}),
                    token,
                )
                .await
        }
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    token.cancel();
    let result = handle.await.expect("task join");
    assert!(matches!(result, Err(ToolError::Execution(message)) if message == "tool cancelled"));
}

#[test]
fn cancellation_stops_non_shell_tool_in_parallel_execution() {
    run_async_test_with_large_stack(
        cancellation_stops_non_shell_tool_in_parallel_execution_inner(),
    );
}

async fn cancellation_stops_non_shell_tool_in_parallel_execution_inner() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let _ssrf = crate::ssrf::lock_private_override(Some(true));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("late")
                .set_delay(std::time::Duration::from_secs(2)),
        )
        .mount(&server)
        .await;

    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::BrowserAccess,
            evohime_permissions::PermissionMode::Allow,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("tempdir");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let token = CancellationToken::new();
    let cancel = token.clone();
    let handle = tokio::spawn(async move {
        registry
            .execute_parallel(
                &context,
                vec![(
                    "browser.open".into(),
                    serde_json::json!({ "url": format!("{}/slow", server.uri()) }),
                )],
                token,
            )
            .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    let results = handle.await.expect("parallel task");
    assert!(matches!(
        results.as_slice(),
        [Err(ToolError::Execution(message))] if message == "tool cancelled"
    ));
}

#[test]
fn registry_dispatches_browser_open_when_allowed() {
    run_async_test_with_large_stack(registry_dispatches_browser_open_when_allowed_inner());
}

async fn registry_dispatches_browser_open_when_allowed_inner() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let _ssrf = crate::ssrf::lock_private_override(Some(true));
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/page"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "<html><head><title>Hi</title></head><body><p>hello registry</p></body></html>",
        ))
        .mount(&server)
        .await;

    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::BrowserAccess,
            evohime_permissions::PermissionMode::Allow,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("tempdir");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let result = registry
        .execute(
            &context,
            "browser.open",
            serde_json::json!({ "url": format!("{}/page", server.uri()) }),
        )
        .await
        .expect("browser.open should dispatch");
    assert!(result.output.to_lowercase().contains("hi") || result.output.contains("hello"));
}

#[test]
fn scope_from_input_prefers_path_then_cwd_then_url() {
    assert_eq!(
        scope_from_input(
            "filesystem.write",
            &serde_json::json!({ "path": "src\\main.rs" })
        ),
        "src/main.rs"
    );
    assert_eq!(
        scope_from_input("shell.execute", &serde_json::json!({ "cwd": "scripts" })),
        "scripts"
    );
    assert_eq!(
        scope_from_input(
            "browser.open",
            &serde_json::json!({ "url": "https://example.com" })
        ),
        "https://example.com"
    );
    assert_eq!(
        scope_from_input("git.status", &serde_json::json!({})),
        "workspace"
    );
}

#[test]
fn approval_preview_describes_shell_command_without_full_input_dump() {
    let input = serde_json::json!({
        "program": "cargo",
        "args": ["test", "-p", "evohime-core"],
        "cwd": "crates/evohime-core",
        "token": "must-not-be-previewed"
    });
    let preview = approval_preview(
        tools::shell::NAME,
        "crates/evohime-core",
        Some("cargo test -p evohime-core"),
        &input,
    );

    assert_eq!(preview.kind, "command");
    assert_eq!(preview.cwd.as_deref(), Some("crates/evohime-core"));
    assert_eq!(
        preview.command.as_deref(),
        Some("cargo test -p evohime-core")
    );
    assert!(preview.details.is_none());
}

#[test]
fn approval_preview_bounds_patch_details() {
    let input = serde_json::json!({
        "path": "src/lib.rs",
        "patch": "x".repeat(MAX_APPROVAL_PREVIEW_DETAILS + 32)
    });
    let preview = approval_preview(tools::patch::NAME, "src/lib.rs", None, &input);

    assert_eq!(preview.kind, "diff");
    assert!(preview.truncated);
    assert!(preview
        .details
        .as_deref()
        .is_some_and(|details| { details.len() <= MAX_APPROVAL_PREVIEW_DETAILS + 32 }));
}

#[test]
fn policy_denies_shell_subject_after_cd_prefix_is_resolved() {
    run_async_test_with_large_stack(
        policy_denies_shell_subject_after_cd_prefix_is_resolved_inner(),
    );
}

async fn policy_denies_shell_subject_after_cd_prefix_is_resolved_inner() {
    let permissions = evohime_permissions::PermissionEngine::new();
    permissions
        .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
            evohime_permissions::PolicyRule {
                permission: evohime_permissions::Permission::ShellExecute,
                pattern: "rm *".into(),
                mode: evohime_permissions::PermissionMode::Deny,
            },
        ]))
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("workspace");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let error = registry
        .execute(
            &context,
            "shell.execute",
            serde_json::json!({"command": "cd nested && rm -rf target"}),
        )
        .await
        .expect_err("policy deny must happen before approval or spawn");
    assert!(matches!(
        error,
        ToolError::PermissionDenied(Permission::ShellExecute)
    ));
}

#[test]
fn policy_denies_canonical_path_despite_relative_alias() {
    run_async_test_with_large_stack(policy_denies_canonical_path_despite_relative_alias_inner());
}

async fn policy_denies_canonical_path_despite_relative_alias_inner() {
    let permissions = evohime_permissions::PermissionEngine::new();
    let dir = tempfile::tempdir().expect("workspace");
    std::fs::create_dir(dir.path().join("secrets")).expect("secrets directory");
    std::fs::write(dir.path().join("secrets/token.txt"), "secret").expect("secret file");
    let canonical_root = dir.path().canonicalize().expect("canonical workspace");
    let canonical_pattern = format!("{}/secrets/*", canonical_root.display()).replace('\\', "/");
    let canonical_pattern = canonical_pattern
        .strip_prefix("//?/")
        .unwrap_or(&canonical_pattern)
        .to_string();
    permissions
        .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
            evohime_permissions::PolicyRule {
                permission: evohime_permissions::Permission::FilesystemRead,
                pattern: canonical_pattern,
                mode: evohime_permissions::PermissionMode::Deny,
            },
        ]))
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };

    let error = registry
        .execute(
            &context,
            "filesystem.read",
            serde_json::json!({ "path": "secrets/../secrets/token.txt" }),
        )
        .await
        .expect_err("canonical hard deny must happen before read");
    assert!(matches!(
        error,
        ToolError::PermissionDenied(Permission::FilesystemRead)
    ));
}

#[test]
fn policy_uses_a_separate_subject_for_git_push() {
    run_async_test_with_large_stack(policy_uses_a_separate_subject_for_git_push_inner());
}

async fn policy_uses_a_separate_subject_for_git_push_inner() {
    let permissions = evohime_permissions::PermissionEngine::new();
    permissions
        .set_policy_rules(evohime_permissions::PolicyRuleSet::new(vec![
            evohime_permissions::PolicyRule {
                permission: evohime_permissions::Permission::GitWrite,
                pattern: "git push*".into(),
                mode: evohime_permissions::PermissionMode::Deny,
            },
        ]))
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("workspace");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let error = registry
        .execute(
            &context,
            "git.push",
            serde_json::json!({"remote": "origin", "branch": "main"}),
        )
        .await
        .expect_err("git subject policy must deny before git runs");
    assert!(matches!(
        error,
        ToolError::PermissionDenied(Permission::GitWrite)
    ));
}

#[test]
fn ask_mode_creates_scoped_approval() {
    run_async_test_with_large_stack(ask_mode_creates_scoped_approval_inner());
}

async fn ask_mode_creates_scoped_approval_inner() {
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::FilesystemWrite,
            evohime_permissions::PermissionMode::Ask,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("tempdir");
    let session_id = Uuid::new_v4();
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: Some(session_id),
        progress_tx: None,
    };
    let err = registry
        .execute(
            &context,
            "filesystem.write",
            serde_json::json!({ "path": "notes/todo.txt", "content": "x" }),
        )
        .await
        .expect_err("ask mode should require approval");
    match err {
        ToolError::NeedsApproval(details) => {
            assert_eq!(details.scope, "notes/todo.txt");
        }
        other => panic!("expected NeedsApproval, got {other:?}"),
    }
}

#[test]
fn approval_is_bound_to_exact_call_and_rechecks_deny() {
    run_async_test_with_large_stack(approval_is_bound_to_exact_call_and_rechecks_deny_inner());
}

async fn approval_is_bound_to_exact_call_and_rechecks_deny_inner() {
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::FilesystemWrite,
            evohime_permissions::PermissionMode::Ask,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions.clone());
    let dir = tempfile::tempdir().expect("tempdir");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::new_v4(),
        session_id: Some(Uuid::new_v4()),
        progress_tx: None,
    };
    let original = serde_json::json!({
        "path": "notes/todo.txt",
        "content": "approved"
    });
    let approval_id = match registry
        .execute(&context, "filesystem.write", original.clone())
        .await
        .expect_err("ask mode should require approval")
    {
        ToolError::NeedsApproval(details) => details.approval_id,
        other => panic!("expected NeedsApproval, got {other:?}"),
    };
    permissions
        .resolve(approval_id, true)
        .await
        .expect("granted");

    let changed = serde_json::json!({
        "path": "notes/todo.txt",
        "content": "tampered"
    });
    assert!(matches!(
        registry
            .execute_after_approval(
                &context,
                "filesystem.write",
                changed,
                approval_id,
                CancellationToken::new(),
            )
            .await,
        Err(ToolError::ApprovalMismatch)
    ));
    assert!(!dir.path().join("notes/todo.txt").exists());

    permissions
        .set_mode(
            evohime_permissions::Permission::FilesystemWrite,
            evohime_permissions::PermissionMode::Deny,
        )
        .await;
    assert!(matches!(
        registry
            .execute_after_approval(
                &context,
                "filesystem.write",
                original,
                approval_id,
                CancellationToken::new(),
            )
            .await,
        Err(ToolError::PermissionDenied(
            evohime_permissions::Permission::FilesystemWrite
        ))
    ));
    assert!(!dir.path().join("notes/todo.txt").exists());
}

#[test]
fn denied_approval_is_rejected_when_rechecked() {
    run_async_test_with_large_stack(denied_approval_is_rejected_when_rechecked_inner());
}

async fn denied_approval_is_rejected_when_rechecked_inner() {
    let permissions = PermissionEngine::new();
    let input = serde_json::json!({ "path": "notes/todo.txt", "content": "x" });
    let request = permissions
        .create_approval_scoped_for_call(
            Uuid::new_v4(),
            None,
            "filesystem.write",
            evohime_permissions::Permission::FilesystemWrite,
            "notes/todo.txt",
            &input,
        )
        .await;
    permissions
        .resolve(request.id, false)
        .await
        .expect("denied");
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("tempdir");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: request.task_id,
        session_id: request.session_id,
        progress_tx: None,
    };

    assert!(matches!(
        registry
            .execute_after_approval(
                &context,
                "filesystem.write",
                input,
                request.id,
                CancellationToken::new(),
            )
            .await,
        Err(ToolError::ApprovalDenied)
    ));
    assert!(!dir.path().join("notes/todo.txt").exists());
}

#[test]
fn granted_approval_is_consumed_before_execution_and_cannot_replay() {
    run_async_test_with_large_stack(
        granted_approval_is_consumed_before_execution_and_cannot_replay_inner(),
    );
}

async fn granted_approval_is_consumed_before_execution_and_cannot_replay_inner() {
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::FilesystemWrite,
            evohime_permissions::PermissionMode::Ask,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions.clone());
    let dir = tempfile::tempdir().expect("tempdir");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::new_v4(),
        session_id: Some(Uuid::new_v4()),
        progress_tx: None,
    };
    let input = serde_json::json!({
        "path": "notes/once.txt",
        "content": "written once"
    });
    let approval_id = match registry
        .execute(&context, "filesystem.write", input.clone())
        .await
        .expect_err("ask mode should require approval")
    {
        ToolError::NeedsApproval(details) => details.approval_id,
        other => panic!("expected NeedsApproval, got {other:?}"),
    };
    permissions
        .resolve(approval_id, true)
        .await
        .expect("granted");

    let (first, second) = tokio::join!(
        registry.execute_after_approval(
            &context,
            "filesystem.write",
            input.clone(),
            approval_id,
            CancellationToken::new(),
        ),
        registry.execute_after_approval(
            &context,
            "filesystem.write",
            input,
            approval_id,
            CancellationToken::new(),
        )
    );
    let results = [first, second];
    assert_eq!(
        results.iter().filter(|result| result.is_ok()).count(),
        1,
        "one-shot approval must allow exactly one concurrent execution"
    );
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ToolError::ApprovalMismatch)))
            .count(),
        1,
        "replay must be rejected as an approval mismatch"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes/once.txt")).expect("written file"),
        "written once"
    );
}

#[test]
fn post_approval_path_revalidates_patch_input() {
    run_async_test_with_large_stack(post_approval_path_revalidates_patch_input_inner());
}

async fn post_approval_path_revalidates_patch_input_inner() {
    let permissions = PermissionEngine::new();
    let task_id = Uuid::new_v4();
    let malformed = serde_json::json!({ "path": "notes/todo.txt" });
    let request = permissions
        .create_approval_scoped_for_call(
            task_id,
            None,
            "filesystem.patch",
            evohime_permissions::Permission::FilesystemWrite,
            "notes/todo.txt",
            &malformed,
        )
        .await;
    permissions
        .resolve(request.id, true)
        .await
        .expect("approval grants");
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("tempdir");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id,
        session_id: None,
        progress_tx: None,
    };

    assert!(matches!(
        registry
            .execute_after_approval(
                &context,
                "filesystem.patch",
                malformed,
                request.id,
                CancellationToken::new(),
            )
            .await,
        Err(ToolError::InvalidInput { .. })
    ));
}

#[test]
fn oversized_patch_is_rejected_before_approval() {
    run_async_test_with_large_stack(oversized_patch_is_rejected_before_approval_inner());
}

async fn oversized_patch_is_rejected_before_approval_inner() {
    let permissions = PermissionEngine::new();
    permissions
        .set_mode(
            evohime_permissions::Permission::FilesystemWrite,
            evohime_permissions::PermissionMode::Ask,
        )
        .await;
    let registry = ToolRegistry::bootstrap_with_permissions(permissions);
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("file.txt"), "old\n").expect("write fixture");
    let context = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: Some(Uuid::new_v4()),
        progress_tx: None,
    };

    let error = registry
        .execute(
            &context,
            "filesystem.patch",
            serde_json::json!({
                "path": "file.txt",
                "patch": "a".repeat(tools::patch::MAX_PATCH_BYTES + 1)
            }),
        )
        .await
        .expect_err("preflight must reject oversized input");

    assert!(matches!(error, ToolError::InvalidInput { .. }));
}

#[test]
fn every_registered_tool_has_a_valid_non_permissive_manifest() {
    let registry = ToolRegistry::bootstrap();
    assert!(registry.list().len() >= 50);
    for manifest in registry.manifests() {
        manifest.validate().expect("registered manifest validates");
        assert_ne!(
            manifest.input_schema.get("additionalProperties"),
            Some(&serde_json::Value::Bool(true))
        );
        assert!(!manifest.canonical_hash().unwrap().is_empty());
    }
}
