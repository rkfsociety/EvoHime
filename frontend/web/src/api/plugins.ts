import { apiRequest } from "./client";

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
