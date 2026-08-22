use serde::{Deserialize, Serialize};

pub const MAX_EVENTS: usize = 2048;
pub const MAX_ERROR: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolLifecycle {
    Started,
    WaitingApproval,
    Approved,
    Rejected,
    Dispatched,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Retried,
    PolicyDenied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTelemetryEvent {
    pub task_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub attempt: u32,
    pub manifest_hash: String,
    pub phase: ToolLifecycle,
    pub duration_ms: u64,
    pub tokens: u64,
    pub cost_micros: u64,
    pub output_bytes: u64,
    pub budget_remaining: u64,
    pub retry_count: u32,
    pub error_class: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct TelemetryBuffer {
    events: Vec<ToolTelemetryEvent>,
}

impl TelemetryBuffer {
    pub fn record(&mut self, mut event: ToolTelemetryEvent) {
        if let Some(error) = &mut event.error_class {
            error.truncate(MAX_ERROR);
        }
        if self.events.len() >= MAX_EVENTS {
            self.events.remove(0);
        }
        self.events.push(event);
    }
    pub fn events(&self) -> &[ToolTelemetryEvent] {
        &self.events
    }
    pub fn export_jsonl(&self) -> String {
        self.events
            .iter()
            .filter_map(|e| serde_json::to_string(e).ok())
            .collect::<Vec<_>>()
            .join("\n")
    }
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

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetrySummary {
    pub calls: u64,
    pub tokens: u64,
    pub cost_micros: u64,
    pub duration_ms: u64,
    pub retries: u64,
    pub failures: u64,
}
