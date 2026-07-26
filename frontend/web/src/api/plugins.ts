import { apiRequest, apiRequestVoid } from "./client";

export type InstalledPlugin = {
  id: string;
  name: string;
  display_name: string;
  version: string;
  description: string;
  path: string;
  skills_count: number;
  skills: string[];
};

export type CatalogPlugin = {
  name: string;
  description: string;
  version: string;
  category: string;
  group: string;
  source_url: string | null;
  source_path?: string | null;
  ref?: string | null;
  installable: boolean;
  installed: boolean;
  installed_version?: string | null;
  trust: PluginTrust;
};

export type PluginTrust = {
  score: number;
  level: "official" | "curated" | "community" | "unverified";
  reasons: string[];
};

export type CatalogGroup = {
  id: string;
  label: string;
  count: number;
};

export type PluginCatalogResponse = {
  marketplace: string;
  source: string;
  sources?: string[];
  groups?: CatalogGroup[];
  plugins: CatalogPlugin[];
};

export type PluginSkillSummary = {
  name: string;
  preview: string;
};

export type PluginIntegrityEntry = {
  name: string;
  status: "ok" | "modified" | "unlocked" | "missing";
  locked_hash?: string;
  current_hash?: string;
  trust_level?: string;
  installed_at?: string;
};

export type PluginIntegrityResponse = {
  lock_corrupted: boolean;
  plugins: PluginIntegrityEntry[];
};

export function getPluginIntegrity() {
  return apiRequest<PluginIntegrityResponse>(
    "/api/plugins/integrity",
    { method: "GET" },
    "Не удалось проверить целостность плагинов",
  );
}

export type PluginAuditEntry = {
  id: number;
  operator_id: string;
  plugin_name: string;
  action: "install" | "update" | "uninstall" | "pin" | "force_override";
  trust_level: string | null;
  risk_findings_count: number;
  force_used: boolean;
  details: string | null;
  at_ms: number;
  created_at: string;
};

export function getPluginAudit(name?: string, limit?: number) {
  const params = new URLSearchParams();
  if (name) params.set("name", name);
  if (limit) params.set("limit", String(limit));
  const query = params.toString();
  return apiRequest<PluginAuditEntry[]>(
    `/api/plugins/audit${query ? `?${query}` : ""}`,
    { method: "GET" },
    "Не удалось загрузить историю действий с плагинами",
  );
}

export function listPlugins() {
  return apiRequest<InstalledPlugin[]>(
    "/api/plugins",
    undefined,
    "Не удалось загрузить установленные плагины",
  );
}

export function listCatalog() {
  return apiRequest<PluginCatalogResponse>(
    "/api/plugins/catalog",
    undefined,
    "Не удалось загрузить каталог плагинов",
  );
}

export function installPlugin(name: string) {
  return apiRequest<InstalledPlugin>(
    "/api/plugins/install",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    },
    "Не удалось установить плагин",
  );
}

export function updatePlugin(name: string) {
  return apiRequest<InstalledPlugin>(
    "/api/plugins/update",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    },
    "Не удалось обновить плагин",
  );
}

export function uninstallPlugin(name: string) {
  return apiRequestVoid(
    "/api/plugins/uninstall",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    },
    "Не удалось удалить плагин",
  );
}

export function listPluginSkills(name: string) {
  return apiRequest<PluginSkillSummary[]>(
    `/api/plugins/${encodeURIComponent(name)}/skills`,
    undefined,
    "Не удалось загрузить skills плагина",
  );
}

export type PluginSkillStatus = {
  plugin_id: string;
  skill_name: string;
  enabled: boolean;
};

export type PluginSkillsResponse = {
  plugin_id: string;
  skills: PluginSkillStatus[];
};

export function listPluginSkillStatus(pluginId: string) {
  return apiRequest<PluginSkillsResponse>(
    `/api/plugins/${encodeURIComponent(pluginId)}/skills/status`,
    { method: "GET" },
    "Не удалось загрузить статус skills плагина",
  );
}

export function togglePluginSkill(pluginId: string, skillName: string) {
  return apiRequest<PluginSkillStatus>(
    `/api/plugins/${encodeURIComponent(pluginId)}/skills/${encodeURIComponent(skillName)}/toggle`,
    { method: "POST" },
    "Не удалось переключить skill",
  );
}
