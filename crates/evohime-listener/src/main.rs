use evohime_listener::{backoff, data_dir, ListenerRuntime, NullEngine};
use evohime_listener_contract::{AmbientPolicy, ListeningState};
use evohime_listener_ipc::{envelope, generated, read_frame, write_frame};
use tokio::time::sleep;

#[cfg(windows)]
#[tokio::main]
async fn main() {
    evohime_listener::harden_process();
    let (tx, _rx) = tokio::sync::watch::channel(ListeningState::PausedByPolicy);
    let mut runtime = ListenerRuntime::new(
        AmbientPolicy {
            paused: true,
            ..Default::default()
        },
        NullEngine,
        tx,
    );
    let mut attempt = 0;
    loop {
        match run_connection(&mut runtime).await {
            Ok(()) => attempt = 0,
            Err(error) => {
                log_error(&error.to_string());
                sleep(backoff(attempt)).await;
                attempt = attempt.saturating_add(1);
            }
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("evohime-listener is a Windows capture process");
}

#[cfg(windows)]
async fn run_connection(
    runtime: &mut ListenerRuntime<NullEngine>,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let context_path = std::env::var("EVOHIME_LAUNCH_CONTEXT")?;
    let context =
        evohime_desktop_ipc::session::read_launch_context(std::path::Path::new(&context_path))?;
    let pipe = std::env::var("EVOHIME_LISTENER_PIPE")?;
    let mut stream = ClientOptions::new().open(&pipe)?;
    let hello = generated::Hello {
        protocol_major: 1,
        client_id: format!("listener-{}", std::process::id()),
        role: "listener".into(),
    };
    let client_id = hello.client_id.clone();
    write_frame(
        &mut stream,
        &envelope(generated::envelope::Payload::Hello(hello)),
    )
    .await?;
    loop {
        let message = read_frame(&mut stream).await?;
        match message.payload {
            Some(generated::envelope::Payload::Handshake(challenge)) => {
                let proof = context
                    .secret
                    .proof("listener", &client_id, &challenge.nonce);
                write_frame(
                    &mut stream,
                    &envelope(generated::envelope::Payload::Handshake(
                        generated::Handshake {
                            nonce: challenge.nonce,
                            proof,
                        },
                    )),
                )
                .await?;
            }
            Some(generated::envelope::Payload::Policy(policy)) => {
                runtime.policy.paused = policy.paused;
                runtime.policy.process_blocklist = policy.process_blocklist;
                runtime.policy.window_title_blocklist = policy.window_title_blocklist;
            }
            Some(generated::envelope::Payload::Command(command)) => {
                if matches!(
                    command.command,
                    Some(generated::local_command::Command::ResetBuffers(true))
                ) {
                    runtime.reset_buffers();
                }
            }
            _ => {}
        }
    }
}

fn log_error(error: &str) {
    let path = data_dir().join("logs").join("listener.jsonl");
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    let line = serde_json::json!({"event":"listener.connection_failed","code":"core_unavailable","error":error});
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, format!("{}\n", line).as_bytes())
        });
}
