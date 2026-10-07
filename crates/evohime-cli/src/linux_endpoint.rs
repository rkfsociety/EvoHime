use evohime_cli::protocol::CoreClient as ProtocolClient;
use evohime_desktop_ipc::unix_runtime::{read_private_launch_context, UnixRuntimePaths};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::net::UnixStream;

pub(crate) type CoreClient = ProtocolClient<UnixStream>;

const CORE_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(750);

pub(crate) async fn connect(after_sequence: u64) -> Result<CoreClient, String> {
    let paths = UnixRuntimePaths::current()
        .map_err(|_| "core_unavailable: Linux runtime directory is unavailable".to_string())?;
    if let Some(client) = try_connect(&paths, after_sequence).await? {
        return Ok(client);
    }

    let mut child = spawn_core()?;
    let deadline = Instant::now() + CORE_STARTUP_TIMEOUT;
    loop {
        if let Some(client) = try_connect(&paths, after_sequence).await? {
            drop(child);
            return Ok(client);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "core_unavailable: could not inspect Core startup".to_string())?
        {
            if !status.success() {
                return Err("core_unavailable: evohime-core exited during startup".to_string());
            }
        }
        if Instant::now() >= deadline {
            return Err("core_unavailable: Core startup timed out".to_string());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn try_connect(
    paths: &UnixRuntimePaths,
    after_sequence: u64,
) -> Result<Option<CoreClient>, String> {
    let context = match read_private_launch_context(&paths.launch_context) {
        Ok(context) => context,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err("core_unavailable: Linux launch context is invalid or not private".into())
        }
    };
    let stream =
        match tokio::time::timeout(CONNECT_TIMEOUT, UnixStream::connect(&paths.socket)).await {
            Ok(Ok(stream)) => stream,
            Ok(Err(_)) | Err(_) => return Ok(None),
        };
    match tokio::time::timeout(
        CONNECT_TIMEOUT,
        ProtocolClient::connect(stream, &context, after_sequence),
    )
    .await
    {
        Ok(Ok(client)) => Ok(Some(client)),
        Ok(Err(_)) | Err(_) => Ok(None),
    }
}

fn spawn_core() -> Result<Child, String> {
    let adjacent = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("evohime-core")))
        .filter(|path| path.is_file());
    let mut command = adjacent.map_or_else(|| Command::new("evohime-core"), Command::new);
    use std::os::unix::process::CommandExt;
    command
        .arg("--cli-server")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|_| {
            "core_unavailable: evohime-core was not found; build both evohime-cli and evohime-core or add Core to PATH".to_string()
        })
}
