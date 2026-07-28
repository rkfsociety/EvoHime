import { apiRequest } from "./client";

export interface ThinkingSettingsResponse {
  enabled: boolean;
  budget_tokens: number | null;
  max_budget_tokens: number | null;
  show_thinking: boolean;
  thinking_verbosity: string | null;
  monthly_cost_limit_usd: number | null;
  warning_threshold_percent: number | null;
  monthly_spending_usd: number | null;
}

export interface ThinkingSettingsRequest {
  enabled: boolean;
  budget_tokens?: number;
  show_thinking?: boolean;
  thinking_verbosity?: string;
}

export function getThinkingSettings() {
  return apiRequest<ThinkingSettingsResponse>(
    "/api/settings/thinking",
    undefined,
    "Не удалось загрузить настройки расширенного мышления",
  );
}

export function putThinkingSettings(settings: ThinkingSettingsRequest) {
  return apiRequest<ThinkingSettingsResponse>(
    "/api/settings/thinking",
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(settings),
    },
    "Не удалось сохранить настройки расширенного мышления",
  );
}
