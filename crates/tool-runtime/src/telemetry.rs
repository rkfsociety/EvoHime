use serde::{Deserialize, Serialize};

/// Maximum number of lifecycle events retained in one buffer.
pub const MAX_EVENTS: usize = 2048;
/// Maximum length of a recorded error category.
pub const MAX_ERROR: usize = 256;

/// Lifecycle stages recorded for a tool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecycle {
    /// The tool call was accepted and execution setup began.
    Started,
    /// Execution is paused while approval is requested.
    WaitingApproval,
    /// The requested operation was approved.
    Approved,
    /// The requested operation was rejected.
    Rejected,
    /// The tool effect was dispatched.
    Dispatched,
    /// The tool call completed successfully.
    Succeeded,
    /// The tool call failed.
    Failed,
    /// The call was cancelled before completion.
    Cancelled,
    /// The call exceeded its deadline.
    TimedOut,
    /// The runtime scheduled another attempt.
    Retried,
    /// Policy denied the operation before dispatch.
    PolicyDenied,
}

/// Bounded telemetry captured for one tool lifecycle stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTelemetryEvent {
    /// Task that owns the call.
    pub task_id: String,
    /// Workflow run containing the call, if applicable.
    pub run_id: String,
    /// Stable identifier for this tool invocation.
    pub tool_call_id: String,
    /// One-based attempt number.
    pub attempt: u32,
    /// Digest of the tool manifest used for the call.
    pub manifest_hash: String,
    /// Lifecycle stage represented by this event.
    pub phase: ToolLifecycle,
    /// Elapsed time in milliseconds for this stage or call.
    pub duration_ms: u64,
    /// Token usage attributed to this call.
    pub tokens: u64,
    /// Cost in millionths of the currency unit.
    pub cost_micros: u64,
    /// Number of output bytes produced.
    pub output_bytes: u64,
    /// Remaining budget after the recorded stage.
    pub budget_remaining: u64,
    /// Retry count observed for the call.
    pub retry_count: u32,
    /// Optional bounded category for an error.
    pub error_class: Option<String>,
}

/// In-memory bounded buffer for recent tool telemetry events.
#[derive(Debug, Default, Clone)]
pub struct TelemetryBuffer {
    events: Vec<ToolTelemetryEvent>,
}

impl TelemetryBuffer {
    /// Records an event, truncating error labels and evicting the oldest event at capacity.
    pub fn record(&mut self, mut event: ToolTelemetryEvent) {
        if let Some(error) = &mut event.error_class {
            error.truncate(MAX_ERROR);
        }
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }
    /// Returns the currently retained events in insertion order.
    pub fn events(&self) -> &[ToolTelemetryEvent] {
        &self.events
    }
    /// Serializes retained events as newline-delimited JSON.
    pub fn export_jsonl(&self) -> String {
        let mut output = String::new();
        for event in &self.events {
            let Ok(json) = serde_json::to_string(event) else {
                continue;
            };
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&json);
        }
        output
    }
    /// Aggregates call, usage, retry, and failure totals for one run.
    pub fn aggregate(&self, run_id: &str) -> TelemetrySummary {
        self.events.iter().filter(|e| e.run_id == run_id).fold(
            TelemetrySummary::default(),
            |mut s, e| {
                s.calls += u64::from(matches!(e.phase, ToolLifecycle::Dispatched));
                s.tokens += e.tokens;
                s.cost_micros += e.cost_micros;
                s.duration_ms += e.duration_ms;
                s.retries += u64::from(matches!(e.phase, ToolLifecycle::Retried));
                s.failures += u64::from(matches!(e.phase, ToolLifecycle::Failed));
                s
            },
        )
    }
}

/// Aggregate usage totals for a workflow run.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetrySummary {
    /// Number of dispatched tool calls.
    pub calls: u64,
    /// Total token usage.
    pub tokens: u64,
    /// Total cost in millionths of the currency unit.
    pub cost_micros: u64,
    /// Total elapsed milliseconds.
    pub duration_ms: u64,
    /// Number of retry lifecycle events.
    pub retries: u64,
    /// Number of failed lifecycle events.
    pub failures: u64,
}
