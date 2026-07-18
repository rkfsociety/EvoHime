import { apiRequest, apiRequestVoid } from "./client";

export type Site = { id: string; workspace_path: string; name: string; slug: string; description: string; status: "draft" | "published"; created_at: string; updated_at: string };
export type SiteInput = Pick<Site, "name" | "slug" | "description" | "status">;
const query = (workspacePath: string) => `?workspace_path=${encodeURIComponent(workspacePath)}`;
export function listSites(workspacePath: string) { return apiRequest<Site[]>(`/api/sites${query(workspacePath)}`, undefined, "Не удалось загрузить сайты"); }
export function createSite(workspacePath: string, input: SiteInput) { return apiRequest<Site>(`/api/sites${query(workspacePath)}`, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(input) }, "Не удалось создать сайт"); }
export function deleteSite(workspacePath: string, id: string) { return apiRequestVoid(`/api/sites/${id}${query(workspacePath)}`, { method: "DELETE" }, "Не удалось удалить сайт"); }
export function publishSite(workspacePath: string, id: string) { return apiRequest<Site>(`/api/sites/${id}/publish${query(workspacePath)}`, { method: "POST" }, "Не удалось опубликовать сайт"); }
export function previewUrl(workspacePath: string, id: string) { return `/api/sites/${id}/preview${query(workspacePath)}`; }
