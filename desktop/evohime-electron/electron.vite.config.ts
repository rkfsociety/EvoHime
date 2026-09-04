import { resolve } from 'node:path'

import react from '@vitejs/plugin-react'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'

function stripProductionCspInDev() {
  return {
    name: 'strip-production-csp-in-dev',
    transformIndexHtml(html: string, context: { server?: unknown }) {
      if (!context.server) return html
      return html.replace(/\s*<meta\s+http-equiv="Content-Security-Policy"[^>]*>\s*/i, '\n')
    }
  }
}

// Production builds never emit source maps: the packaged renderer must not ship
// readable sources or map files (plan 0, stage 2).
const isProduction = process.env.NODE_ENV === 'production'

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    resolve: {
      alias: { '@shared': resolve(__dirname, 'src/shared') }
    },
    build: {
      sourcemap: !isProduction,
      minify: isProduction,
      rollupOptions: {
        input: { index: resolve(__dirname, 'src/main/index.ts') }
      }
    }
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    resolve: {
      alias: { '@shared': resolve(__dirname, 'src/shared') }
    },
    build: {
      sourcemap: !isProduction,
      minify: isProduction,
      rollupOptions: {
        // A sandboxed preload cannot use ESM or `require` of app modules, so it
        // is bundled into a single CommonJS file with no external imports.
        input: { index: resolve(__dirname, 'src/preload/index.ts') },
        output: { format: 'cjs' }
      }
    }
  },
  renderer: {
    root: resolve(__dirname, 'src/renderer'),
    plugins: [react(), stripProductionCspInDev()],
    resolve: {
      alias: { '@shared': resolve(__dirname, 'src/shared') }
    },
    build: {
      sourcemap: false,
      outDir: resolve(__dirname, 'out/ui-bundle'),
      rollupOptions: {
        input: { index: resolve(__dirname, 'src/renderer/index.html') }
      }
    }
  }
})
