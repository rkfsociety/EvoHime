//! Owner-only Linux Unix-domain socket server for `desktop-ipc-v1`.

use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::sync::Arc;

use evohime_desktop_ipc::session::{
    HandshakeRequest, HandshakeVerifier, LaunchContext, PeerIdentity, DEFAULT_NONCE_TTL_MS,
};
use evohime_desktop_ipc::{generated, transport, unix_runtime::UnixRuntimePaths};
use prost::Message;
use tokio::io::{split, AsyncRead, AsyncWrite};
use tokio::net::{UnixListener, UnixStream};

use crate::{IpcBridge, StructuredLogger};

const HANDSHAKE_TIMEOUT_MS: u64 = 10_000;
const OUTBOUND_FRAME_CAPACITY: usize = 128;

/// Bound Linux server endpoint and the matching authenticated session.
pub struct UnixPipeServerConfig {
    listener: UnixListener,
    context: LaunchContext,
}

impl UnixPipeServerConfig {
    /// Binds the private socket and writes its owner-only launch context.
    ///
    /// Returns `Ok(None)` when another Core instance already owns the socket.
    pub async fn bind(
        paths: &UnixRuntimePaths,
    ) -> Result<Option<Self>, Box<dyn std::error::Error + Send + Sync>> {
        let listener = match UnixListener::bind(&paths.socket) {
            Ok(listener) => listener,
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                if UnixStream::connect(&paths.socket).await.is_ok() {
                    return Ok(None);
                }
                let metadata = std::fs::symlink_metadata(&paths.socket)?;
                if !metadata.file_type().is_socket() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "existing Linux IPC endpoint is not a socket",
                    )
                    .into());
                }
                std::fs::remove_file(&paths.socket)?;
                UnixListener::bind(&paths.socket)?
            }
            Err(error) => return Err(error.into()),
        };

        std::fs::set_permissions(&paths.socket, std::fs::Permissions::from_mode(0o600))?;
        let socket_metadata = std::fs::metadata(&paths.socket)?;
        let context =
            LaunchContext::generate(socket_metadata.uid().to_string(), String::new(), now_ms())
                .map_err(|error| std::io::Error::other(error.to_string()))?;

        match std::fs::symlink_metadata(&paths.launch_context) {
            Ok(metadata) if metadata.file_type().is_file() => {
                std::fs::remove_file(&paths.launch_context)?;
            }
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "existing Linux launch context is not a regular file",
                )
                .into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        evohime_desktop_ipc::session::write_launch_context(&paths.launch_context, &context)?;

        Ok(Some(Self { listener, context }))
    }

    /// Returns the launch context used to authenticate the Linux socket.
    pub fn context(&self) -> &LaunchContext {
        &self.context
    }
}

