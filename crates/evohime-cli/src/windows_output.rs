use evohime_cli::{emit, CliEvent, ExitCode, CLI_SCHEMA};

pub(crate) fn print_doctor(json: bool) -> ExitCode {
    if json {
        println!(
            "{}",
            emit(&CliEvent {
                schema: CLI_SCHEMA,
                sequence: 0,
                kind: "core.ready",
                run_id: "",
                payload: serde_json::json!({"status":"ready"})
            })
        );
    } else {
        println!("Core готов");
    }
    ExitCode::Completed
}

pub(crate) fn print_run_accepted(run_id: &str, json: bool) -> ExitCode {
    if json {
        println!(
            "{}",
            emit(&CliEvent {
                schema: CLI_SCHEMA,
                sequence: 0,
                kind: "run.accepted",
                run_id,
                payload: serde_json::json!({"detached":true})
            })
        );
    } else {
        println!("{run_id}");
    }
    ExitCode::Completed
}

pub(crate) fn print_cancel_requested(task_id: &str, json: bool) -> ExitCode {
    if json {
        println!(
            "{}",
            emit(&CliEvent {
                schema: CLI_SCHEMA,
                sequence: 0,
                kind: "run.cancel_requested",
                run_id: task_id,
                payload: serde_json::json!({"accepted":true})
            })
        );
    } else {
        println!("Отмена запрошена: {task_id}");
    }
    ExitCode::Completed
}
