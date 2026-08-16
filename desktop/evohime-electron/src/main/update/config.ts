import { existsSync, readFileSync } from 'node:fs'
import { dirname, isAbsolute, join } from 'node:path'

import { normalizeGithubToken } from './github-token'

/**
 * Configuration of the source-based updater.
 *
 * The installer writes `%LOCALAPPDATA%\EvoHime\update.json`; anything missing
 * falls back to the compiled defaults so a hand-copied install still updates.
 * Environment overrides exist for development and CI only.
 */

export const UPDATE_CONFIG_FILE = 'update.json'

/** Repository the product rebuilds itself from. */
export const DEFAULT_REPOSITORY_URL = 'https://github.com/rkfsociety/EvoHime.git'
export const DEFAULT_BRANCH = 'main'

/**
 * What a launch does when the tracked branch moved ahead.
 *
 * `installer` downloads the installer produced by the green GitHub workflow.
 * `build` is retained as a development fallback. `apply-ready` only swaps a
 * package that a previous background run already staged.
 */
export type LaunchPolicy = 'installer' | 'build' | 'apply-ready' | 'off'

export interface UpdateConfig {
  readonly enabled: boolean
  readonly repositoryUrl: string
  readonly branch: string
  readonly launchPolicy: LaunchPolicy
  readonly checkIntervalMs: number
  /**
   * Only move to a commit whose CI checks are green. The rebuild happens on the
   * user's machine, so installing a commit that failed CI would compile a known
   * broken tree locally.
   */
  readonly requireGreenCommit: boolean
  /** How far back to look for a green commit while CI runs on the tip. */
  readonly greenCommitDepth: number
  /**
   * Credential for the GitHub API check. `null` is the normal case: the token
   * is then taken from the environment or the gh CLI, and the check falls back
   * to anonymous requests when neither has one.
   */
  readonly githubToken: string | null
  /** Git checkout the rebuild runs in. */
  readonly sourceDirectory: string
  /** Rebuilt package waiting to be swapped in. */
  readonly stagingDirectory: string
  /** Transaction journal and backups of `evohime-transaction.exe`. */
  readonly stateDirectory: string
  /** Directory holding the installed `EvoHime.exe`. */
  readonly installDirectory: string
}

const MIN_CHECK_INTERVAL_MS = 5 * 60_000
const DEFAULT_CHECK_INTERVAL_MS = 30 * 60_000
const DEFAULT_GREEN_DEPTH = 10
const MAX_CHECK_INTERVAL_MS = 24 * 60 * 60_000

export interface ConfigInputs {
  readonly dataDirectory: string
  readonly executablePath: string
  readonly environment?: NodeJS.ProcessEnv
  /** Injected in tests; production reads the config file from disk. */
  readonly readFile?: (path: string) => string | null
}

export function loadUpdateConfig(inputs: ConfigInputs): UpdateConfig {
  const environment = inputs.environment ?? process.env
  const read = inputs.readFile ?? defaultReadFile
  const file = asRecord(parseJson(read(join(inputs.dataDirectory, UPDATE_CONFIG_FILE))))

  const repositoryUrl =
    normalizeRepositoryUrl(environment['EVOHIME_UPDATE_REPOSITORY']) ??
    normalizeRepositoryUrl(file['repositoryUrl']) ??
    DEFAULT_REPOSITORY_URL
  const branch =
    normalizeBranch(environment['EVOHIME_UPDATE_BRANCH']) ??
    normalizeBranch(file['branch']) ??
    DEFAULT_BRANCH
  const configuredPolicy = normalizeLaunchPolicy(file['launchPolicy'])
  const launchPolicy =
    normalizeLaunchPolicy(environment['EVOHIME_UPDATE_LAUNCH_POLICY']) ??
    // Version 1 installers configured local rebuilding. Migrate those
    // installations automatically; an environment override still permits
    // developers to opt into the old path explicitly.
    (file['version'] === 1 && configuredPolicy === 'build' ? 'installer' : configuredPolicy) ??
    'installer'
  const enabled =
    normalizeBoolean(environment['EVOHIME_UPDATE_ENABLED']) ?? normalizeBoolean(file['enabled']) ?? true

  return {
    enabled,
    repositoryUrl,
    branch,
    launchPolicy: enabled ? launchPolicy : 'off',
    checkIntervalMs: normalizeInterval(file['checkIntervalMinutes']),
    requireGreenCommit:
      normalizeBoolean(environment['EVOHIME_UPDATE_REQUIRE_GREEN']) ??
      normalizeBoolean(file['requireGreenCommit']) ??
      true,
    greenCommitDepth: normalizeDepth(file['greenCommitDepth']),
    githubToken: normalizeGithubToken(file['githubToken']),
    sourceDirectory:
      absoluteOr(environment['EVOHIME_UPDATE_SOURCE_DIR'], join(inputs.dataDirectory, 'source')),
    stagingDirectory:
      absoluteOr(environment['EVOHIME_UPDATE_STAGING_DIR'], join(inputs.dataDirectory, 'update-staging')),
    stateDirectory: join(inputs.dataDirectory, 'update-state'),
    installDirectory:
      absoluteOr(environment['EVOHIME_UPDATE_INSTALL_DIR'], dirname(inputs.executablePath))
  }
}

