//! Core-launched packaged browser adapter.
//!
//! The adapter is an untrusted effect worker. Core owns the session and only
//! sends bounded, typed JSON commands; the worker never receives a workspace
//! path, credentials, CDP endpoint, or model prompt.

use serde_json::{json, Value};
use std::{path::PathBuf, process::Stdio};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout, Command},
};

pub struct BrowserBackendProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl BrowserBackendProcess {
    pub async fn spawn() -> Result<Self, String> {
        let executable = std::env::var_os("EVOHIME_BROWSER_BACKEND_EXE")
            .map(PathBuf::from)
            .ok_or_else(|| "browser_backend_unavailable".to_string())?;
        let mut child = Command::new(executable)
            .arg("--evohime-browser-backend")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "browser_backend_spawn_failed".to_string())?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "browser_backend_stdio_failed".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "browser_backend_stdio_failed".to_string())?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    pub async fn request(&mut self, request: Value) -> Result<Value, String> {
        let line = serde_json::to_string(&request)
            .map_err(|_| "browser_backend_encode_failed".to_string())?;
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|_| "browser_backend_write_failed".to_string())?;
        self.stdin
            .write_all(b"\n")
            .await
            .map_err(|_| "browser_backend_write_failed".to_string())?;
        self.stdin
            .flush()
            .await
            .map_err(|_| "browser_backend_write_failed".to_string())?;
        let mut response = String::new();
        let read = self
            .stdout
            .read_line(&mut response)
            .await
            .map_err(|_| "browser_backend_read_failed".to_string())?;
        if read == 0 {
            let _ = self.child.kill().await;
            return Err("browser_backend_unknown_outcome".to_string());
        }
        serde_json::from_str(&response).map_err(|_| "browser_backend_invalid_response".to_string())
    }

    pub fn command(id: &str, operation: &str, payload: &Value) -> Value {
        let mut command = json!({"id": id, "op": operation});
        if let (Some(target), Some(object)) = (command.as_object_mut(), payload.as_object()) {
            target.extend(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone())),
            );
        }
        command
    }
}
