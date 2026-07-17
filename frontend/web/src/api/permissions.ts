import type {
  PermissionAuditEntry,
  PermissionMode,
  PermissionScopes,
  PermissionSettings,
} from "../types";
import { apiRequest, apiRequestVoid } from "./client";

export function getPermissions() {
  return apiRequest<PermissionSettings>(
    "/api/permissions",
    undefined,
    "Не удалось загрузить разрешения",
  );
}

export function getPermissionAudit() {
  return apiRequest<{ entries: PermissionAuditEntry[] }>(
    "/api/permissions/audit",
    undefined,
    "Не удалось загрузить аудит approvals",
  );
}

export function getPermissionScopes() {
  return apiRequest<PermissionScopes>(
    "/api/permissions/scopes",
    undefined,
    "Не удалось загрузить scoped permissions",
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
