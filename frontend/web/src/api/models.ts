import type { ModelConfig } from "../types";
import { apiRequest } from "./client";

export function getModelConfig() {
  return apiRequest<ModelConfig>(
    "/api/models/config",
    undefined,
    "Не удалось загрузить конфигурацию модели",
  );
}

export function putModelConfig(body: unknown) {
  return apiRequest<ModelConfig>(
    "/api/models/config",
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    },
    "Не удалось сохранить конфигурацию модели",
  );
}

export function getAvailableModels(route: string) {
  return apiRequest<{ models: string[] }>(
    `/api/models/available?route=${encodeURIComponent(route)}`,
    undefined,
    "Не удалось загрузить список моделей",
  );
}
