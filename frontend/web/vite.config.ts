import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";

// 7.115: Rollup still wires a static edge to the Monaco CSS chunk even though
// EditorPanel is only reached via React.lazy() — the entry never executes any
// Monaco code, so Vite's own render-blocking <link rel="stylesheet"> for it
// just delays first paint for a panel most sessions never open. Defer it with
// the standard preload-then-swap pattern instead of blocking on it.
function deferNonCriticalCss(): Plugin {
  const deferred = ["vendor-monaco"];
  return {
    name: "defer-non-critical-css",
    transformIndexHtml(html) {
      return html.replace(
        /<link rel="stylesheet"([^>]*href="[^"]*(?:vendor-monaco)[^"]*"[^>]*)>/g,
        (match, attrs) => {
          if (!deferred.some((marker) => match.includes(marker))) {
            return match;
          }
          return `<link rel="preload" as="style"${attrs} onload="this.onload=null;this.rel='stylesheet'"><noscript>${match}</noscript>`;
        },
      );
    },
  };
}

export default defineConfig({
  plugins: [react(), deferNonCriticalCss()],
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
    // 7.115: sourcemaps so production stack traces map back to TS source
    sourcemap: true,
    modulePreload: {
      // Vite preloads every chunk reachable from the entry by default, which
      // eagerly fetches (and render-blocks on) the Monaco/terminal CSS even
      // though those panels are opened on demand. Keep preload for the small,
      // likely-to-open-soon panels; skip it for the two heavy ones.
      resolveDependencies: (_filename, deps) =>
        deps.filter((dep) => !dep.includes("vendor-monaco") && !dep.includes("panel-terminal")),
    },
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
