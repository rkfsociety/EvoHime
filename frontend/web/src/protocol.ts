export type SessionCreatedEvent = {
  type: "session.created";
  session_id: string;
  created_at: string;
};

export type TaskStartedEvent = {
  type: "task.started";
  task_id: string;
  session_id: string;
  user_message: string;
  created_at: string;
};

export type AgentMessageDeltaEvent = {
  type: "agent.message.delta";
  task_id: string;
  delta: string;
};

export type AgentPlanUpdatedEvent = {
  type: "agent.plan.updated";
  task_id: string;
  plan: string[];
};

export type ToolStartedEvent = {
  type: "tool.started";
  task_id: string;
  tool_name: string;
};

export type ToolOutputEvent = {
  type: "tool.output";
  task_id: string;
  tool_name: string;
  output: string;
};

export type ToolCompletedEvent = {
  type: "tool.completed";
  task_id: string;
  tool_name: string;
  success: boolean;
};

export type TaskCompletedEvent = {
  type: "task.completed";
  task_id: string;
  final_message: string;
  completed_at: string;
};

export type TaskFailedEvent = {
  type: "task.failed";
  task_id: string;
  error: string;
};

export type ServerEvent =
  | SessionCreatedEvent
  | TaskStartedEvent
  | AgentMessageDeltaEvent
  | AgentPlanUpdatedEvent
  | ToolStartedEvent
  | ToolOutputEvent
  | ToolCompletedEvent
  | TaskCompletedEvent
  | TaskFailedEvent;

export type UserMessageCommand = {
  type: "user.message";
  content: string;
};

export type ClientCommand = UserMessageCommand;

export type SessionBootstrap = {
  session_id: string;
  created_at: string;
  events: ServerEvent[];
};

