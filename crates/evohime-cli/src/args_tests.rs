use crate::{parse_args, Command, ParseError};

#[test]
fn parses_bounded_run_and_modes() {
    let args = vec![
        "run".into(),
        "hello".into(),
        "--json".into(),
        "--detach".into(),
    ];
    assert_eq!(
        parse_args(&args).unwrap(),
        Command::Run {
            prompt: "hello".into(),
            workspace: std::env::current_dir().unwrap().display().to_string(),
            workflow: None,
            json: true,
            detach: true
        }
    );
}

#[test]
fn parses_read_only_run_controls() {
    for (name, expected) in [
        (
            "status",
            Command::Status {
                task_id: "run-1".into(),
                json: true,
            },
        ),
        (
            "watch",
            Command::Watch {
                task_id: "run-1".into(),
                json: true,
            },
        ),
        (
            "cancel",
            Command::Cancel {
                task_id: "run-1".into(),
                json: true,
            },
        ),
        (
            "resume",
            Command::Resume {
                task_id: "run-1".into(),
                json: true,
            },
        ),
    ] {
        let args = vec![name.into(), "run-1".into(), "--json".into()];
        assert_eq!(parse_args(&args).unwrap(), expected);
    }
}

#[test]
fn rejects_option_names_as_missing_values() {
    for args in [
        vec!["run", "prompt", "--workspace", "--json"],
        vec!["run", "--workflow", "--json"],
        vec!["run", "prompt", "--workflow", "--detach"],
    ] {
        let args = args.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(parse_args(&args), Err(ParseError::InvalidValue));
    }
}

#[test]
fn rejects_stdin_for_workflows_until_inputs_are_supported() {
    let args = ["run", "--workflow", "template-1", "--stdin"]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(parse_args(&args), Err(ParseError::InvalidValue));
}
