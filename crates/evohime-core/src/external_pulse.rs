//! IPC proxy for querying the external `evohime-hive-daemon`.

use std::time::Duration;

use evohime_desktop_ipc::transport;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::eva_pulse::{
    HiveObservationControlRequest, HiveObservationSnapshot, DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE,
    HIVE_OBSERVATION_CONTROL_PIPE_ENV, HIVE_OBSERVATION_CONTROL_SOCKET_ENV,
};

const OBSERVATION_IPC_TIMEOUT: Duration = Duration::from_secs(2);

/// Proxy client that forwards read-only observation status queries over local IPC
/// to the standalone `evohime-hive-daemon` process.
#[derive(Debug, Clone, Default)]
pub struct ExternalHiveObservationProxy {
    endpoint_override: Option<String>,
}

impl ExternalHiveObservationProxy {
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

    /// Queries the current status snapshot from the external Hive observation daemon.
    pub async fn snapshot(&self) -> Result<HiveObservationSnapshot, String> {
        self.send_request(HiveObservationControlRequest::GetObservationStatus)
            .await
    }

    /// Notifies the external Hive observation daemon that `HiveSettings` changed.
    pub async fn notify_settings_changed(&self) -> Result<(), String> {
        let _ = self
            .send_request(HiveObservationControlRequest::NotifySettingsChanged)
            .await?;
        Ok(())
    }

    async fn send_request(
        &self,
        request: HiveObservationControlRequest,
    ) -> Result<HiveObservationSnapshot, String> {
        let payload = serde_json::to_vec(&request)
            .map_err(|error| format!("observation_serialization_failed: {error}"))?;
        let response_bytes = tokio::time::timeout(OBSERVATION_IPC_TIMEOUT, async {
            let mut stream = self.connect_stream().await?;
            exchange_frame(&mut stream, &payload).await
        })
        .await
        .map_err(|_| "observation_unavailable: IPC request timed out".to_owned())??;

        serde_json::from_slice::<HiveObservationSnapshot>(&response_bytes)
            .map_err(|error| format!("observation_unavailable: invalid daemon response: {error}"))
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
                std::env::var(HIVE_OBSERVATION_CONTROL_PIPE_ENV)
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .unwrap_or_else(|| DEFAULT_HIVE_OBSERVATION_CONTROL_PIPE.to_owned());

        for attempt in 0..4 {
            match ClientOptions::new().open(&pipe_name) {
                Ok(client) => return Ok(client),
                Err(error) if error.raw_os_error() == Some(231) && attempt < 3 => {
                    tokio::time::sleep(Duration::from_millis(15)).await;
                }
                Err(error) => {
                    return Err(format!("observation_unavailable: {error}"));
                }
            }
        }

        Err("observation_unavailable: pipe busy".to_owned())
    }

    #[cfg(unix)]
    async fn connect_stream(&self) -> Result<tokio::net::UnixStream, String> {
        use std::path::PathBuf;

        let socket_path = if let Some(endpoint) = &self.endpoint_override {
            PathBuf::from(endpoint)
        } else if let Some(env_path) =
            std::env::var_os(HIVE_OBSERVATION_CONTROL_SOCKET_ENV).filter(|value| !value.is_empty())
        {
            PathBuf::from(env_path)
        } else {
            let paths = evohime_desktop_ipc::unix_runtime::UnixRuntimePaths::current()
                .map_err(|error| format!("observation_unavailable: {error}"))?;
            paths.directory.join("hive-observation.sock")
        };

        tokio::net::UnixStream::connect(&socket_path)
            .await
            .map_err(|error| format!("observation_unavailable: {error}"))
    }
}

async fn exchange_frame<S>(stream: &mut S, request_bytes: &[u8]) -> Result<Vec<u8>, String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    transport::write_frame(stream, request_bytes)
        .await
        .map_err(|error| format!("observation_unavailable: IPC write failed: {error}"))?;
    transport::read_frame(stream)
        .await
        .map_err(|error| format!("observation_unavailable: IPC read failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_observation_unavailable_when_daemon_is_absent() {
        let endpoint = if cfg!(windows) {
            format!(
                r"\\.\pipe\evohime-hive-observation-missing-{}",
                uuid::Uuid::new_v4()
            )
        } else {
            format!(
                "/tmp/evohime-hive-observation-missing-{}.sock",
                uuid::Uuid::new_v4()
            )
        };
        let proxy = ExternalHiveObservationProxy::with_endpoint(endpoint);

        let status_err = proxy
            .snapshot()
            .await
            .expect_err("missing daemon should return unavailable");
        assert!(status_err.starts_with("observation_unavailable"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn forwards_observation_status_over_named_pipe() {
        use tokio::net::windows::named_pipe::ServerOptions;

        let pipe_name = format!(
            r"\\.\pipe\evohime-hive-observation-test-{}",
            uuid::Uuid::new_v4()
        );
        let server_pipe = pipe_name.clone();
        let server_task = tokio::spawn(async move {
            let mut server = ServerOptions::new()
                .create(&server_pipe)
                .expect("create test named pipe");
            server.connect().await.expect("accept client");
            let req_bytes = transport::read_frame(&mut server)
                .await
                .expect("read request frame");
            let req: HiveObservationControlRequest =
                serde_json::from_slice(&req_bytes).expect("decode request");
            let snapshot = match req {
                HiveObservationControlRequest::GetObservationStatus
                | HiveObservationControlRequest::NotifySettingsChanged
                | HiveObservationControlRequest::GetCityMap
                | HiveObservationControlRequest::GetHoneycombWorld
                | HiveObservationControlRequest::GetHoneycombDelta { .. } => {
                    HiveObservationSnapshot {
                        schema_version: 1,
                        owner: "hive_observation".to_owned(),
                        status: "observing".to_owned(),
                        enabled: true,
                        next_wake_at_ms: Some(12345),
                    }
                }
            };
            let resp_bytes = serde_json::to_vec(&snapshot).expect("encode snapshot");
            transport::write_frame(&mut server, &resp_bytes)
                .await
                .expect("write response frame");
        });

        tokio::time::sleep(Duration::from_millis(20)).await;
        let proxy = ExternalHiveObservationProxy::with_endpoint(pipe_name);

        let status = proxy.snapshot().await.expect("get observation status");
        assert!(status.enabled);
        assert_eq!(status.status, "observing");
        assert_eq!(status.next_wake_at_ms, Some(12345));

        server_task.await.expect("server task completes");
    }
}
