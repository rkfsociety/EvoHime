import type { ModelConfig } from "../types";
import { apiRequest } from "./client";

export interface CostLimit {
  id: number;
  model: string;
  daily_cap_tokens: number;
  reset_hour: number;
  enabled: boolean;
}

export interface CostTracking {
  id: number;
  model: string;
  date: string;
  tokens_consumed: number;
}

export interface CostLimitsResponse {
  limits: CostLimit[];
  tracking: CostTracking[];
}

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

export function getCostLimits() {
  return apiRequest<CostLimitsResponse>(
    "/api/models/cost-limits",
    undefined,
    "Не удалось загрузить лимиты стоимости",
  );
}

export function putCostLimits(body: unknown) {
  return apiRequest<CostLimitsResponse>(
    "/api/models/cost-limits",
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    },
    "Не удалось сохранить лимиты стоимости",
  );
}
