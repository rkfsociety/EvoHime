import type { PermissionMode, PermissionSettings } from "../types";
import { apiRequest, apiRequestVoid } from "./client";

export function getPermissions() {
  return apiRequest<PermissionSettings>(
    "/api/permissions",
    undefined,
    "Не удалось загрузить разрешения",
  );
}

export function putPermission(name: string, mode: PermissionMode) {
  return apiRequestVoid(
    `/api/permissions/${name}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ mode }),
    },
    "Не удалось обновить разрешение",
  );
}
