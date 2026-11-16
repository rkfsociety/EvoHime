import { readFileSync } from 'node:fs'

import { describe, expect, it } from 'vitest'

import allowlist from '../scripts/installer-allowlist.json'

const lock = JSON.parse(
  readFileSync(new URL('../package-lock.json', import.meta.url), 'utf8')
) as { packages: Record<string, unknown> }

/**
 * `.npmrc` disables lifecycle scripts, so `npm ci` never runs a dependency
 * installer on its own; `scripts/postinstall-allowlist.mjs` runs the listed ones
 * explicitly after the install. The allowlist is therefore a second source of
 * truth next to `package-lock.json`, and a toolchain bump that drops a package
 * leaves a stale entry that stops the bootstrap in CI before any build runs:
 * Vite 8 bundles with rolldown and no longer installs `esbuild`.
 */
describe('dependency installer allow-list', () => {
  it('lists a script for every installer', () => {
    expect(allowlist.installers.length).toBeGreaterThan(0)
    for (const { name, script } of allowlist.installers) {
      expect(name, 'installer name must not be empty').not.toBe('')
      expect(script, `installer script for ${name} must not be empty`).not.toBe('')
    }
  })

  it('keeps only packages the lockfile still installs', () => {
    for (const { name } of allowlist.installers) {
      expect(
        lock.packages[`node_modules/${name}`],
        `${name} is allow-listed but absent from package-lock.json`
      ).toBeDefined()
    }
  })

  it('runs the Electron installer the shell host needs', () => {
    expect(allowlist.installers).toContainEqual({ name: 'electron', script: 'install.js' })
  })
})