/** Marker the rebuild writes next to the binaries so a launch knows its commit. */
export const BUILD_MARKER_FILE = 'evohime.build.json'

export interface BuildMarker {
  readonly commit: string
  readonly branch: string
  readonly builtAtMs: number
}

export function readBuildMarker(
  directory: string,
  read: (path: string) => string | null = defaultReadFile
): BuildMarker | null {
  const value = asRecord(parseJson(read(join(directory, BUILD_MARKER_FILE))))
  const commit = normalizeCommit(value['commit'])
  if (!commit) return null
  return {
    commit,
    branch: normalizeBranch(value['branch']) ?? DEFAULT_BRANCH,
    builtAtMs: typeof value['builtAtMs'] === 'number' ? value['builtAtMs'] : 0
  }
}

/** Accepts the 40-character object names git prints, and nothing else. */
export function normalizeCommit(value: unknown): string | null {
  return typeof value === 'string' && /^[0-9a-f]{40}$/.test(value.trim()) ? value.trim() : null
}

/**
 * Only https git remotes are accepted: an `ssh://`, `file://` or `ext::` remote
 * would let a writable config file point the rebuild at attacker-controlled
 * sources or run a helper command.
 */
export function normalizeRepositoryUrl(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const candidate = value.trim()
  if (candidate.length === 0 || candidate.length > 2_048) return null
  try {
    const url = new URL(candidate)
    return url.protocol === 'https:' ? url.toString() : null
  } catch {
    return null
  }
}

export function normalizeBranch(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const candidate = value.trim()
  return /^[A-Za-z0-9][A-Za-z0-9._\-/]{0,100}$/.test(candidate) && !candidate.includes('..')
    ? candidate
    : null
}

function normalizeLaunchPolicy(value: unknown): LaunchPolicy | null {
  return value === 'installer' || value === 'build' || value === 'apply-ready' || value === 'off' ? value : null
}

function normalizeBoolean(value: unknown): boolean | null {
  if (typeof value === 'boolean') return value
  if (value === '1' || value === 'true') return true
  if (value === '0' || value === 'false') return false
  return null
}

function normalizeInterval(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_CHECK_INTERVAL_MS
  return Math.min(MAX_CHECK_INTERVAL_MS, Math.max(MIN_CHECK_INTERVAL_MS, Math.round(value) * 60_000))
}

/** Bounded so a config file cannot turn one check into a hundred API calls. */
function normalizeDepth(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_GREEN_DEPTH
  return Math.min(30, Math.max(1, Math.round(value)))
}

function absoluteOr(value: unknown, fallback: string): string {
  return typeof value === 'string' && value.trim().length > 0 && isAbsolute(value.trim())
    ? value.trim()
    : fallback
}

function parseJson(text: string | null): unknown {
  if (text === null) return null
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

function defaultReadFile(path: string): string | null {
  try {
    return existsSync(path) ? readFileSync(path, 'utf8') : null
  } catch {
    return null
  }
}
