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
