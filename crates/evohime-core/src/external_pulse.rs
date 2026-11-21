//! IPC proxy for controlling and querying the external `evohime-hive-daemon`.

use std::time::Duration;

use evohime_desktop_ipc::transport;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::eva_pulse::{
    EvaPulseSnapshot, HivePulseControlRequest, DEFAULT_HIVE_PULSE_CONTROL_PIPE,
    HIVE_PULSE_CONTROL_PIPE_ENV, HIVE_PULSE_CONTROL_SOCKET_ENV,
};

const PULSE_IPC_TIMEOUT: Duration = Duration::from_secs(2);

/// Proxy client that forwards EvaPulse control commands over local IPC
/// to the standalone `evohime-hive-daemon` process.
#[derive(Debug, Clone, Default)]
pub struct ExternalHivePulseProxy {
    endpoint_override: Option<String>,
}

impl ExternalHivePulseProxy {
    /// Creates a proxy using the default platform IPC endpoint
    /// (or the environment override if configured).
    pub fn new() -> Self {
        Self {
            endpoint_override: None,
        }
    }

    /// Creates a proxy targeting a specific pipe name or Unix socket path.
    pub fn with_endpoint(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint_override: Some(endpoint.into()),
        }
    }

    /// Enables or pauses the external EvaPulse daemon and returns its updated snapshot.
    pub async fn set_enabled(&self, enabled: bool) -> Result<EvaPulseSnapshot, String> {
        self.send_request(HivePulseControlRequest::TogglePulse { enabled })
            .await
    }

    /// Queries the current status snapshot from the external EvaPulse daemon.
    pub async fn snapshot(&self) -> Result<EvaPulseSnapshot, String> {
        self.send_request(HivePulseControlRequest::GetPulseStatus)
            .await
    }

    async fn send_request(
        &self,
        request: HivePulseControlRequest,
    ) -> Result<EvaPulseSnapshot, String> {
        let payload = serde_json::to_vec(&request)
            .map_err(|error| format!("pulse_serialization_failed: {error}"))?;
        let response_bytes = tokio::time::timeout(PULSE_IPC_TIMEOUT, async {
            let mut stream = self.connect_stream().await?;
            exchange_frame(&mut stream, &payload).await
        })
        .await
        .map_err(|_| "pulse_unavailable: IPC request timed out".to_owned())??;

        serde_json::from_slice::<EvaPulseSnapshot>(&response_bytes)
            .map_err(|error| format!("pulse_unavailable: invalid daemon response: {error}"))
    }

    #[cfg(windows)]
    async fn connect_stream(
        &self,
    ) -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String> {
        use tokio::net::windows::named_pipe::ClientOptions;

        let pipe_name = self
            .endpoint_override
            .clone()
            .or_else(|| {
                std::env::var(HIVE_PULSE_CONTROL_PIPE_ENV)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_HIVE_PULSE_CONTROL_PIPE.to_owned());

        for attempt in 0..4 {
            match ClientOptions::new().open(&pipe_name) {
                Ok(client) => return Ok(client),
                Err(error) if error.raw_os_error() == Some(231) && attempt < 3 => {
                    tokio::time::sleep(Duration::from_millis(15)).await;
                }
                Err(error) => {
                    return Err(format!("pulse_unavailable: {error}"));
                }
            }
        }

        Err("pulse_unavailable: pipe busy".to_owned())
    }

    #[cfg(unix)]
    async fn connect_stream(&self) -> Result<tokio::net::UnixStream, String> {
        use std::path::PathBuf;

        let socket_path = if let Some(endpoint) = &self.endpoint_override {
            PathBuf::from(endpoint)
        } else if let Some(env_path) = std::env::var_os(HIVE_PULSE_CONTROL_SOCKET_ENV)
            .filter(|value| !value.is_empty())
        {
            PathBuf::from(env_path)
        } else {
            let paths = evohime_desktop_ipc::unix_runtime::UnixRuntimePaths::current()
                .map_err(|error| format!("pulse_unavailable: {error}"))?;
            paths.directory.join("hive-pulse.sock")
        };

        tokio::net::UnixStream::connect(&socket_path)
            .await
            .map_err(|error| format!("pulse_unavailable: {error}"))
    }
}

async fn exchange_frame<S>(stream: &mut S, request_bytes: &[u8]) -> Result<Vec<u8>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    transport::write_frame(stream, request_bytes)
        .await
        .map_err(|error| format!("pulse_unavailable: IPC write failed: {error}"))?;
    transport::read_frame(stream)
        .await
        .map_err(|error| format!("pulse_unavailable: IPC read failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_pulse_unavailable_when_daemon_is_absent() {
        let endpoint = if cfg!(windows) {
            format!(r"\\.\pipe\evohime-hive-pulse-missing-{}", uuid::Uuid::new_v4())
        } else {
            format!("/tmp/evohime-hive-pulse-missing-{}.sock", uuid::Uuid::new_v4())
        };
        let proxy = ExternalHivePulseProxy::with_endpoint(endpoint);

        let status_err = proxy
            .snapshot()
            .await
            .expect_err("missing daemon should return unavailable");
        assert!(status_err.starts_with("pulse_unavailable"));

        let toggle_err = proxy
            .set_enabled(true)
            .await
            .expect_err("missing daemon should return unavailable");
        assert!(toggle_err.starts_with("pulse_unavailable"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn forwards_toggle_and_status_over_named_pipe() {
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe_name = format!(r"\\.\pipe\evohime-hive-pulse-test-{}", uuid::Uuid::new_v4());
        let server_pipe = pipe_name.clone();
        let server_task = tokio::spawn(async move {
            for _ in 0..2 {
                let mut server = ServerOptions::new()
                    .create(&server_pipe)
                    .expect("create test named pipe");
                server.connect().await.expect("accept client");
                let req_bytes = transport::read_frame(&mut server)
                    .await
                    .expect("read request frame");
                let req: HivePulseControlRequest =
                    serde_json::from_slice(&req_bytes).expect("decode request");
                let snapshot = match req {
                    HivePulseControlRequest::TogglePulse { enabled } => EvaPulseSnapshot {
                        schema_version: 1,
                        owner: "eva_pulse".to_owned(),
                        status: if enabled {
                            "active".to_owned()
                        } else {
                            "disabled".to_owned()
                        },
                        enabled,
                        next_wake_at_ms: Some(12345),
                    },
                    HivePulseControlRequest::GetPulseStatus => EvaPulseSnapshot {
                        schema_version: 1,
                        owner: "eva_pulse".to_owned(),
                        status: "active".to_owned(),
                        enabled: true,
                        next_wake_at_ms: Some(12345),
                    },
                };
                let resp_bytes = serde_json::to_vec(&snapshot).expect("encode snapshot");
                transport::write_frame(&mut server, &resp_bytes)
                    .await
                    .expect("write response frame");
            }
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let proxy = ExternalHivePulseProxy::with_endpoint(pipe_name);

        let toggled = proxy.set_enabled(false).await.expect("toggle pulse");
        assert!(!toggled.enabled);
        assert_eq!(toggled.status, "disabled");

        let status = proxy.snapshot().await.expect("get pulse status");
        assert!(status.enabled);
        assert_eq!(status.status, "active");
        assert_eq!(status.next_wake_at_ms, Some(12345));

        server_task.await.expect("server task completes");
    }
}