/// Serves authenticated desktop IPC over the bound Linux Unix socket.
pub async fn run_unix_pipe(
    config: UnixPipeServerConfig,
    bridge: Arc<IpcBridge>,
    logger: Arc<StructuredLogger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut verifier = HandshakeVerifier::new(config.context, DEFAULT_NONCE_TTL_MS)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let _ = logger.write(
        "info",
        "ipc.listening",
        serde_json::json!({"authenticated": true, "transport": "unix-domain-socket", "mode": "0600"}),
    );

    loop {
        let (stream, _) = config.listener.accept().await?;
        let peer = match stream.peer_cred() {
            Ok(credentials) => PeerIdentity {
                user_sid: credentials.uid().to_string(),
                logon_session: String::new(),
            },
            Err(error) => {
                let _ = logger.write(
                    "warn",
                    "ipc.peer_identity_failed",
                    serde_json::json!({"error": error.to_string()}),
                );
                continue;
            }
        };
        let (mut response_reader, mut response_writer) = split(stream);
        match authenticate(
            &mut verifier,
            &bridge,
            &logger,
            peer,
            &mut response_reader,
            &mut response_writer,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                let _ = logger.write(
                    "warn",
                    "ipc.handshake_failed",
                    serde_json::json!({"error": error.to_string()}),
                );
                continue;
            }
        }

        let (frames, mut outbound) = tokio::sync::mpsc::channel::<Vec<u8>>(OUTBOUND_FRAME_CAPACITY);
        let writer_task = tokio::spawn(async move {
            while let Some(bytes) = outbound.recv().await {
                if tokio::io::AsyncWriteExt::write_all(&mut response_writer, &bytes)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        let pump = bridge.journalled().map(|mut journalled| {
            let bridge = Arc::clone(&bridge);
            let mut sink = ChannelWriter::new(frames.clone());
            tokio::spawn(async move {
                let mut pushed = bridge.latest_sequence().await;
                while journalled.changed().await.is_ok() {
                    match bridge.push_journal_tail(&mut sink, pushed).await {
                        Ok(sequence) => pushed = sequence,
                        Err(_) => break,
                    }
                }
            })
        });

        let mut sink = ChannelWriter::new(frames);
        loop {
            if let Err(error) = bridge.process_once(&mut response_reader, &mut sink).await {
                let _ = logger.write(
                    "info",
                    "ipc.connection_closed",
                    serde_json::json!({"error": error.to_string()}),
                );
                break;
            }
        }
        if let Some(pump) = pump {
            pump.abort();
        }
        writer_task.abort();
    }
}

async fn authenticate<R, W>(
    verifier: &mut HandshakeVerifier,
    bridge: &IpcBridge,
    logger: &Arc<StructuredLogger>,
    peer: PeerIdentity,
    reader: &mut R,
    writer: &mut W,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let nonce = verifier
        .issue_nonce(now_ms())
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let challenge = bridge.control_event(
        "ipc.challenge",
        Some(generated::event_envelope::Event::AuthChallenge(
            generated::AuthChallenge {
                nonce: nonce.value.clone(),
                expires_at_ms: nonce.expires_at_ms,
            },
        )),
        Vec::new(),
    );
    transport::write_frame(writer, &challenge.encode_to_vec()).await?;
    let payload = match tokio::time::timeout(
        std::time::Duration::from_millis(HANDSHAKE_TIMEOUT_MS),
        transport::read_frame(reader),
    )
    .await
    {
        Ok(frame) => frame?,
        Err(_) => {
            let _ = logger.write("warn", "ipc.handshake_timeout", serde_json::json!({}));
            return Ok(false);
        }
    };
    let command = generated::CommandEnvelope::decode(payload.as_slice())?;
    let Some(generated::command_envelope::Command::Handshake(handshake)) = command.command else {
        reject(bridge, writer, "protocol-error").await?;
        return Ok(false);
    };
    let request = HandshakeRequest {
        protocol_major: handshake
            .protocol
            .as_ref()
            .map(|version| version.major)
            .unwrap_or_default(),
        client_id: handshake.client_id.clone(),
        client_role: handshake.client_role.clone(),
        nonce: handshake.nonce.clone(),
        proof: handshake.proof.clone(),
        capabilities: handshake.capabilities.clone(),
        peer,
    };
    match verifier.verify(&request, now_ms()) {
        Ok(verified) => {
            let _ = logger.write(
                "info",
                "ipc.client_authenticated",
                serde_json::json!({"role": verified.client_role}),
            );
        }
        Err(rejection) => {
            let _ = logger.write(
                "warn",
                "ipc.handshake_rejected",
                serde_json::json!({"reason": rejection.to_string()}),
            );
            reject(bridge, writer, "auth-rejected").await?;
            return Ok(false);
        }
    }
    transport::write_frame(writer, &bridge.ready_event().encode_to_vec()).await?;
    Ok(true)
}

async fn reject<W: AsyncWrite + Unpin>(
    bridge: &IpcBridge,
    writer: &mut W,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let event = bridge.control_event(
        "ipc.rejected",
        None,
        serde_json::to_vec(&serde_json::json!({"reason": reason}))?,
    );
    transport::write_frame(writer, &event.encode_to_vec()).await?;
    Ok(())
}

struct ChannelWriter(tokio_util::sync::PollSender<Vec<u8>>);

impl ChannelWriter {
    fn new(sender: tokio::sync::mpsc::Sender<Vec<u8>>) -> Self {
        Self(tokio_util::sync::PollSender::new(sender))
    }
}

impl AsyncWrite for ChannelWriter {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
        buffer: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.0.poll_reserve(context) {
            std::task::Poll::Ready(Ok(())) => {
                if self.0.send_item(buffer.to_vec()).is_ok() {
                    std::task::Poll::Ready(Ok(buffer.len()))
                } else {
                    std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
                }
            }
            std::task::Poll::Ready(Err(_)) => {
                std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    #[tokio::test]
    async fn creates_owner_only_socket_and_session_context() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let directory =
            std::env::temp_dir().join(format!("evohime-core-ipc-{}-{unique}", std::process::id()));
        let paths = UnixRuntimePaths::prepare(&directory).expect("private paths");
        let config = UnixPipeServerConfig::bind(&paths)
            .await
            .expect("bind server")
            .expect("first server owns endpoint");
        assert!(config.context().is_authenticated());
        assert_eq!(
            std::fs::metadata(&paths.socket)
                .expect("socket metadata")
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&paths.launch_context)
                .expect("context metadata")
                .mode()
                & 0o777,
            0o600
        );
        assert!(UnixPipeServerConfig::bind(&paths)
            .await
            .expect("second bind attempt")
            .is_none());
        assert!(
            evohime_desktop_ipc::unix_runtime::read_private_launch_context(&paths.launch_context)
                .is_ok()
        );
        drop(config);
        let _ = std::fs::remove_file(paths.socket);
        let _ = std::fs::remove_file(paths.launch_context);
        let _ = std::fs::remove_dir_all(directory);
    }
}
