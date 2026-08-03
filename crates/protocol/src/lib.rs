use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod planning;

pub use planning::{PlanCandidate, PlanGenerationResponse, ScoreBreakdown};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalReview {
    UnifiedDiff { path: String, diff: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionType {
    PostToolExecution,
    PlanRevision,
    ErrorRecovery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReflectionAction {
    Proceed,
    AskUser,
    RetryTool,
    RevisePlan,
    Escalate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    pub pattern_id: String,
    pub pattern_name: String,
    pub confidence: f64,
    pub source: String, // "experience_memory" | "heuristic"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionAnalysis {
    pub success_score: f64,
    pub error_patterns: Vec<ErrorPattern>,
    pub confidence: f64,
    pub reasoning: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "session.created")]
    SessionCreated {
        session_id: Uuid,
        created_at: DateTime<Utc>,
    },
    #[serde(rename = "task.started")]
    TaskStarted {
        task_id: Uuid,
        session_id: Uuid,
        user_message: String,
        created_at: DateTime<Utc>,
    },
    #[serde(rename = "agent.message.delta")]
    AgentMessageDelta { task_id: Uuid, delta: String },
    #[serde(rename = "agent.thinking")]
    AgentThinking {
        task_id: Uuid,
        thinking: String,
        created_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        correlation_id: Option<String>,
        llm_request_id: String,
    },
    #[serde(rename = "agent.status")]
    AgentStatus { task_id: Uuid, phase: String },
    #[serde(rename = "agent.plan.updated")]
    AgentPlanUpdated { task_id: Uuid, plan: Vec<PlanStep> },
    #[serde(rename = "agent.plan")]
    AgentPlan {
        task_id: Uuid,
        candidates: Vec<PlanCandidate>,
        chosen_plan_id: String,
        reasoning: String,
    },
    #[serde(rename = "tool.started")]
    ToolStarted { task_id: Uuid, tool_name: String },
    #[serde(rename = "tool.output")]
    ToolOutput {
        task_id: Uuid,
        tool_name: String,
        output: String,
    },
    #[serde(rename = "tool.output.delta")]
    ToolOutputDelta {
        task_id: Uuid,
        tool_name: String,
        /// `stdout` | `stderr` | `log`
        stream: String,
        delta: String,
    },
    #[serde(rename = "tool.completed")]
    ToolCompleted {
        task_id: Uuid,
        tool_name: String,
        success: bool,
    },
    #[serde(rename = "task.completed")]
    TaskCompleted {
        task_id: Uuid,
        final_message: String,
        completed_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "task.failed")]
    TaskFailed {
        task_id: Uuid,
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "file.changed")]
    FileChanged {
        path: String,
        change: String,
        created_at: DateTime<Utc>,
    },
    #[serde(rename = "git.diff.changed")]
    GitDiffChanged {
        status: String,
        diff: String,
        created_at: DateTime<Utc>,
    },
    #[serde(rename = "task.status.changed")]
    TaskStatusChanged { task_id: Uuid, status: String },
    #[serde(rename = "task.step.changed")]
    TaskStepChanged {
        task_id: Uuid,
        step_id: Uuid,
        status: String,
        tool_name: String,
    },
    #[serde(rename = "action.logged")]
    ActionLogged {
        task_id: Uuid,
        action: String,
        detail: String,
        created_at: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        correlation_id: Option<Uuid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
    },
    #[serde(rename = "approval.required")]
    ApprovalRequired {
        approval_id: Uuid,
        task_id: Uuid,
        tool_name: String,
        permission: String,
        scope: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        review: Option<ApprovalReview>,
        created_at: DateTime<Utc>,
    },
    #[serde(rename = "memory.proposed")]
    MemoryProposed {
        memory_id: Uuid,
        task_id: Uuid,
        scope: String,
        kind: String,
        content: String,
        confidence: f64,
        status: String,
    },
    #[serde(rename = "memory.ask")]
    MemoryAsk {
        memory_id: Uuid,
        task_id: Uuid,
        scope: String,
        kind: String,
        content: String,
        confidence: f64,
        status: String,
        reason: String,
    },
    #[serde(rename = "memory.accepted")]
    MemoryAccepted { memory_id: Uuid, task_id: Uuid },
    #[serde(rename = "memory.rejected")]
    MemoryRejected { memory_id: Uuid, task_id: Uuid },
    #[serde(rename = "memory.used")]
    MemoryUsed {
        memory_id: Uuid,
        task_id: Uuid,
        signal: String,
        confidence: f64,
    },
    #[serde(rename = "agent.reflection")]
    AgentReflection {
        task_id: Uuid,
        timestamp: DateTime<Utc>,
        reflection_type: ReflectionType,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<Uuid>,
        analysis: ReflectionAnalysis,
        action: ReflectionAction,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        recommendation: Option<String>,
    },
    #[serde(rename = "agent.confidence")]
    AgentConfidence {
        task_id: Uuid,
        timestamp: DateTime<Utc>,
        #[serde(default)]
        confidence_version: String,
        confidence_score: f32,
        risk_level: String,
        breakdown: serde_json::Value,
        reliability: serde_json::Value,
        missing_signals: Vec<String>,
        recommendation: String, // "proceed" | "ask" | "require_approval"
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCommand {
    #[serde(rename = "user.message")]
    UserMessage {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_route: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace_path: Option<String>,
    },
    #[serde(rename = "task.cancel")]
    TaskCancel { task_id: Uuid },
    #[serde(rename = "task.resume")]
    TaskResume { task_id: Uuid },
    #[serde(rename = "task.retry")]
    TaskRetry { task_id: Uuid },
    #[serde(rename = "task.plan.approve")]
    TaskPlanApprove { task_id: Uuid, plan: Vec<PlanStep> },
    #[serde(rename = "task.plan.reject")]
    TaskPlanReject { task_id: Uuid },
    #[serde(rename = "approval.granted")]
    ApprovalGranted {
        approval_id: Uuid,
        #[serde(default)]
        remember_path: bool,
    },
    #[serde(rename = "approval.denied")]
    ApprovalDenied { approval_id: Uuid },
    #[serde(rename = "memory.accept")]
    MemoryAccept { memory_id: Uuid },
    #[serde(rename = "memory.reject")]
    MemoryReject { memory_id: Uuid },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Running,
    Cancelling,
    Cancelled,
    Paused,
    Failed,
    Retrying,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBootstrap {
    pub session_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub events: Vec<HistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub sequence: i64,
    pub created_at: DateTime<Utc>,
    pub event: ServerEvent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_event_tags() {
        let event = ServerEvent::TaskCompleted {
            task_id: Uuid::nil(),
            final_message: "done".to_string(),
            completed_at: Utc::now(),
            duration_ms: Some(1250),
        };

        let json = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(json["type"], "task.completed");
        assert_eq!(json["final_message"], "done");
        assert_eq!(json["duration_ms"], 1250);
    }

    #[test]
    fn action_timeline_round_trips_optional_telemetry() {
        let correlation_id = Uuid::new_v4();
        let event = ServerEvent::ActionLogged {
            task_id: Uuid::nil(),
            action: "task.retry".into(),
            detail: "retrying".into(),
            created_at: Utc::now(),
            correlation_id: Some(correlation_id),
            duration_ms: Some(42),
        };
        let decoded: ServerEvent =
            serde_json::from_value(serde_json::to_value(event).unwrap()).unwrap();
        match decoded {
            ServerEvent::ActionLogged {
                correlation_id: decoded_id,
                duration_ms,
                ..
            } => {
                assert_eq!(decoded_id, Some(correlation_id));
                assert_eq!(duration_ms, Some(42));
            }
            _ => panic!("unexpected event variant"),
        }
    }

    #[test]
    fn serializes_react_status_without_reasoning_payload() {
        let event = ServerEvent::AgentStatus {
            task_id: Uuid::nil(),
            phase: "selecting_action".into(),
        };
        let json = serde_json::to_value(event).expect("status serializes");
        assert_eq!(json["type"], "agent.status");
        assert_eq!(json["phase"], "selecting_action");
        assert!(json.get("rationale").is_none());
    }

    #[test]
    fn serializes_structured_plan_steps() {
        let event = ServerEvent::AgentPlanUpdated {
            task_id: Uuid::nil(),
            plan: vec![PlanStep {
                id: "step-1".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read the workspace context".to_string(),
                depends_on: vec!["step-0".to_string()],
            }],
        };

        let json = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(json["type"], "agent.plan.updated");
        assert_eq!(json["plan"][0]["tool_name"], "filesystem.read");
        assert_eq!(json["plan"][0]["depends_on"][0], "step-0");
    }

    #[test]
    fn round_trips_client_command() {
        let command = ClientCommand::UserMessage {
            content: "hello".to_string(),
            model_route: Some("planner".to_string()),
            model: Some("deepseek:free".to_string()),
            workspace_path: Some("F:/github/test".to_string()),
        };

        let json = serde_json::to_string(&command).expect("command serializes");
        let decoded: ClientCommand = serde_json::from_str(&json).expect("command deserializes");

        match decoded {
            ClientCommand::UserMessage {
                content,
                model_route,
                model,
                workspace_path,
            } => {
                assert_eq!(content, "hello");
                assert_eq!(model_route.as_deref(), Some("planner"));
                assert_eq!(model.as_deref(), Some("deepseek:free"));
                assert_eq!(workspace_path.as_deref(), Some("F:/github/test"));
            }
            _ => panic!("unexpected command variant"),
        }
    }

    #[test]
    fn serializes_file_changed_event() {
        let event = ServerEvent::FileChanged {
            path: "src/main.rs".to_string(),
            change: "updated".to_string(),
            created_at: Utc::now(),
        };

        let json = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(json["type"], "file.changed");
        assert_eq!(json["path"], "src/main.rs");
    }

    #[test]
    fn round_trips_approval_event_and_commands() {
        let event = ServerEvent::ApprovalRequired {
            approval_id: Uuid::nil(),
            task_id: Uuid::nil(),
            tool_name: "filesystem.patch".into(),
            permission: "filesystem_write".into(),
            scope: "src/lib.rs".into(),
            review: Some(ApprovalReview::UnifiedDiff {
                path: "src/lib.rs".into(),
                diff: "@@ -1 +1 @@\n-old\n+new".into(),
            }),
            created_at: Utc::now(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "approval.required");
        assert_eq!(json["review"]["kind"], "unified_diff");
        assert_eq!(json["review"]["path"], "src/lib.rs");
        assert_eq!(json["review"]["diff"], "@@ -1 +1 @@\n-old\n+new");
        let decoded: ServerEvent = serde_json::from_value(json).unwrap();
        assert!(matches!(
            decoded,
            ServerEvent::ApprovalRequired {
                review: Some(ApprovalReview::UnifiedDiff { .. }),
                ..
            }
        ));

        let ordinary = ServerEvent::ApprovalRequired {
            approval_id: Uuid::nil(),
            task_id: Uuid::nil(),
            tool_name: "shell.execute".into(),
            permission: "shell_execute".into(),
            scope: "workspace".into(),
            review: None,
            created_at: Utc::now(),
        };
        let ordinary_json = serde_json::to_value(ordinary).unwrap();
        assert!(ordinary_json.get("review").is_none());

        for command in [
            ClientCommand::ApprovalGranted {
                approval_id: Uuid::nil(),
                remember_path: false,
            },
            ClientCommand::ApprovalGranted {
                approval_id: Uuid::nil(),
                remember_path: true,
            },
            ClientCommand::ApprovalDenied {
                approval_id: Uuid::nil(),
            },
        ] {
            let decoded: ClientCommand =
                serde_json::from_value(serde_json::to_value(command).unwrap()).unwrap();
            assert!(matches!(
                decoded,
                ClientCommand::ApprovalGranted { .. } | ClientCommand::ApprovalDenied { .. }
            ));
        }
    }

    #[test]
    fn round_trips_memory_events_and_commands() {
        let ask = ServerEvent::MemoryAsk {
            memory_id: Uuid::nil(),
            task_id: Uuid::nil(),
            scope: "workspace".into(),
            kind: "fact".into(),
            content: "prefer worktrees".into(),
            confidence: 0.55,
            status: "candidate".into(),
            reason: "low confidence".into(),
        };
        let json = serde_json::to_value(&ask).unwrap();
        assert_eq!(json["type"], "memory.ask");
        assert_eq!(json["reason"], "low confidence");

        let used = ServerEvent::MemoryUsed {
            memory_id: Uuid::nil(),
            task_id: Uuid::nil(),
            signal: "helpful".into(),
            confidence: 0.73,
        };
        let used_json = serde_json::to_value(&used).unwrap();
        assert_eq!(used_json["type"], "memory.used");
        assert_eq!(used_json["signal"], "helpful");

        for command in [
            ClientCommand::MemoryAccept {
                memory_id: Uuid::nil(),
            },
            ClientCommand::MemoryReject {
                memory_id: Uuid::nil(),
            },
        ] {
            let decoded: ClientCommand =
                serde_json::from_value(serde_json::to_value(command).unwrap()).unwrap();
            assert!(matches!(
                decoded,
                ClientCommand::MemoryAccept { .. } | ClientCommand::MemoryReject { .. }
            ));
        }
    }

    #[test]
    fn serializes_plan_approval_command() {
        let command = ClientCommand::TaskPlanApprove {
            task_id: Uuid::nil(),
            plan: vec![PlanStep {
                id: "step-1".to_string(),
                tool_name: "filesystem.read".to_string(),
                description: "Read context".to_string(),
                depends_on: vec![],
            }],
        };

        let json = serde_json::to_value(&command).expect("command serializes");
        assert_eq!(json["type"], "task.plan.approve");
        assert_eq!(json["plan"][0]["id"], "step-1");
        let decoded: ClientCommand = serde_json::from_value(json).expect("command deserializes");
        assert!(matches!(
            decoded,
            ClientCommand::TaskPlanApprove { task_id, plan }
                if task_id == Uuid::nil() && plan[0].id == "step-1"
        ));
    }

    #[test]
    fn serializes_agent_plan_event() {
        let event = ServerEvent::AgentPlan {
            task_id: Uuid::nil(),
            candidates: vec![
                PlanCandidate {
                    id: "plan-1".to_string(),
                    description: "Parallel execution".to_string(),
                    confidence: 0.85,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.9,
                        tool_success_rate: 0.85,
                        complexity_penalty: 0.1,
                        feedback_adjustment: 0.0,
                        final_score: 0.8,
                    },
                },
                PlanCandidate {
                    id: "plan-2".to_string(),
                    description: "Sequential execution".to_string(),
                    confidence: 0.65,
                    score_breakdown: ScoreBreakdown {
                        similarity_score: 0.7,
                        tool_success_rate: 0.75,
                        complexity_penalty: 0.05,
                        feedback_adjustment: 0.0,
                        final_score: 0.67,
                    },
                },
            ],
            chosen_plan_id: "plan-1".to_string(),
            reasoning: "First plan has higher confidence and better scores".to_string(),
        };

        let json = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(json["type"], "agent.plan");
        assert_eq!(json["chosen_plan_id"], "plan-1");
        assert_eq!(json["candidates"].as_array().unwrap().len(), 2);
        assert_eq!(json["candidates"][0]["id"], "plan-1");
        assert!(json["candidates"][0]["score_breakdown"]["similarity_score"].is_number());

        let decoded: ServerEvent = serde_json::from_value(json).expect("event deserializes");
        assert!(matches!(
            decoded,
            ServerEvent::AgentPlan {
                task_id,
                chosen_plan_id,
                candidates,
                reasoning
            }
                if task_id == Uuid::nil()
                    && chosen_plan_id == "plan-1"
                    && candidates.len() == 2
                    && reasoning.contains("confidence")
        ));
    }
}
