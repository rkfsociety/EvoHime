/// Agent task lifecycle and handoff tools.
pub mod agent;
/// Declared access to the Core-owned Persistent Analysis Kernel.
pub mod analysis;
/// Application catalog inspection and interaction tools.
pub mod app;
/// Bounded archive listing and extraction tools.
pub mod archive;
/// SSRF-guarded web-page opening and extraction tools.
pub mod browser;
pub mod browser_session;
/// Cargo project inspection and verification tools.
pub mod cargo;
/// Revision-safe workspace text reading.
pub mod filesystem;
/// Advanced workspace operations such as structured file discovery.
pub mod filesystem_advanced;
/// Git status and repository inspection tools.
pub mod git;
/// Higher-risk Git operations with explicit policy checks.
pub mod git_advanced;
pub mod git_worktree;
/// Core goal criterion verification registry adapter.
pub mod goal;
pub mod http;
pub mod list;
/// Structured tool and execution log access.
pub mod logs;
/// SSRF-guarded remote MCP JSON-RPC invocation.
pub mod mcp;
/// Agent-memory search registry adapter and result formatting.
pub mod memory;
/// Revision-checked unified-diff patching.
pub mod patch;
/// Bounded process inspection and lifecycle tools.
pub mod process;
/// Allow-listed interpreter script execution for workspace scripts.
pub mod script;
/// Sandboxed workspace content search.
pub mod search;
/// Isolated Queen self-development tools.
pub mod self_development;
/// Policy-bound direct program execution.
pub mod shell;
/// Core-owned durable workflow orchestration entry points.
pub mod workflow;
/// Revision-safe workspace file writing.
pub mod write;

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum UsizeOrString {
    Num(usize),
    Text(String),
}

pub(crate) fn deserialize_optional_usize<'de, D>(deserializer: D) -> Result<Option<usize>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let Some(value) = Option::<UsizeOrString>::deserialize(deserializer)? else {
        return Ok(None);
    };
    match value {
        UsizeOrString::Num(number) => Ok(Some(number)),
        UsizeOrString::Text(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            trimmed.parse::<usize>().map(Some).map_err(|_| {
                serde::de::Error::invalid_value(
                    serde::de::Unexpected::Str(&text),
                    &"a non-negative integer",
                )
            })
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum BoolOrString {
    Flag(bool),
    Text(String),
}

pub(crate) fn deserialize_flexible_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let Some(value) = Option::<BoolOrString>::deserialize(deserializer)? else {
        return Ok(false);
    };
    match value {
        BoolOrString::Flag(flag) => Ok(flag),
        BoolOrString::Text(text) => match text.trim().to_ascii_lowercase().as_str() {
            "" | "false" | "0" | "no" => Ok(false),
            "true" | "1" | "yes" => Ok(true),
            _ => Err(serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&text),
                &"a boolean",
            )),
        },
    }
}

#[cfg(test)]
pub(crate) async fn assert_timeout_terminates_child<F, Fut, T>(run: F)
where
    F: FnOnce(tokio::process::Command) -> Fut,
    Fut: std::future::Future<Output = std::io::Result<T>>,
{
    let dir = tempfile::tempdir().expect("tempdir");
    let ready = dir.path().join("ready");
    let survived = dir.path().join("survived");

    #[cfg(windows)]
    let command = {
        let mut cmd = tokio::process::Command::new("cmd");
        cmd.current_dir(dir.path());
        cmd.args([
            "/C",
            "echo ready>ready & ping -n 2 127.0.0.1 >nul & echo survived>survived",
        ]);
        cmd
    };

    #[cfg(not(windows))]
    let command = {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.current_dir(dir.path());
        cmd.args([
            "-c",
            "printf ready > ready; sleep 1; printf survived > survived",
        ]);
        cmd
    };

    {
        let fut = run(command);
        tokio::pin!(fut);

        let wait_ready = async {
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while !ready.exists() {
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "child process did not signal readiness in time"
                );
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        };

        tokio::select! {
            res = &mut fut => panic!("child process finished before timeout (ok={})", res.is_ok()),
            _ = wait_ready => {}
        }

        let timed_out = tokio::time::timeout(std::time::Duration::from_millis(50), fut).await;
        assert!(
            timed_out.is_err(),
            "expected timeout to cancel child future"
        );
    }

    // Wait longer than the child's sleep (~1s) to verify it was actually terminated.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert!(
        !survived.exists(),
        "child process survived after timeout and wrote {}",
        survived.display()
    );
}
