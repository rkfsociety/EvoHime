import { useCallback, useEffect, useState } from "react";
import { filesApi, gitApi } from "../api";
import { initialPanelFromLocation, panelFromLocation, syncPanelToLocation } from "../lib/panel-route";
import { translateGitAction } from "../lib/format";
import { normalizePath, parentPath, sortFileNodes } from "../lib/paths";
import {
  loadSelectedProject,
  loadShowToolLines,
  loadTraceOpen,
  saveSelectedProject,
  saveShowToolLines,
  saveTraceOpen,
} from "../lib/storage";
import type {
  FileNode,
  GitAction,
  ProjectSelection,
  ProjectSummary,
  WorkspacePanel,
} from "../types";

type UseWorkspaceOptions = {
  sessionId: string | null;
  reportError: (message: string) => void;
};

export function useWorkspace({ sessionId, reportError }: UseWorkspaceOptions) {
  const [activePanel, setActivePanel] = useState<WorkspacePanel>(initialPanelFromLocation);
  const [traceOpen, setTraceOpen] = useState(loadTraceOpen);
  const [showToolLines, setShowToolLines] = useState(loadShowToolLines);
  const [selectedProject, setSelectedProject] = useState<ProjectSelection>(loadSelectedProject);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectPickerOpen, setProjectPickerOpen] = useState(false);
  const [projectSearch, setProjectSearch] = useState("");
  const [newProjectName, setNewProjectName] = useState("");
  const [projectCreating, setProjectCreating] = useState(false);
  const [projectCreateError, setProjectCreateError] = useState<string | null>(null);
  const [directoryCache, setDirectoryCache] = useState<Record<string, FileNode[]>>({});
  const [expandedDirectories, setExpandedDirectories] = useState<Record<string, boolean>>({ ".": true });
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [selectedFileContent, setSelectedFileContent] = useState("");
  const [selectedFileOriginal, setSelectedFileOriginal] = useState("");
  const [selectedFileLoading, setSelectedFileLoading] = useState(false);
  const [selectedFileNotice, setSelectedFileNotice] = useState<string | null>(null);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved">("idle");
  const [gitStatus, setGitStatus] = useState("Загрузка статуса Гит...");
  const [gitDiff, setGitDiff] = useState("Загрузка изменений Гит...");
  const [gitDiffPath, setGitDiffPath] = useState<string | null>(null);
  const [gitDiffPathInput, setGitDiffPathInput] = useState("");
  const [newFilePath, setNewFilePath] = useState("");
  const [newFileContent, setNewFileContent] = useState("");
  const [gitCommitMessage, setGitCommitMessage] = useState("");
  const [gitRemote, setGitRemote] = useState("origin");
  const [gitBranch, setGitBranch] = useState("");
  const [gitAction, setGitAction] = useState<GitAction | null>(null);
  const [gitActionNotice, setGitActionNotice] = useState<string | null>(null);

  const navigateToPanel = useCallback((panel: WorkspacePanel) => {
    setActivePanel(panel);
    syncPanelToLocation(panel);
  }, []);

  useEffect(() => {
    const onPopState = () => setActivePanel(panelFromLocation() ?? "chat");
    window.addEventListener("popstate", onPopState);
    return () => window.removeEventListener("popstate", onPopState);
  }, []);

  useEffect(() => saveSelectedProject(selectedProject), [selectedProject]);
  useEffect(() => saveTraceOpen(traceOpen), [traceOpen]);
  useEffect(() => saveShowToolLines(showToolLines), [showToolLines]);

  const refreshDirectory = useCallback(async (path: string) => {
    const data = await filesApi.listFiles(normalizePath(path));
    const key = normalizePath(data.path);
    setDirectoryCache((current) => ({ ...current, [key]: sortFileNodes(data.entries) }));
  }, []);

  const toggleDirectory = useCallback(async (path: string) => {
    const normalized = normalizePath(path);
    setExpandedDirectories((current) => ({ ...current, [normalized]: !current[normalized] }));
    if (!directoryCache[normalized]) await refreshDirectory(normalized);
  }, [directoryCache, refreshDirectory]);

  const refreshSelectedFile = useCallback(async (path: string) => {
    setSelectedFileLoading(true);
    setSelectedFileNotice(null);
    try {
      const data = await filesApi.readFile(normalizePath(path));
      setSelectedFilePath(data.path);
      setSelectedFileContent(data.content);
      setSelectedFileOriginal(data.content);
      setSaveState("idle");
    } finally {
      setSelectedFileLoading(false);
    }
  }, []);

  const refreshGitSnapshot = useCallback(async (path?: string | null) => {
    const diffPath = normalizePath(path ?? gitDiffPathInput ?? gitDiffPath ?? selectedFilePath ?? undefined);
    const [statusData, diffData] = await Promise.all([
      gitApi.getGitStatus(),
      gitApi.getGitDiff(diffPath === "." ? null : diffPath),
    ]);
    setGitStatus(statusData.status);
    setGitDiff(diffData.diff);
    setGitDiffPath(diffPath === "." ? null : diffPath);
    setGitDiffPathInput(diffPath === "." ? "" : diffPath);
  }, [gitDiffPath, gitDiffPathInput, selectedFilePath]);

  const openFile = useCallback(async (path: string) => {
    try {
      navigateToPanel("editor");
      setSelectedFileNotice(null);
      await refreshSelectedFile(path);
      await refreshGitSnapshot(path);
    } catch (error) {
      reportError(String(error));
    }
  }, [navigateToPanel, refreshGitSnapshot, refreshSelectedFile, reportError]);

  const saveFile = useCallback(async () => {
    if (!sessionId || !selectedFilePath) return;
    setSaveState("saving");
    try {
      const data = await filesApi.saveFile(selectedFilePath, selectedFileContent, sessionId);
      setSelectedFileOriginal(selectedFileContent);
      setSaveState("saved");
      setSelectedFileNotice(data.change === "created" ? "Новый файл создан в рабочем пространстве." : "Изменения сохранены.");
      await refreshDirectory(parentPath(data.path));
      await refreshGitSnapshot(data.path);
    } catch (error) {
      setSaveState("idle");
      reportError(String(error));
    }
  }, [refreshDirectory, refreshGitSnapshot, reportError, selectedFileContent, selectedFilePath, sessionId]);

  const createFile = useCallback(async () => {
    if (!sessionId || !newFilePath.trim()) return;
    const path = normalizePath(newFilePath.trim());
    try {
      await filesApi.createFile(path, newFileContent, sessionId);
      setNewFilePath("");
      setNewFileContent("");
      await refreshDirectory(parentPath(path));
      await refreshDirectory(".");
      await openFile(path);
    } catch (error) {
      setSelectedFileNotice(`Не удалось создать файл: ${String(error)}`);
    }
  }, [newFileContent, newFilePath, openFile, refreshDirectory, sessionId]);

  const gitOperation = useCallback(async (action: GitAction) => {
    if (!sessionId || gitAction) return;
    setGitAction(action);
    setGitActionNotice(null);
    try {
      if (action === "commit") await gitApi.gitCommit(sessionId, gitCommitMessage);
      else if (action === "pull") await gitApi.gitPull(sessionId, gitRemote || undefined, gitBranch || undefined);
      else await gitApi.gitPush(sessionId, gitRemote || undefined, gitBranch || undefined);
      setGitActionNotice(`Операция Гит «${translateGitAction(action)}» завершена.`);
      if (action === "commit") setGitCommitMessage("");
      await refreshGitSnapshot(gitDiffPath);
      await refreshDirectory(".");
    } catch (error) {
      setGitActionNotice(`Операция Гит «${translateGitAction(action)}»: ${String(error)}`);
    } finally {
      setGitAction(null);
    }
  }, [gitAction, gitBranch, gitCommitMessage, gitDiffPath, gitRemote, refreshDirectory, refreshGitSnapshot, sessionId]);

  return {
    activePanel,
    setActivePanel,
    navigateToPanel,
    traceOpen,
    setTraceOpen,
    showToolLines,
    setShowToolLines,
    selectedProject,
    setSelectedProject,
    projects,
    setProjects,
    projectPickerOpen,
    setProjectPickerOpen,
    projectSearch,
    setProjectSearch,
    newProjectName,
    setNewProjectName,
    projectCreating,
    setProjectCreating,
    projectCreateError,
    setProjectCreateError,
    directoryCache,
    expandedDirectories,
    setExpandedDirectories,
    selectedFilePath,
    setSelectedFilePath,
    selectedFileContent,
    setSelectedFileContent,
    selectedFileOriginal,
    setSelectedFileOriginal,
    selectedFileLoading,
    selectedFileNotice,
    setSelectedFileNotice,
    saveState,
    setSaveState,
    gitStatus,
    setGitStatus,
    gitDiff,
    setGitDiff,
    gitDiffPath,
    setGitDiffPath,
    gitDiffPathInput,
    setGitDiffPathInput,
    newFilePath,
    setNewFilePath,
    newFileContent,
    setNewFileContent,
    gitCommitMessage,
    setGitCommitMessage,
    gitRemote,
    setGitRemote,
    gitBranch,
    setGitBranch,
    gitAction,
    gitActionNotice,
    refreshDirectory,
    toggleDirectory,
    refreshSelectedFile,
    refreshGitSnapshot,
    openFile,
    saveFile,
    createFile,
    gitOperation,
  };
}
