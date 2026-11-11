import { resolve } from 'node:path'

import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { '@shared': resolve(__dirname, 'src/shared') }
  },
  test: {
    environment: 'node',
    include: ['tests/**/*.test.ts', 'tests/**/*.test.tsx'],
    globals: false,
    restoreMocks: true,
    setupFiles: ['tests/setup.ts'],
    environmentOptions: {
      // jsdom ≥ 26 exposes localStorage only for non-opaque origins; without
      // an URL the default about:blank origin is opaque and every test that
      // touches window.localStorage fails with an undefined storage.
      jsdom: { url: 'http://localhost/' }
    },
    // Keep the feedback loop deterministic: the first failing test stops the
    // run before another file starts doing work.
    bail: 1,
    fileParallelism: false
  }
})
