use evohime_cli::{emit, redact_payload, CliEvent, CLI_SCHEMA};
use evohime_desktop_ipc::generated;

pub(crate) fn print_event(event: &generated::EventEnvelope, run_id: &str, json: bool) {
    let payload = redact_payload(&event.payload);
    if json {
        println!(
            "{}",
            emit(&CliEvent {
                schema: CLI_SCHEMA,
                sequence: event.sequence_id,
                kind: &event.event_type,
                run_id,
                payload
            })
        );
    } else {
        println!("{} {}", event.event_type, run_id);
    }
}
