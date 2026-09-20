use evohime_cli::protocol::CoreClient as ProtocolClient;
use std::path::PathBuf;
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

pub(crate) type CoreClient = ProtocolClient<NamedPipeClient>;

pub(crate) async fn connect(after_sequence: u64) -> Result<CoreClient, String> {
    let context_path = std::env::var_os("EVOHIME_LAUNCH_CONTEXT")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|value| PathBuf::from(value).join("EvoHime/runtime/session.json"))
        })
        .ok_or_else(|| "core_unavailable: launch context is not configured".to_string())?;
    let context = evohime_desktop_ipc::session::read_launch_context(&context_path)
        .map_err(|_| "core_unavailable: invalid launch context".to_string())?;
    let pipe = ClientOptions::new()
        .open(&context.pipe_name)
        .map_err(|_| "core_unavailable: named pipe is unavailable".to_string())?;
    ProtocolClient::connect(pipe, &context, after_sequence).await
}
