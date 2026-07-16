import type { ProjectSummary } from "../types";
import { apiRequest } from "./client";

export function listProjects() {
  return apiRequest<ProjectSummary[]>(
    "/api/projects",
    undefined,
    "Не удалось загрузить проекты",
  );
}

export function createProject(name: string) {
  return apiRequest<ProjectSummary>(
    "/api/projects",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    },
    "Не удалось создать проект",
  );
}
