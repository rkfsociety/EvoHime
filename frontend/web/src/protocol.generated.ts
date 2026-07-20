/* eslint-disable */
/**
 * Generated from crates/protocol/schema/evohime.protocol.schema.json.
 * Do not edit by hand. Run: npm run generate:protocol
 */

export type EvoHimeProtocol = ServerEvent | ClientCommand;
export type ServerEvent =
  | SessionCreatedEvent
  | TaskStartedEvent
  | AgentMessageDeltaEvent
  | AgentStatusEvent
  | AgentPlanUpdatedEvent
  | ToolStartedEvent
  | ToolOutputEvent
  | ToolOutputDeltaEvent
  | ToolCompletedEvent
  | TaskCompletedEvent
  | TaskFailedEvent
  | FileChangedEvent
  | GitDiffChangedEvent
  | TaskStatusChangedEvent
  | TaskStepChangedEvent
  | ActionLoggedEvent
  | ApprovalRequiredEvent
  | MemoryProposedEvent
  | MemoryAskEvent
  | MemoryAcceptedEvent
  | MemoryRejectedEvent
  | MemoryUsedEvent;
export type Uuid = string;
export type DateTime = string;
export type ClientCommand =
  | UserMessageCommand
  | TaskCancelCommand
  | TaskResumeCommand
  | TaskRetryCommand
  | TaskPlanApproveCommand
  | TaskPlanRejectCommand
  | ApprovalGrantedCommand
  | ApprovalDeniedCommand
  | MemoryAcceptCommand
  | MemoryRejectCommand;

export interface SessionCreatedEvent {
  type: "session.created";
  session_id: Uuid;
  created_at: DateTime;
}
export interface TaskStartedEvent {
  type: "task.started";
  task_id: Uuid;
  session_id: Uuid;
  user_message: string;
  created_at: DateTime;
}
export interface AgentMessageDeltaEvent {
  type: "agent.message.delta";
  task_id: Uuid;
  delta: string;
}
export interface AgentStatusEvent {
  type: "agent.status";
  task_id: Uuid;
  phase: "selecting_action" | "executing_tools" | "waiting_approval" | "responding" | "stopped";
}
export interface AgentPlanUpdatedEvent {
  type: "agent.plan.updated";
  task_id: Uuid;
  plan: PlanStep[];
}
export interface PlanStep {
  id: string;
  tool_name: string;
  description: string;
  depends_on?: string[];
}
export interface ToolStartedEvent {
  type: "tool.started";
  task_id: Uuid;
  tool_name: string;
}
export interface ToolOutputEvent {
  type: "tool.output";
  task_id: Uuid;
  tool_name: string;
  output: string;
}
export interface ToolOutputDeltaEvent {
  type: "tool.output.delta";
  task_id: Uuid;
  tool_name: string;
  stream: "stdout" | "stderr" | "log";
  delta: string;
}
export interface ToolCompletedEvent {
  type: "tool.completed";
  task_id: Uuid;
  tool_name: string;
  success: boolean;
}
export interface TaskCompletedEvent {
  type: "task.completed";
  task_id: Uuid;
  final_message: string;
  completed_at: DateTime;
}
export interface TaskFailedEvent {
  type: "task.failed";
  task_id: Uuid;
  error: string;
}
export interface FileChangedEvent {
  type: "file.changed";
  path: string;
  change: "created" | "updated" | "deleted";
  created_at: DateTime;
}
export interface GitDiffChangedEvent {
  type: "git.diff.changed";
  status: string;
  diff: string;
  created_at: DateTime;
}
export interface TaskStatusChangedEvent {
  type: "task.status.changed";
  task_id: Uuid;
  status: string;
}
export interface TaskStepChangedEvent {
  type: "task.step.changed";
  task_id: Uuid;
  step_id: Uuid;
  status: string;
  tool_name: string;
}
export interface ActionLoggedEvent {
  type: "action.logged";
  task_id: Uuid;
  action: string;
  detail: string;
  created_at: DateTime;
}
export interface ApprovalRequiredEvent {
  type: "approval.required";
  approval_id: Uuid;
  task_id: Uuid;
  tool_name: string;
  permission: string;
  scope: string;
  created_at: DateTime;
}
export interface MemoryProposedEvent {
  type: "memory.proposed";
  memory_id: Uuid;
  task_id: Uuid;
  scope: string;
  kind: string;
  content: string;
  confidence: number;
  status: string;
}
export interface MemoryAskEvent {
  type: "memory.ask";
  memory_id: Uuid;
  task_id: Uuid;
  scope: string;
  kind: string;
  content: string;
  confidence: number;
  status: string;
  reason: string;
}
export interface MemoryAcceptedEvent {
  type: "memory.accepted";
  memory_id: Uuid;
  task_id: Uuid;
}
export interface MemoryRejectedEvent {
  type: "memory.rejected";
  memory_id: Uuid;
  task_id: Uuid;
}
export interface MemoryUsedEvent {
  type: "memory.used";
  memory_id: Uuid;
  task_id: Uuid;
  signal: string;
  confidence: number;
}
export interface UserMessageCommand {
  type: "user.message";
  content: string;
  model_route?: string;
  model?: string;
  workspace_path?: string;
}
export interface TaskCancelCommand {
  type: "task.cancel";
  task_id: Uuid;
}
export interface TaskResumeCommand {
  type: "task.resume";
  task_id: Uuid;
}
export interface TaskRetryCommand {
  type: "task.retry";
  task_id: Uuid;
}
export interface TaskPlanApproveCommand {
  type: "task.plan.approve";
  task_id: Uuid;
  plan: PlanStep[];
}
export interface TaskPlanRejectCommand {
  type: "task.plan.reject";
  task_id: Uuid;
}
export interface ApprovalGrantedCommand {
  type: "approval.granted";
  approval_id: Uuid;
}
export interface ApprovalDeniedCommand {
  type: "approval.denied";
  approval_id: Uuid;
}
export interface MemoryAcceptCommand {
  type: "memory.accept";
  memory_id: Uuid;
}
export interface MemoryRejectCommand {
  type: "memory.reject";
  memory_id: Uuid;
}


export type HistoryItem = {
  sequence: number;
  created_at: string;
  event: ServerEvent;
};

export type SessionBootstrap = {
  session_id: string;
  created_at: string;
  events: HistoryItem[];
};
