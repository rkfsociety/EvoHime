import { createHash } from 'node:crypto'
import { existsSync } from 'node:fs'
import { cp, mkdir, mkdtemp, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises'
import { join, resolve, sep } from 'node:path'
import { tmpdir } from 'node:os'

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

/** Records which lockfile the current `node_modules` was installed from. */
export const DEPENDENCY_MARKER = join('node_modules', '.evohime-deps')

const CARGO_TIMEOUT_MS = 90 * 60_000
const NPM_TIMEOUT_MS = 60 * 60_000
const CLEANUP_MAX_RETRIES = 60
const CLEANUP_RETRY_DELAY_MS = 500

export class BuildError extends Error {}

export interface BuildInputs {
  readonly sourceDirectory: string
  readonly stagingDirectory: string
  /** Installed application directory; build output must never overlap it. */
  readonly installDirectory: string
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
  assertBuildIsolation(inputs.sourceDirectory, inputs.installDirectory)

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
  // `npm ci` deletes node_modules and reinstalls every package, and the
  // allow-list then unpacks Electron again — a minute of work that changes
  // nothing while the lockfile is the same. Most updates do not touch it.
  const lockDigest = await lockfileDigest(electronRoot)
  if (lockDigest === null || lockDigest !== (await installedDependencies(electronRoot))) {
    await exec('npm ci', npm.file, [...npm.args, 'ci', '--ignore-scripts'], electronRoot, NPM_TIMEOUT_MS)
    await exec(
      'postinstall allow-list',
      node,
      [join('scripts', 'postinstall-allowlist.mjs')],
      electronRoot,
      NPM_TIMEOUT_MS
    )
    await recordDependencies(electronRoot, lockDigest)
  } else {
    deps.onLine?.('Зависимости не изменились — установка пропущена.')
  }
  await exec('npm run build', npm.file, [...npm.args, 'run', 'build'], electronRoot, NPM_TIMEOUT_MS)

  const outputRoot = await mkdtemp(join(tmpdir(), 'evohime-electron-build-'))
  try {
    deps.onStep?.('package')
    await exec(
      'electron-builder',
      npm.file,
      [
        ...npm.args,
        'exec',
        '--',
        'electron-builder',
        '--dir',
        '--config',
        'electron-builder.yml',
        `--config.directories.output=${outputRoot}`
      ],
      electronRoot,
      NPM_TIMEOUT_MS
    )

    return await assembleStaging(inputs, electronRoot, { ...deps, exists }, join(outputRoot, 'win-unpacked'))
  } finally {
    await withoutAsar(() => removeTreeResilient(outputRoot)).catch(() => undefined)
  }
}

/**
 * Collects the freshly built binaries and Electron payload into the staging
 * directory. The layout matches what the installer lays down, so the swap is a
 * plain file replacement.
 */
export async function assembleStaging(
  inputs: BuildInputs,
  electronRoot: string,
  deps: BuildDeps & { readonly exists: (path: string) => boolean },
  unpackedDirectory = join(electronRoot, 'release', 'win-unpacked')
): Promise<BuildMarker> {
  const cargoTarget = join(inputs.sourceDirectory, 'target', 'release')
  const unpacked = unpackedDirectory
  if (!deps.exists(unpacked)) {
    throw new BuildError('Electron package не собрался — каталог win-unpacked отсутствует.')
  }

  await withoutAsar(async () => {
    await mkdir(inputs.stagingDirectory, { recursive: true })
    await syncTree(unpacked, inputs.stagingDirectory)
  })

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
  await withoutAsar(async () => {
    for (const path of ['release', '.electron-cache', 'out']) {
      await removeTreeResilient(join(electronRoot, path))
    }
  })
}

/**
 * Brings `destination` to exactly the contents of `source`, copying only what
 * differs.
 *
 * Almost all of a package is the Electron runtime, which is unpacked from the
 * same cached archive on every build: copying it again moves hundreds of
 * megabytes past the antivirus to produce byte-identical files. Only the parts
 * the build actually regenerates — the asar payload and the patched executable —
 * change size or timestamp, so a size-and-mtime comparison is what separates
 * them. Files that are no longer produced are removed, so the staging directory
 * never accumulates leftovers from an older package.
 */
async function syncTree(source: string, destination: string): Promise<void> {
  const wanted = await readdir(source, { withFileTypes: true })
  const present = new Set(
    await readdir(destination).catch(() => [] as string[])
  )

  for (const entry of wanted) {
    present.delete(entry.name)
    const from = join(source, entry.name)
    const to = join(destination, entry.name)
    if (entry.isDirectory()) {
      await mkdir(to, { recursive: true })
      await syncTree(from, to)
      continue
    }
    if (await sameFile(from, to)) continue
    await rm(to, { force: true, maxRetries: CLEANUP_MAX_RETRIES, retryDelay: CLEANUP_RETRY_DELAY_MS })
    // Timestamps are carried over deliberately: they are half of what the next
    // build compares, and without them every file looks changed every time.
    await cp(from, to, { preserveTimestamps: true })
  }

  for (const stale of present) {
    await removeTreeResilient(join(destination, stale))
  }
}

/** Same size and modification time — rsync's test, and enough for build output. */
async function sameFile(source: string, destination: string): Promise<boolean> {
  try {
    const [from, to] = await Promise.all([stat(source), stat(destination)])
    return from.size === to.size && Math.abs(from.mtimeMs - to.mtimeMs) < 1
  } catch {
    return false
  }
}

/**
 * Fingerprint of the dependency set the next `npm ci` would install.
 *
 * `null` means "cannot tell" — an unreadable lockfile always reinstalls, because
 * building against stale `node_modules` is far worse than a wasted minute.
 */
async function lockfileDigest(electronRoot: string): Promise<string | null> {
  try {
    const lock = await readFile(join(electronRoot, 'package-lock.json'))
    return createHash('sha256').update(lock).digest('hex')
  } catch {
    return null
  }
}

/** Fingerprint recorded by the install that produced the current node_modules. */
async function installedDependencies(electronRoot: string): Promise<string | null> {
  try {
    const recorded = await readFile(join(electronRoot, DEPENDENCY_MARKER), 'utf8')
    return recorded.trim().length > 0 ? recorded.trim() : null
  } catch {
    return null
  }
}

/**
 * The marker lives inside `node_modules`, so it disappears together with the
 * tree it describes: a wiped or partially installed directory can never be
 * mistaken for a current one.
 */
async function recordDependencies(electronRoot: string, digest: string | null): Promise<void> {
  if (!digest) return
  try {
    await writeFile(join(electronRoot, DEPENDENCY_MARKER), `${digest}\n`, 'utf8')
  } catch {
    // Losing the marker only costs the next build a reinstall.
  }
}

/**
 * Runs a file operation with Electron's asar mapping switched off.
 *
 * The rebuild runs inside an Electron process, where `fs` presents every
 * `.asar` archive as a directory and keeps it open once touched. Copying the
 * freshly packaged `win-unpacked` then walks *into* `resources/app.asar`
 * instead of copying the file, leaves the archive locked, and the copy never
 * finishes — the update gate hangs forever with no output. `process.noAsar`
 * turns the archive back into an ordinary file for the duration of the call.
 */
async function withoutAsar<T>(operation: () => Promise<T>): Promise<T> {
  const previous = process.noAsar
  process.noAsar = true
  try {
    return await operation()
  } finally {
    process.noAsar = previous
  }
}

/**
 * Windows signing and antivirus processes can keep a build directory open for
 * a short time after electron-builder has exited. Node's recursive removal
 * supports bounded retries for exactly this class of transient failure; keep
 * the retry here so both normal staging and the clean rebuild use the same
 * behavior.
 */
async function removeTreeResilient(path: string): Promise<void> {
  await rm(path, {
    recursive: true,
    force: true,
    maxRetries: CLEANUP_MAX_RETRIES,
    retryDelay: CLEANUP_RETRY_DELAY_MS
  })
}

/** Refuse a configuration that would make the rebuild overwrite the running app. */
function assertBuildIsolation(sourceDirectory: string, installDirectory: string): void {
  const source = normalizePath(sourceDirectory)
  const install = normalizePath(installDirectory)
  if (source === install || source.startsWith(`${install}${sep}`) || install.startsWith(`${source}${sep}`)) {
    throw new BuildError(
      `Каталог исходников пересекается с установленным приложением: ${installDirectory}`
    )
  }
}

function normalizePath(path: string): string {
  return resolve(path).replace(/[\\/]+$/, '').toLowerCase()
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
