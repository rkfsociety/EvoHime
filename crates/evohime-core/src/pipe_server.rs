//! Windows named-pipe server for `desktop-ipc-v1`.
//!
//! Core owns the endpoint: it creates the pipe with an owner-only DACL,
//! issues a single-use nonce to every connecting client and refuses to serve
//! commands until the client answers with a valid proof from the supervisor's
//! launch context. The supervisor manages Core's lifecycle but never
//! substitutes the endpoint, and the client's identity is taken from the
//! operating system rather than from what the client claims.

use std::os::windows::io::AsRawHandle;
use std::sync::Arc;

use evohime_desktop_ipc::session::{
    HandshakeRejection, HandshakeRequest, HandshakeVerifier, LaunchContext, PeerIdentity,
    DEFAULT_NONCE_TTL_MS,
};
use evohime_desktop_ipc::windows_security::{current_user_sid, peer_identity, PipeSecurity};
use evohime_desktop_ipc::{generated, transport};
use prost::Message;
use tokio::io::split;
use tokio::net::windows::named_pipe::ServerOptions;

use crate::{IpcBridge, StructuredLogger};

/// How long a client has to answer the nonce before the connection is closed.
const HANDSHAKE_TIMEOUT_MS: u64 = 10_000;

pub struct PipeServerConfig {
    pub context: LaunchContext,
    /// When false, a client that skips authentication is still served and the
    /// connection is logged as unauthenticated. This keeps the WinUI
    /// compatibility shell working until it is retired; the packaged
    /// supervisor always enables enforcement.
    pub enforce_authentication: bool,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

pub async fn run_windows_pipe(
    config: PipeServerConfig,
    bridge: IpcBridge,
    logger: Arc<StructuredLogger>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pipe_name = config.context.pipe_name.clone();
    let enforce = config.enforce_authentication;
    let mut verifier = HandshakeVerifier::new(config.context, DEFAULT_NONCE_TTL_MS)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let user_sid = current_user_sid()?;

    let _ = logger.write(
        "info",
        "ipc.listening",
        serde_json::json!({
            "authenticated": enforce,
            "acl": "owner-only",
        }),
    );

    loop {
        let mut security = PipeSecurity::owner_only(&user_sid)?;
        let server = unsafe {
            ServerOptions::new().create_with_security_attributes_raw(&pipe_name, security.as_raw())
        }?;
        server.connect().await?;

        // Kept before the split: the identity of the client is read from this
        // handle after the first frame arrives, because Windows only allows
        // impersonating a pipe client once the server has read from it.
        let handle = server.as_raw_handle();

        let (mut reader, mut writer) = split(server);
        match authenticate(
            &mut verifier,
            &bridge,
            &logger,
            handle,
            enforce,
            &mut reader,
            &mut writer,
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

        loop {
            if let Err(error) = bridge.process_once(&mut reader, &mut writer).await {
                let _ = logger.write(
                    "warn",
                    "ipc.connection_closed",
                    serde_json::json!({"error": error.to_string()}),
                );
                break;
            }
        }
    }
}

/// Runs the challenge/response exchange for one connection.
///
/// Returns `Ok(true)` when the connection may proceed to the command loop and
/// `Ok(false)` when it was refused and the listener should wait for the next
/// client.
async fn authenticate<R, W>(
    verifier: &mut HandshakeVerifier,
    bridge: &IpcBridge,
    logger: &Arc<StructuredLogger>,
    handle: std::os::windows::io::RawHandle,
    enforce: bool,
    reader: &mut R,
    writer: &mut W,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
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
    // The client has now written to the pipe, which is what Windows requires
    // before its identity can be impersonated and read.
    let peer = match peer_identity(handle) {
        Ok(peer) => peer,
        Err(error) => {
            let _ = logger.write(
                "warn",
                "ipc.peer_identity_failed",
                serde_json::json!({"error": error.to_string()}),
            );
            PeerIdentity::default()
        }
    };

    let command = generated::CommandEnvelope::decode(payload.as_slice())?;
    let Some(generated::command_envelope::Command::Handshake(handshake)) = command.command else {
        // The first frame must be a handshake; anything else is a protocol
        // error and never mutates state.
        let _ = logger.write("warn", "ipc.handshake_expected", serde_json::json!({}));
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
            let unauthenticated_legacy = !enforce
                && matches!(
                    rejection,
                    HandshakeRejection::ProofMismatch
                        | HandshakeRejection::NonceMismatch
                        | HandshakeRejection::UnknownRole
                );
            if !unauthenticated_legacy {
                let _ = logger.write(
                    "warn",
                    "ipc.handshake_rejected",
                    serde_json::json!({"reason": rejection.to_string()}),
                );
                reject(bridge, writer, "auth-rejected").await?;
                return Ok(false);
            }
            let _ = logger.write(
                "warn",
                "ipc.client_unauthenticated",
                serde_json::json!({"reason": rejection.to_string()}),
            );
        }
    }

    transport::write_frame(writer, &bridge.ready_event().encode_to_vec()).await?;
    Ok(true)
}

async fn reject<W: tokio::io::AsyncWrite + Unpin>(
    bridge: &IpcBridge,
    writer: &mut W,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // The reason is a bounded category, never the expected nonce or secret.
    let event = bridge.control_event(
        "ipc.rejected",
        None,
        serde_json::to_vec(&serde_json::json!({"reason": reason}))?,
    );
    transport::write_frame(writer, &event.encode_to_vec()).await?;
    Ok(())
}
