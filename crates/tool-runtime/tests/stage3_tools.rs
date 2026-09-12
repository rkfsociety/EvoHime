use evohime_tool_runtime::{patch, search, shell, write, ToolContext, ToolError, ToolRegistry};
use serde_json::json;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

fn run_async_test_with_large_stack<F>(test: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    std::thread::Builder::new()
        .name("evohime-tool-runtime-integration-test".into())
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
fn write_creates_and_updates_nested_file() {
    run_async_test_with_large_stack(write_creates_and_updates_nested_file_inner());
}

async fn write_creates_and_updates_nested_file_inner() {
    let dir = tempdir().unwrap();
    let ctx = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let first = write::execute(&ctx, json!({"path":"nested/a.txt","content":"one"}))
        .await
        .unwrap();
    assert_eq!(first.structured["change"], "created");
    let hash = first.structured["content_hash"].as_str().unwrap();
    let second = write::execute(
        &ctx,
        json!({"path":"nested/a.txt","content":"two","expected_hash":hash}),
    )
    .await
    .unwrap();
    assert_eq!(second.structured["change"], "updated");
}

#[test]
fn patch_rejects_context_mismatch_without_mutation() {
    run_async_test_with_large_stack(patch_rejects_context_mismatch_without_mutation_inner());
}

async fn patch_rejects_context_mismatch_without_mutation_inner() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    let ctx = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let result = patch::execute(
        &ctx,
        json!({"path":"a.txt","patch":"@@ -1,1 +1,1 @@\n-wrong\n+new\n","expected_hash":"c3f9c8c283a2b1f2f1896f27a01cbe3cddc0c9d93f752e4639035a0f5b36f6e8"}),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\ntwo\n"
    );
}

#[test]
fn registry_requires_approval_for_write() {
    run_async_test_with_large_stack(registry_requires_approval_for_write_inner());
}

async fn registry_requires_approval_for_write_inner() {
    let dir = tempdir().unwrap();
    let result = ToolRegistry::bootstrap()
        .execute(
            &ToolContext {
                workspace_root: dir.path().to_path_buf(),
                task_id: Uuid::nil(),
                session_id: None,
                progress_tx: None,
            },
            "filesystem.write",
            json!({"path":"a.txt","content":"x"}),
        )
        .await;
    assert!(matches!(
        result,
        Err(ToolError::NeedsApproval(details))
            if details.tool == "filesystem.write" && details.input["path"] == "a.txt"
    ));
}

#[test]
fn shell_runs_direct_executable_and_rejects_wrapper() {
    run_async_test_with_large_stack(shell_runs_direct_executable_and_rejects_wrapper_inner());
}

async fn shell_runs_direct_executable_and_rejects_wrapper_inner() {
    let dir = tempdir().unwrap();
    let ctx = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let (program, args) = if cfg!(windows) {
        ("rustc", vec!["--version"])
    } else {
        ("printf", vec!["hello"])
    };
    let result = shell::execute(
        &ctx,
        json!({"program":program,"args":args}),
        CancellationToken::new(),
    )
    .await
    .unwrap();
    assert!(!result.structured["stdout"].as_str().unwrap().is_empty());
    let rejected =
        shell::execute(&ctx, json!({"program":"cmd.exe"}), CancellationToken::new()).await;
    assert!(matches!(rejected, Err(ToolError::InvalidInput { .. })));
}

#[test]
fn shell_times_out_and_reports_timeout() {
    run_async_test_with_large_stack(shell_times_out_and_reports_timeout_inner());
}

async fn shell_times_out_and_reports_timeout_inner() {
    let dir = tempdir().unwrap();
    let ctx = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let (program, args) = if cfg!(windows) {
        ("ping", vec!["-n", "5", "127.0.0.1"])
    } else {
        ("sleep", vec!["2"])
    };
    let result = shell::execute(
        &ctx,
        json!({"program":program,"args":args,"timeout_ms":1}),
        CancellationToken::new(),
    )
    .await;
    assert!(matches!(result, Err(ToolError::TimedOut(_))));
}

#[test]
fn test_filesystem_read_only_behavior() {
    run_async_test_with_large_stack(test_filesystem_read_only_behavior_inner());
}

async fn test_filesystem_read_only_behavior_inner() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("test.txt"), "content").unwrap();
    let ctx = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };

    let before = std::fs::read_to_string(dir.path().join("test.txt")).unwrap();

    let registry = ToolRegistry::bootstrap();
    let _ = registry
        .execute(&ctx, "filesystem.list", json!({"path": "."}))
        .await
        .unwrap();
    let _ = evohime_tool_runtime::filesystem::execute(&ctx, json!({"path": "test.txt"}))
        .await
        .unwrap();
    let _ = search::execute(&ctx, json!({"query": "content"}))
        .await
        .unwrap();

    let after = std::fs::read_to_string(dir.path().join("test.txt")).unwrap();

    assert_eq!(
        before, after,
        "Filesystem state changed after read-only tools execution"
    );
}

#[test]
fn patch_context_recovery_on_wrong_hunk_start() {
    run_async_test_with_large_stack(patch_context_recovery_on_wrong_hunk_start_inner());
}

async fn patch_context_recovery_on_wrong_hunk_start_inner() {
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "line1\nline2\nline3\n").unwrap();
    let ctx = ToolContext {
        workspace_root: dir.path().to_path_buf(),
        task_id: Uuid::nil(),
        session_id: None,
        progress_tx: None,
    };
    let result = patch::execute(
        &ctx,
        json!({"path":"file.txt","patch":"@@ -5,1 +5,1 @@\n-line2\n+modified\n","expected_hash":"66663af9c7aa341431a8ee2ff27b72abd06c9218f517bb6fef948e4803c19e03"}),
    )
    .await;
    assert!(result.is_err());
    let content = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(content, "line1\nline2\nline3\n");
}
