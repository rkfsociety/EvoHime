use evohime_tool_runtime::{patch, shell, write, ToolContext, ToolError, ToolRegistry};
use serde_json::json;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[tokio::test]
async fn write_creates_and_updates_nested_file() {
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
    let second = write::execute(&ctx, json!({"path":"nested/a.txt","content":"two"}))
        .await
        .unwrap();
    assert_eq!(second.structured["change"], "updated");
}

#[tokio::test]
async fn patch_rejects_context_mismatch_without_mutation() {
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
        json!({"path":"a.txt","patch":"@@ -1,1 +1,1 @@\n-wrong\n+new\n"}),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\ntwo\n"
    );
}

#[tokio::test]
async fn registry_requires_approval_for_write() {
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
        Err(ToolError::NeedsApproval { tool, input, .. })
            if tool == "filesystem.write" && input["path"] == "a.txt"
    ));
}

#[tokio::test]
async fn shell_runs_direct_executable_and_rejects_wrapper() {
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

#[tokio::test]
async fn shell_times_out_and_reports_timeout() {
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
