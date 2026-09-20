use evohime_cli::{event_matches_run, terminal_exit_code, ExitCode};

use crate::windows_endpoint::{connect, CoreClient};
use crate::windows_event_output;

pub(crate) async fn watch_events(client: &mut CoreClient, run_id: &str, json: bool) -> ExitCode {
    loop {
        match client.next().await {
            Ok(event) => {
                if !event_matches_run(&event.task_id, run_id) {
                    continue;
                }
                windows_event_output::print_event(&event, run_id, json);
                if let Some(code) = terminal_exit_code(&event.event_type) {
                    return code;
                }
            }
            Err(error) => {
                eprintln!("{error}; переподключение по cursor={}", client.sequence());
                let cursor = client.sequence();
                let mut replacement = None;
                for _ in 0..5 {
                    match connect(cursor).await {
                        Ok(next) => {
                            replacement = Some(next);
                            break;
                        }
                        Err(_) => tokio::time::sleep(std::time::Duration::from_millis(250)).await,
                    }
                }
                let Some(next) = replacement else {
                    return ExitCode::CoreUnavailable;
                };
                *client = next;
            }
        }
    }
}
