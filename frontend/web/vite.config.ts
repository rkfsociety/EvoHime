import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": process.env.VITE_API_PROXY_TARGET ?? "http://localhost:3000",
      "/ws": {
        target: (process.env.VITE_API_PROXY_TARGET ?? "http://localhost:3000").replace(
          /^http/,
          "ws",
        ),
        ws: true,
      },
    },
  },
  // Phase 5.8: Frontend code splitting for reduced initial bundle
  build: {
    rollupOptions: {
      output: {
        // Manual chunk splitting for known panels
        manualChunks: {
          // Core dependencies (always needed)
          "vendor-react": ["react", "react-dom"],
          "vendor-markdown": ["react-markdown", "remark-gfm"],
          "vendor-monaco": ["monaco-editor"],
          // Panel chunks (lazy-loaded)
          "panel-editor": ["./src/panels/EditorPanel.tsx"],
          "panel-files": ["./src/panels/FilesPanel.tsx"],
          "panel-git": ["./src/panels/GitPanel.tsx"],
          "panel-tasks": ["./src/panels/TasksPanel.tsx"],
          "panel-actions": ["./src/panels/ActionsPanel.tsx"],
          "panel-terminal": ["./src/components/TerminalPanel.tsx"],
          "panel-settings": ["./src/panels/SettingsPanel.tsx"],
          "panel-memory": ["./src/panels/MemoryPanel.tsx"],
          "panel-plugins": ["./src/panels/PluginsPanel.tsx"],
          "panel-pull-requests": ["./src/panels/PullRequestsPanel.tsx"],
          "panel-scheduled": ["./src/panels/ScheduledPanel.tsx"],
          "panel-sites": ["./src/panels/SitesPanel.tsx"],
        },
      },
    },
  },
});
