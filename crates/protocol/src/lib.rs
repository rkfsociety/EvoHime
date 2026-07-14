use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    AgentMessageDelta {
        task_id: Uuid,
        delta: String,
    },
    #[serde(rename = "agent.plan.updated")]
    AgentPlanUpdated {
        task_id: Uuid,
        plan: Vec<String>,
    },
    #[serde(rename = "tool.started")]
    ToolStarted {
        task_id: Uuid,
        tool_name: String,
    },
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
    TaskFailed {
        task_id: Uuid,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCommand {
    #[serde(rename = "user.message")]
    UserMessage { content: String },
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
    fn round_trips_client_command() {
        let command = ClientCommand::UserMessage {
            content: "hello".to_string(),
        };

        let json = serde_json::to_string(&command).expect("command serializes");
        let decoded: ClientCommand = serde_json::from_str(&json).expect("command deserializes");

        match decoded {
            ClientCommand::UserMessage { content } => assert_eq!(content, "hello"),
        }
    }
}
