#!/usr/bin/env node
/**
 * Runs the explicitly allow-listed dependency installers.
 *
 * `.npmrc` disables lifecycle scripts for the whole project, so no dependency
 * can execute code during `npm ci`. The two packages below ship platform
 * binaries that the toolchain genuinely needs, and each is invoked here by
 * name — adding an entry requires a security review (plan 0, stage 2).
 */
import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')

const ALLOWED_INSTALLERS = [
  { name: 'electron', script: 'install.js' },
  { name: 'esbuild', script: 'install.js' }
]

let failed = false
for (const { name, script } of ALLOWED_INSTALLERS) {
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
