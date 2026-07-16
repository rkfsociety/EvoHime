import type { McpServerConfig, ToolDefinition } from "../types";
import { apiRequest } from "./client";

export function listTools() {
  return apiRequest<ToolDefinition[]>(
    "/api/tools",
    undefined,
    "Не удалось загрузить каталог инструментов",
  );
}

export function listMcpServers() {
  return apiRequest<McpServerConfig[]>(
    "/api/mcp/servers",
    undefined,
    "Не удалось загрузить MCP-серверы",
  );
}

export function putMcpServers(servers: McpServerConfig[]) {
  return apiRequest<McpServerConfig[]>(
    "/api/mcp/servers",
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(servers),
    },
    "Не удалось сохранить MCP-серверы",
  );
}
