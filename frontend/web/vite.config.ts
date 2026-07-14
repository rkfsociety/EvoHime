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
});
