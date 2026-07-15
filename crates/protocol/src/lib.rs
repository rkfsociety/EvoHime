use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub id: String,
    pub tool_name: String,
    pub description: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
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
    #[serde(rename = "agent.plan.updated")]
    AgentPlanUpdated { task_id: Uuid, plan: Vec<PlanStep> },
    #[serde(rename = "tool.started")]
    ToolStarted { task_id: Uuid, tool_name: String },
    #[serde(rename = "tool.output")]
    ToolOutput {
        task_id: Uuid,
        tool_name: String,
        output: String,
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
    },
    #[serde(rename = "task.failed")]
    TaskFailed { task_id: Uuid, error: String },
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
    },
    #[serde(rename = "approval.required")]
    ApprovalRequired {
        approval_id: Uuid,
        task_id: Uuid,
        tool_name: String,
        permission: String,
        scope: String,
        created_at: DateTime<Utc>,
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
    },
    #[serde(rename = "task.cancel")]
    TaskCancel { task_id: Uuid },
    #[serde(rename = "task.resume")]
    TaskResume { task_id: Uuid },
    #[serde(rename = "task.retry")]
    TaskRetry { task_id: Uuid },
    #[serde(rename = "approval.granted")]
    ApprovalGranted { approval_id: Uuid },
    #[serde(rename = "approval.denied")]
    ApprovalDenied { approval_id: Uuid },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        };

        let json = serde_json::to_value(&event).expect("event serializes");
        assert_eq!(json["type"], "task.completed");
        assert_eq!(json["final_message"], "done");
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
        };

        let json = serde_json::to_string(&command).expect("command serializes");
        let decoded: ClientCommand = serde_json::from_str(&json).expect("command deserializes");

        match decoded {
            ClientCommand::UserMessage {
                content,
                model_route,
                model,
            } => {
                assert_eq!(content, "hello");
                assert_eq!(model_route.as_deref(), Some("planner"));
                assert_eq!(model.as_deref(), Some("deepseek:free"));
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
            tool_name: "shell.execute".into(),
            permission: "shell_execute".into(),
            scope: "workspace".into(),
            created_at: Utc::now(),
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["type"], "approval.required");
        for command in [
            ClientCommand::ApprovalGranted {
                approval_id: Uuid::nil(),
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
}
