import { existsSync } from 'node:fs'
import { mkdir } from 'node:fs/promises'
import { join } from 'node:path'

import { normalizeCommit } from './config'
import { describeFailure, runCommand, type CommandRunner } from './run-command'

/**
 * Git checkout the product rebuilds itself from.
 *
 * The checkout lives under `%LOCALAPPDATA%\EvoHime\source` and is owned by the
 * updater, not by the user: it is reset to `origin/<branch>` on every update, so
 * a local edit there is never carried into an installed build. Build caches
 * (`target`, `node_modules`, the Electron download cache) survive the reset —
 * they are what makes a local rebuild take minutes instead of an hour.
 */

const PRESERVED_PATHS = [
  'target',
  'desktop/evohime-electron/node_modules',
  'desktop/evohime-electron/.electron-cache',
  'desktop/evohime-electron/release',
  'desktop/evohime-electron/out'
] as const

const GIT_TIMEOUT_MS = 30 * 60_000
const LS_REMOTE_TIMEOUT_MS = 60_000

export interface CheckoutDeps {
  readonly git: string
  readonly cwd?: string
  readonly env?: NodeJS.ProcessEnv
  readonly run?: CommandRunner
  readonly exists?: (path: string) => boolean
  readonly onLine?: (line: string) => void
}

export interface CheckoutOptions {
  readonly directory: string
  readonly repositoryUrl: string
  readonly branch: string
}

export class SourceError extends Error {}

/** Newest commit of the tracked branch, read without touching the checkout. */
export async function readRemoteHead(
  options: CheckoutOptions,
  deps: CheckoutDeps
): Promise<string> {
  const result = await git(deps, options.directory, [
    'ls-remote',
    options.repositoryUrl,
    `refs/heads/${options.branch}`
  ], { timeoutMs: LS_REMOTE_TIMEOUT_MS, capture: true, allowMissingDirectory: true })
  const head = normalizeCommit(result.raw.at(0)?.split(/\s+/)[0])
  if (!head) {
    throw new SourceError(`Ветка ${options.branch} не найдена в удалённом репозитории.`)
  }
  return head
}

/** Commit the checkout currently sits on, or `null` when there is no checkout. */
export async function readLocalHead(
  options: CheckoutOptions,
  deps: CheckoutDeps
): Promise<string | null> {
  const exists = deps.exists ?? existsSync
  if (!exists(join(options.directory, '.git'))) return null
  const result = await git(deps, options.directory, ['rev-parse', 'HEAD'], { capture: true })
  return normalizeCommit(result.raw.at(-1))
}

/**
 * Brings the checkout to `commit`, cloning it first if needed, and returns the
 * commit that is actually checked out.
 */
export async function syncCheckout(
  options: CheckoutOptions,
  commit: string,
  deps: CheckoutDeps
): Promise<string> {
  const exists = deps.exists ?? existsSync
  const target = normalizeCommit(commit)
  if (!target) throw new SourceError('Некорректный идентификатор коммита.')

  if (!exists(join(options.directory, '.git'))) {
    await mkdir(options.directory, { recursive: true })
    await git(deps, options.directory, [
      'clone',
      '--branch',
      options.branch,
      options.repositoryUrl,
      options.directory
    ], { allowMissingDirectory: true })
  } else {
    // The remote may have been re-pointed by a reinstall; keep it authoritative.
    await git(deps, options.directory, ['remote', 'set-url', 'origin', options.repositoryUrl])
  }

  await git(deps, options.directory, ['fetch', '--prune', 'origin', options.branch])
  await git(deps, options.directory, ['checkout', '--detach', target])
  await git(deps, options.directory, ['reset', '--hard', target])
  await git(deps, options.directory, [
    'clean',
    '-ffd',
    ...PRESERVED_PATHS.flatMap((path) => ['-e', path])
  ])

  const head = await readLocalHead(options, deps)
  if (head !== target) {
    throw new SourceError('Checkout не встал на нужный коммит.')
  }
  return head
}

interface GitOptions {
  readonly timeoutMs?: number
  readonly capture?: boolean
  /** Clone and ls-remote run before the directory exists. */
  readonly allowMissingDirectory?: boolean
}

async function git(
  deps: CheckoutDeps,
  directory: string,
  args: readonly string[],
  options: GitOptions = {}
): ReturnType<CommandRunner> {
  const run = deps.run ?? runCommand
  const exists = deps.exists ?? existsSync
  const cwd = options.allowMissingDirectory && !exists(directory) ? (deps.cwd ?? process.cwd()) : directory
  const result = await run({
    file: deps.git,
    args,
    cwd,
    timeoutMs: options.timeoutMs ?? GIT_TIMEOUT_MS,
    ...(deps.env ? { env: deps.env } : {}),
    ...(options.capture ? { capture: true } : {}),
    ...(deps.onLine ? { onLine: deps.onLine } : {})
  })
  if (result.code !== 0) {
    throw new SourceError(describeFailure(`git ${args[0] ?? ''}`.trim(), result))
  }
  return result
}
