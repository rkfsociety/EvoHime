import type { ProjectComposerPreference, ProjectSelection } from "../types";

export const selectedProjectStorageKey = "evohime.selectedProject";
export const projectComposerPreferencesStorageKey = "evohime.projectComposerPreferences";
export const traceOpenStorageKey = "evohime.traceOpen";
export const showToolLinesStorageKey = "evohime.showToolLines";

export function loadTraceOpen() {
  try {
    return localStorage.getItem(traceOpenStorageKey) === "true";
  } catch {
    return false;
  }
}

export function projectPreferenceKey(path: string | null) {
  return path ?? "__no_project__";
}

export function loadProjectComposerPreference(path: string | null): ProjectComposerPreference {
  try {
    const stored = localStorage.getItem(projectComposerPreferencesStorageKey);
    const preferences = stored
      ? (JSON.parse(stored) as Record<string, ProjectComposerPreference>)
      : {};
    return preferences[projectPreferenceKey(path)] ?? {};
  } catch {
    return {};
  }
}

export function saveProjectComposerPreference(
  path: string | null,
  preference: ProjectComposerPreference,
) {
  try {
    const stored = localStorage.getItem(projectComposerPreferencesStorageKey);
    const preferences = stored
      ? (JSON.parse(stored) as Record<string, ProjectComposerPreference>)
      : {};
    preferences[projectPreferenceKey(path)] = preference;
    localStorage.setItem(projectComposerPreferencesStorageKey, JSON.stringify(preferences));
  } catch {
    // Browser storage can be unavailable; the in-memory state still works.
  }
}

export function loadSelectedProject(): ProjectSelection {
  try {
    const stored = localStorage.getItem(selectedProjectStorageKey);
    if (stored) {
      const project = JSON.parse(stored) as ProjectSelection;
      if (
        typeof project.label === "string" &&
        (typeof project.path === "string" || project.path === null)
      ) {
        return project;
      }
    }
  } catch {
    // Ignore malformed browser state and use the current workspace.
  }
  return { label: "EvoHime", path: "." };
}

export function saveSelectedProject(project: ProjectSelection) {
  try {
    localStorage.setItem(selectedProjectStorageKey, JSON.stringify(project));
  } catch {
    // Ignore storage failures.
  }
}

export function saveTraceOpen(open: boolean) {
  try {
    localStorage.setItem(traceOpenStorageKey, open ? "true" : "false");
  } catch {
    // Ignore storage failures.
  }
}

export function loadShowToolLines() {
  try {
    const stored = localStorage.getItem(showToolLinesStorageKey);
    return stored !== "false";
  } catch {
    return true;
  }
}

export function saveShowToolLines(show: boolean) {
  try {
    localStorage.setItem(showToolLinesStorageKey, show ? "true" : "false");
  } catch {
    // Ignore storage failures.
  }
}
