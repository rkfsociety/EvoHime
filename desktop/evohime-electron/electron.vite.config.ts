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
const target = process.env.EVOHIME_ELECTRON_TARGET === 'updater' ? 'updater' : 'shell'
const isUpdaterTarget = target === 'updater'
const output = (kind: 'main' | 'preload' | 'renderer'): string => {
  if (isUpdaterTarget) return resolve(__dirname, `out/updater-${kind}`)
  return resolve(__dirname, `out/${kind === 'renderer' ? 'ui-bundle' : kind}`)
}

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    resolve: {
      alias: { '@shared': resolve(__dirname, 'src/shared') }
    },
    build: {
      outDir: output('main'),
      sourcemap: !isProduction,
      minify: isProduction,
      rollupOptions: {
        input: {
          [isUpdaterTarget ? 'updater' : 'index']: resolve(
            __dirname,
            isUpdaterTarget ? 'src/main/updater.ts' : 'src/main/index.ts'
          )
        }
      }
    }
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    resolve: {
      alias: { '@shared': resolve(__dirname, 'src/shared') }
    },
    build: {
      outDir: output('preload'),
      sourcemap: !isProduction,
      minify: isProduction,
      rollupOptions: {
        // A sandboxed preload cannot use ESM or `require` of app modules, so it
        // is bundled into a single CommonJS file with no external imports.
        input: {
          [isUpdaterTarget ? 'updater' : 'index']: resolve(
            __dirname,
            isUpdaterTarget ? 'src/preload/updater.ts' : 'src/preload/index.ts'
          )
        },
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
      outDir: output('renderer'),
      rollupOptions: {
        input: {
          [isUpdaterTarget ? 'updater' : 'index']: resolve(
            __dirname,
            isUpdaterTarget ? 'src/renderer/updater.html' : 'src/renderer/index.html'
          )
        }
      }
    }
  }
})
