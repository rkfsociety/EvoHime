import { apiRequest, apiRequestVoid, jsonBody } from "./client";

export type ScheduledTask = {
  id: string;
  workspace_path: string;
  title: string;
  prompt: string;
  cron_expr: string;
  status: "active" | "paused";
  last_run_at: string | null;
  next_run_at: string;
  run_count: number;
  failure_count: number;
  last_run_status: "dispatched" | "failed" | null;
  last_run_error: string | null;
  created_at: string;
  updated_at: string;
};

export type ScheduledTaskInput = {
  title: string;
  prompt: string;
  cron_expr: string;
};

const q = (workspacePath: string) =>
  `?workspace_path=${encodeURIComponent(workspacePath)}`;

export function listScheduled(workspacePath: string) {
  return apiRequest<ScheduledTask[]>(
    `/api/scheduled${q(workspacePath)}`,
    undefined,
    "Не удалось загрузить расписания",
  );
}

export function createScheduled(workspacePath: string, input: ScheduledTaskInput) {
  return apiRequest<ScheduledTask>(
    `/api/scheduled${q(workspacePath)}`,
    { method: "POST", ...jsonBody(input) },
    "Не удалось создать расписание",
  );
}

export function updateScheduled(
  workspacePath: string,
  id: string,
  input: ScheduledTaskInput,
) {
  return apiRequest<ScheduledTask>(
    `/api/scheduled/${id}${q(workspacePath)}`,
    { method: "PUT", ...jsonBody(input) },
    "Не удалось обновить расписание",
  );
}

export function deleteScheduled(workspacePath: string, id: string) {
  return apiRequestVoid(
    `/api/scheduled/${id}${q(workspacePath)}`,
    { method: "DELETE" },
    "Не удалось удалить расписание",
  );
}

export function pauseScheduled(workspacePath: string, id: string) {
  return apiRequest<ScheduledTask>(
    `/api/scheduled/${id}/pause${q(workspacePath)}`,
    { method: "POST" },
    "Не удалось приостановить расписание",
  );
}

export function resumeScheduled(workspacePath: string, id: string) {
  return apiRequest<ScheduledTask>(
    `/api/scheduled/${id}/resume${q(workspacePath)}`,
    { method: "POST" },
    "Не удалось возобновить расписание",
  );
}

export function triggerScheduled(workspacePath: string, id: string) {
  return apiRequest<ScheduledTask>(
    `/api/scheduled/${id}/trigger${q(workspacePath)}`,
    { method: "POST" },
    "Не удалось запустить расписание вручную",
  );
}
