import type { WorkspacePanel } from "../types";

const WORKSPACE_PANELS: WorkspacePanel[] = [
  "chat",
  "files",
  "sites",
  "editor",
  "terminal",
  "git",
  "plugins",
  "memory",
  "pull-requests",
  "scheduled",
  "tasks",
  "actions",
  "settings",
];

function isWorkspacePanel(value: string): value is WorkspacePanel {
  return WORKSPACE_PANELS.includes(value as WorkspacePanel);
}

export function panelFromLocation(search = window.location.search): WorkspacePanel | null {
  const raw = new URLSearchParams(search).get("panel")?.trim();
  if (!raw || !isWorkspacePanel(raw)) {
    return null;
  }
  return raw;
}

export function syncPanelToLocation(panel: WorkspacePanel, mode: "push" | "replace" = "push") {
  const url = new URL(window.location.href);
  if (panel === "chat") {
    url.searchParams.delete("panel");
  } else {
    url.searchParams.set("panel", panel);
  }
  const href = `${url.pathname}${url.search}${url.hash}`;
  if (mode === "replace") {
    history.replaceState({ panel }, "", href);
  } else {
    history.pushState({ panel }, "", href);
  }
}

export function initialPanelFromLocation(): WorkspacePanel {
  return panelFromLocation() ?? "chat";
}
