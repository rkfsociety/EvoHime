import { existsSync } from 'node:fs'
import { cp, mkdir, rm, writeFile } from 'node:fs/promises'
import { join } from 'node:path'

import { BUILD_MARKER_FILE, type BuildMarker } from './config'
import { describeFailure, runCommand, type CommandRunner } from './run-command'
import {
  npmInvocation,
  toolPath,
  toolchainEnvironment,
  type ToolchainReport
} from './toolchain'

/**
 * Local rebuild of the product from the updated checkout.
 *
 * The steps mirror `scripts/build-windows-native.ps1`: Rust binaries, then the
 * Electron bundle, then the unpacked package. The result is assembled into a
 * staging directory — never into the running installation, which
 * `evohime-transaction.exe` swaps under its own backup/rollback journal.
 */

export const ELECTRON_SUBPATH = join('desktop', 'evohime-electron')

const CARGO_PACKAGES = ['evohime-core', 'evohime-supervisor', 'evohime-updater'] as const

export const REQUIRED_NATIVE_COMPONENTS = [
  'evohime-core.exe',
  'evohime-supervisor.exe',
  'evohime-transaction.exe'
] as const

const CARGO_TIMEOUT_MS = 90 * 60_000
const NPM_TIMEOUT_MS = 60 * 60_000

export class BuildError extends Error {}

export interface BuildInputs {
  readonly sourceDirectory: string
  readonly stagingDirectory: string
  readonly commit: string
  readonly branch: string
  readonly toolchain: ToolchainReport
}

export interface BuildDeps {
  readonly run?: CommandRunner
  readonly env?: NodeJS.ProcessEnv
  readonly exists?: (path: string) => boolean
  readonly onLine?: (line: string) => void
  /** Called when a build step starts, so the UI can advance its checklist. */
  readonly onStep?: (step: 'core' | 'shell' | 'package') => void
  readonly now?: () => number
  readonly signal?: AbortSignal
}

/** Builds the checkout and leaves a ready-to-swap package in the staging directory. */
export async function buildStagedPackage(
  inputs: BuildInputs,
  deps: BuildDeps = {}
): Promise<BuildMarker> {
  const run = deps.run ?? runCommand
  const exists = deps.exists ?? existsSync
  const env = toolchainEnvironment(inputs.toolchain, deps.env ?? process.env)
  const electronRoot = join(inputs.sourceDirectory, ELECTRON_SUBPATH)

  const cargo = toolPath(inputs.toolchain, 'rust')
  const npm = npmInvocation(inputs.toolchain, exists)
  const node = toolPath(inputs.toolchain, 'node')
  if (!cargo || !npm || !node) {
    throw new BuildError('Инструменты сборки не найдены — пересборка невозможна.')
  }

  const exec = async (
    label: string,
    file: string,
    args: readonly string[],
    cwd: string,
    timeoutMs: number
  ): Promise<void> => {
    const result = await run({
      file,
      args,
      cwd,
      env,
      timeoutMs,
      ...(deps.onLine ? { onLine: deps.onLine } : {}),
      ...(deps.signal ? { signal: deps.signal } : {})
    })
    if (result.code !== 0) throw new BuildError(describeFailure(label, result))
  }

  deps.onStep?.('core')
  await exec(
    'cargo build',
    cargo,
    ['build', '--release', '--locked', ...CARGO_PACKAGES.flatMap((name) => ['-p', name])],
    inputs.sourceDirectory,
    CARGO_TIMEOUT_MS
  )

  deps.onStep?.('shell')
  await exec('npm ci', npm.file, [...npm.args, 'ci', '--ignore-scripts'], electronRoot, NPM_TIMEOUT_MS)
  await exec(
    'postinstall allow-list',
    node,
    [join('scripts', 'postinstall-allowlist.mjs')],
    electronRoot,
    NPM_TIMEOUT_MS
  )
  await exec('npm run build', npm.file, [...npm.args, 'run', 'build'], electronRoot, NPM_TIMEOUT_MS)

  deps.onStep?.('package')
  await exec(
    'electron-builder',
    npm.file,
    [...npm.args, 'exec', '--', 'electron-builder', '--dir', '--config', 'electron-builder.yml'],
    electronRoot,
    NPM_TIMEOUT_MS
  )

  return assembleStaging(inputs, electronRoot, { ...deps, exists })
}

/**
 * Collects the freshly built binaries and Electron payload into the staging
 * directory. The layout matches what the installer lays down, so the swap is a
 * plain file replacement.
 */
export async function assembleStaging(
  inputs: BuildInputs,
  electronRoot: string,
  deps: BuildDeps & { readonly exists: (path: string) => boolean }
): Promise<BuildMarker> {
  const cargoTarget = join(inputs.sourceDirectory, 'target', 'release')
  const unpacked = join(electronRoot, 'release', 'win-unpacked')
  if (!deps.exists(unpacked)) {
    throw new BuildError('Electron package не собрался — каталог win-unpacked отсутствует.')
  }

  await rm(inputs.stagingDirectory, { recursive: true, force: true })
  await mkdir(inputs.stagingDirectory, { recursive: true })
  await cp(unpacked, inputs.stagingDirectory, { recursive: true })

  for (const component of REQUIRED_NATIVE_COMPONENTS) {
    const source = join(cargoTarget, component)
    if (!deps.exists(source)) {
      throw new BuildError(`Native-компонент не собрался: ${component}`)
    }
    await cp(source, join(inputs.stagingDirectory, component))
  }

  await writeFile(
    join(inputs.stagingDirectory, 'evohime.manifest.json'),
    `${JSON.stringify(nativeManifest(), null, 2)}\n`,
    'utf8'
  )

  const marker: BuildMarker = {
    commit: inputs.commit,
    branch: inputs.branch,
    builtAtMs: (deps.now ?? Date.now)()
  }
  await writeFile(
    join(inputs.stagingDirectory, BUILD_MARKER_FILE),
    `${JSON.stringify(marker, null, 2)}\n`,
    'utf8'
  )

  if (!deps.exists(join(inputs.stagingDirectory, 'EvoHime.exe'))) {
    throw new BuildError('В staging нет EvoHime.exe — пакет неполный.')
  }
  return marker
}

/**
 * Throws away everything a build derives, keeping only the sources.
 *
 * The Electron runtime is downloaded into `.electron-cache`, and an interrupted
 * download leaves an archive that unpacks into a broken package — the failure
 * then repeats on every attempt until the cache is cleared. Dropping it costs
 * one download and turns a permanent failure into a slower retry.
 */
export async function clearDerivedState(sourceDirectory: string): Promise<void> {
  const electronRoot = join(sourceDirectory, ELECTRON_SUBPATH)
  for (const path of ['release', '.electron-cache', 'out']) {
    await rm(join(electronRoot, path), { recursive: true, force: true })
  }
}

/** Same shape as `New-NativePackageManifest` in `scripts/native-package.ps1`. */
function nativeManifest(): Record<string, unknown> {
  return {
    product: 'EvoHime',
    client: 'electron-shell',
    architecture: 'x64',
    os_minimum: 'Windows 10 2004 / Windows 11',
    protocol: 'desktop-ipc-v1',
    components: {
      ui: 'EvoHime.exe',
      core: 'evohime-core.exe',
      supervisor: 'evohime-supervisor.exe',
      updater: 'evohime-transaction.exe'
    }
  }
}
