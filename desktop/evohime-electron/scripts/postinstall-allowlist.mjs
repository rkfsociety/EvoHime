#!/usr/bin/env node
/**
 * Runs the explicitly allow-listed dependency installers.
 *
 * `.npmrc` disables lifecycle scripts for the whole project, so no dependency
 * can execute code during `npm ci`. Packages listed in
 * `scripts/installer-allowlist.json` ship platform binaries that the toolchain
 * genuinely needs, and each is invoked here by name — adding an entry requires
 * a security review (plan 0, stage 2).
 *
 * The list is a second source of truth next to `package-lock.json`, so it must
 * follow the dependency tree: Vite 8 bundles with rolldown and dropped
 * `esbuild`, so that installer is removed. `tests/dependency-installers.test.ts`
 * reports a listed package that the lockfile no longer installs, and this script
 * stops the bootstrap when a listed installer cannot be found.
 */
import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptsDirectory = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(scriptsDirectory, '..')
const allowlistPath = join(scriptsDirectory, 'installer-allowlist.json')

const { installers } = JSON.parse(readFileSync(allowlistPath, 'utf8'))

let failed = false
for (const { name, script } of installers) {
  const packageDir = join(projectRoot, 'node_modules', name)
  const installer = join(packageDir, script)
  if (!existsSync(installer)) {
    console.error(`allow-listed installer is missing: ${name}/${script}`)
    failed = true
    continue
  }
  execFileSync(process.execPath, [installer], { stdio: 'inherit', cwd: packageDir })
  console.log(`ran allow-listed installer: ${name}`)
}

process.exit(failed ? 1 : 0)
