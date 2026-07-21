import { apiRequest, apiRequestVoid } from "./client";

export type Site = {
  id: string;
  workspace_path: string;
  name: string;
  slug: string;
  description: string;
  status: "draft" | "published";
  created_at: string;
  updated_at: string;
};
export type SiteInput = Pick<Site, "name" | "slug" | "description" | "status">;
export type SiteStatusFilter = "all" | "draft" | "published";
export type SiteListOptions = {
  q?: string;
  status?: SiteStatusFilter;
};

const query = (workspacePath: string, options?: SiteListOptions) => {
  const params = new URLSearchParams({ workspace_path: workspacePath });
  const q = options?.q?.trim();
  if (q) {
    params.set("q", q);
  }
  if (options?.status && options.status !== "all") {
    params.set("status", options.status);
  }
  return `?${params.toString()}`;
};

export function listSites(workspacePath: string, options?: SiteListOptions) {
  return apiRequest<Site[]>(
    `/api/sites${query(workspacePath, options)}`,
    undefined,
    "Не удалось загрузить сайты",
  );
}
export function createSite(workspacePath: string, input: SiteInput) {
  return apiRequest<Site>(
    `/api/sites${query(workspacePath)}`,
    { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) },
    "Не удалось создать сайт",
  );
}
export function deleteSite(workspacePath: string, id: string) {
  return apiRequestVoid(
    `/api/sites/${id}${query(workspacePath)}`,
    { method: "DELETE" },
    "Не удалось удалить сайт",
  );
}
export function publishSite(workspacePath: string, id: string) {
  return apiRequest<Site>(
    `/api/sites/${id}/publish${query(workspacePath)}`,
    { method: "POST" },
    "Не удалось опубликовать сайт",
  );
}
export function previewUrl(workspacePath: string, id: string) {
  return `/api/sites/${id}/preview${query(workspacePath)}`;
}
