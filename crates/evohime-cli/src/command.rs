#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run {
        prompt: String,
        workspace: String,
        workflow: Option<String>,
        json: bool,
        detach: bool,
    },
    Status {
        task_id: String,
        json: bool,
    },
    Watch {
        task_id: String,
        json: bool,
    },
    Cancel {
        task_id: String,
        json: bool,
    },
    Resume {
        task_id: String,
        json: bool,
    },
    Doctor {
        json: bool,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("usage: eva run [<prompt>] [--workflow <id>] [--workspace <path>] [--stdin] [--json] [--detach]")]
    Usage,
    #[error("unknown command or option")]
    UnknownOption,
    #[error("value is missing or exceeds the CLI bound")]
    InvalidValue,
}
