use crate::{ToolContext, ToolError, ToolResult};
use evohime_permissions::Permission;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

// ============================================================================
// cargo.build: Build Rust project
// ============================================================================

pub const BUILD_NAME: &str = "cargo.build";
pub const BUILD_DESCRIPTION: &str = "Build Rust project (cargo build)";
pub const BUILD_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
struct BuildInput {
    #[serde(default)]
    release: bool,
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    features: Option<String>,
}

pub async fn build(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: BuildInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: BUILD_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("cargo");
    cmd.arg("build");

    if opts.release {
        cmd.arg("--release");
    }

    if let Some(pkg) = opts.package {
        cmd.arg("-p").arg(pkg);
    }

    if let Some(features) = opts.features {
        cmd.arg("--features").arg(features);
    }

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("cargo build failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "cargo build failed:\n{}",
            stderr
        )));
    }

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "build",
            "release": opts.release,
            "success": true
        }),
    })
}

// ============================================================================
// cargo.test: Run tests
// ============================================================================

pub const TEST_NAME: &str = "cargo.test";
pub const TEST_DESCRIPTION: &str = "Run Rust tests (cargo test)";
pub const TEST_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const TEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
struct TestInput {
    #[serde(default)]
    test_name: Option<String>,
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    lib: bool,
    #[serde(default)]
    doc: bool,
}

pub async fn test(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: TestInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: TEST_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("cargo");
    cmd.arg("test");

    if let Some(pkg) = opts.package {
        cmd.arg("-p").arg(pkg);
    }

    if opts.lib {
        cmd.arg("--lib");
    }

    if opts.doc {
        cmd.arg("--doc");
    }

    if let Some(test) = opts.test_name {
        cmd.arg(test);
    }

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("cargo test failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    let success = output.status.success();

    Ok(ToolResult {
        output: if success {
            stdout.clone()
        } else {
            stderr.clone()
        },
        structured: json!({
            "action": "test",
            "success": success,
            "stdout": stdout,
            "stderr": stderr
        }),
    })
}

// ============================================================================
// cargo.fmt: Format code
// ============================================================================

pub const FMT_NAME: &str = "cargo.fmt";
pub const FMT_DESCRIPTION: &str = "Format Rust code (cargo fmt)";
pub const FMT_PERMISSIONS: &[Permission] = &[Permission::FilesystemWrite];
pub const FMT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
struct FmtInput {
    #[serde(default)]
    check: bool,
}

pub async fn fmt(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: FmtInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: FMT_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("cargo");
    cmd.arg("fmt");

    if opts.check {
        cmd.arg("--check");
    }

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("cargo fmt failed: {e}")))?;

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && opts.check {
        return Err(ToolError::Execution(format!(
            "Code formatting issues found:\n{}",
            stderr
        )));
    }

    Ok(ToolResult {
        output: if opts.check {
            "Code is properly formatted".to_string()
        } else {
            "Code formatted".to_string()
        },
        structured: json!({
            "action": "fmt",
            "check": opts.check,
            "success": output.status.success()
        }),
    })
}

// ============================================================================
// cargo.clippy: Linting
// ============================================================================

pub const CLIPPY_NAME: &str = "cargo.clippy";
pub const CLIPPY_DESCRIPTION: &str = "Run Rust linter (cargo clippy)";
pub const CLIPPY_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const CLIPPY_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct ClippyInput {
    #[serde(default)]
    package: Option<String>,
    #[serde(default)]
    all_targets: bool,
}

pub async fn clippy(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: ClippyInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: CLIPPY_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("cargo");
    cmd.arg("clippy");

    if let Some(pkg) = opts.package {
        cmd.arg("-p").arg(pkg);
    }

    if opts.all_targets {
        cmd.arg("--all-targets");
    }

    cmd.arg("--");
    cmd.arg("-D").arg("warnings");

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("cargo clippy failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "clippy linting found issues:\n{}",
            stderr
        )));
    }

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "clippy",
            "success": true
        }),
    })
}

// ============================================================================
// cargo.check: Type-check without building
// ============================================================================

pub const CHECK_NAME: &str = "cargo.check";
pub const CHECK_DESCRIPTION: &str = "Type-check Rust code without building (cargo check)";
pub const CHECK_PERMISSIONS: &[Permission] = &[Permission::ShellExecute];
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Deserialize)]
struct CheckInput {
    #[serde(default)]
    package: Option<String>,
}

pub async fn check(ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
    let opts: CheckInput = serde_json::from_value(input).map_err(|e| ToolError::InvalidInput {
        tool: CHECK_NAME.to_string(),
        message: e.to_string(),
    })?;

    let mut cmd = Command::new("cargo");
    cmd.arg("check");

    if let Some(pkg) = opts.package {
        cmd.arg("-p").arg(pkg);
    }

    let output = cmd
        .current_dir(&ctx.workspace_root)
        .output()
        .await
        .map_err(|e| ToolError::Execution(format!("cargo check failed: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "cargo check failed:\n{}",
            stderr
        )));
    }

    Ok(ToolResult {
        output: stdout.clone(),
        structured: json!({
            "action": "check",
            "success": true
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[tokio::test]
    #[ignore]
    async fn cargo_check_works() {
        let ctx = ToolContext {
            workspace_root: std::env::current_dir().expect("cwd"),
            task_id: Uuid::nil(),
            session_id: None,
            progress_tx: None,
        };

        let result = check(&ctx, json!({})).await;
        // Just check it doesn't panic
        let _ = result;
    }
}
