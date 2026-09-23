/// Typed headless CLI operation returned by [`crate::parse_args`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Starts an agent task.
    Run {
        /// Prompt to execute.
        prompt: String,
        /// Workspace path selected for the task.
        workspace: String,
        /// Optional named workflow to execute.
        workflow: Option<String>,
        /// Emit structured JSON events.
        json: bool,
        /// Return after the task has been detached.
        detach: bool,
    },
    /// Reads the terminal status of a task.
    Status {
        /// Task identifier.
        task_id: String,
        /// Emit structured JSON output.
        json: bool,
    },
    /// Streams events for a task until a terminal event arrives.
    Watch {
        /// Task identifier.
        task_id: String,
        /// Emit structured JSON events.
        json: bool,
    },
    /// Requests cancellation of a running task.
    Cancel {
        /// Task identifier.
        task_id: String,
        /// Emit structured JSON output.
        json: bool,
    },
    /// Resumes a task with an unknown external outcome.
    Resume {
        /// Task identifier.
        task_id: String,
        /// Emit structured JSON events.
        json: bool,
    },
    /// Reports local Core/client diagnostics.
    Doctor {
        /// Emit structured JSON output.
        json: bool,
    },
}

/// Argument parsing failures safe to show as CLI usage diagnostics.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    /// Arguments do not match a supported command form.
    #[error("usage: eva run [<prompt>] [--workflow <id>] [--workspace <path>] [--stdin] [--json] [--detach]")]
    Usage,
    /// Command or option name is not supported.
    #[error("unknown command or option")]
    UnknownOption,
    /// A required option value is absent or violates a configured bound.
    #[error("value is missing or exceeds the CLI bound")]
    InvalidValue,
}
