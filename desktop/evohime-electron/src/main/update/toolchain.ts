import { existsSync } from 'node:fs'
import { dirname, join } from 'node:path'

import { describeFailure, runCommand, type CommandRunner } from './run-command'

/**
 * Build toolchain required for a local rebuild.
 *
 * A source update is only possible when git, Node.js, the Rust toolchain and the
 * MSVC linker are present. Everything is discovered by absolute path first: a
 * freshly installed tool is not on the `PATH` this process inherited, and the
 * builder must be able to use it without asking the user to log out.
 *
 * Installation goes through winget with pinned package identifiers — never a
 * downloaded script and never a value taken from the update config.
 */

export const TOOL_IDS = ['git', 'node', 'rust', 'msvc'] as const

export type ToolId = (typeof TOOL_IDS)[number]

export interface ToolInvocation {
  readonly file: string
  readonly args: readonly string[]
}

export interface ToolStatus {
  readonly id: ToolId
  readonly label: string
  readonly available: boolean
  /** Resolved executable, present only when the tool was found. */
  readonly path: string | null
}

export interface ToolchainReport {
  readonly complete: boolean
  readonly tools: readonly ToolStatus[]
  /** Directories prepended to `PATH` for build commands. */
  readonly pathEntries: readonly string[]
}

export interface ToolchainDeps {
  readonly run?: CommandRunner
  readonly exists?: (path: string) => boolean
  readonly env?: NodeJS.ProcessEnv
}

interface ToolSpec {
  readonly id: ToolId
  readonly label: string
  /** Absolute candidates, checked before falling back to the `PATH` lookup. */
  readonly candidates: (env: NodeJS.ProcessEnv) => readonly string[]
  /** Bare command used when no candidate exists on disk. */
  readonly command: string | null
  readonly probeArgs: readonly string[]
  readonly wingetId: string
  readonly wingetExtraArgs?: readonly string[]
}

const PROBE_TIMEOUT_MS = 20_000
const INSTALL_TIMEOUT_MS = 45 * 60_000

const SPECS: readonly ToolSpec[] = [
  {
    id: 'git',
    label: 'Git',
    candidates: (env) => [
      join(env['ProgramFiles'] ?? 'C:\\Program Files', 'Git', 'cmd', 'git.exe'),
      join(env['LOCALAPPDATA'] ?? '', 'Programs', 'Git', 'cmd', 'git.exe')
    ],
    command: 'git',
    probeArgs: ['--version'],
    wingetId: 'Git.Git'
  },
  {
    id: 'node',
    label: 'Node.js 22',
    candidates: (env) => [
      join(env['ProgramFiles'] ?? 'C:\\Program Files', 'nodejs', 'node.exe'),
      join(env['LOCALAPPDATA'] ?? '', 'Programs', 'nodejs', 'node.exe')
    ],
    command: 'node',
    probeArgs: ['--version'],
    wingetId: 'OpenJS.NodeJS.LTS'
  },
  {
    id: 'rust',
    label: 'Rust (cargo)',
    candidates: (env) => [join(env['USERPROFILE'] ?? '', '.cargo', 'bin', 'cargo.exe')],
    command: 'cargo',
    probeArgs: ['--version'],
    wingetId: 'Rustup.Rustup'
  },
  {
    id: 'msvc',
    label: 'MSVC Build Tools',
    candidates: () => [],
    command: null,
    probeArgs: [],
    wingetId: 'Microsoft.VisualStudio.2022.BuildTools',
    wingetExtraArgs: [
      '--override',
      '--quiet --wait --norestart --nocache --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    ]
  }
]

export async function detectToolchain(deps: ToolchainDeps = {}): Promise<ToolchainReport> {
  const env = deps.env ?? process.env
  const exists = deps.exists ?? existsSync
  const run = deps.run ?? runCommand

  const tools: ToolStatus[] = []
  for (const spec of SPECS) {
    const path =
      spec.id === 'msvc'
        ? await locateMsvc(env, exists, run)
        : await locateTool(spec, env, exists, run)
    tools.push({ id: spec.id, label: spec.label, available: path !== null, path })
  }

  return {
    complete: tools.every((tool) => tool.available),
    tools,
    pathEntries: pathEntriesFor(tools)
  }
}

/**
 * Installs whatever is missing and re-detects. Already-present tools are never
 * touched, so a user-managed toolchain keeps working as it is.
 */
export async function ensureToolchain(
  deps: ToolchainDeps & { readonly onLine?: (line: string) => void } = {}
): Promise<{ readonly report: ToolchainReport; readonly error: string | null }> {
  const env = deps.env ?? process.env
  const run = deps.run ?? runCommand
  const report = await detectToolchain(deps)
  if (report.complete) return { report, error: null }

  const winget = await run({ file: 'winget', args: ['--version'], env, timeoutMs: PROBE_TIMEOUT_MS })
  if (winget.code !== 0) {
    return {
      report,
      error:
        'Не хватает инструментов сборки, а winget недоступен. Установи Git, Node.js 22, Rust и MSVC Build Tools вручную.'
    }
  }

  for (const tool of report.tools) {
    if (tool.available) continue
    const spec = SPECS.find((item) => item.id === tool.id)
    if (!spec) continue
    deps.onLine?.(`Устанавливаю ${spec.label}…`)
    const result = await run({
      file: 'winget',
      args: [
        'install',
        '--id',
        spec.wingetId,
        '--exact',
        '--source',
        'winget',
        '--silent',
        '--accept-package-agreements',
        '--accept-source-agreements',
        '--disable-interactivity',
        ...(spec.wingetExtraArgs ?? [])
      ],
      env,
      timeoutMs: INSTALL_TIMEOUT_MS,
      ...(deps.onLine ? { onLine: deps.onLine } : {})
    })
    // winget reports "already installed" as a non-zero code; the re-detection
    // below is the real verdict, so a failed install is not fatal on its own.
    if (result.code !== 0) deps.onLine?.(describeFailure(spec.label, result))
  }

  const verified = await detectToolchain(deps)
  if (verified.complete) return { report: verified, error: null }
  const missing = verified.tools
    .filter((tool) => !tool.available)
    .map((tool) => tool.label)
    .join(', ')
  return { report: verified, error: `Не удалось установить: ${missing}.` }
}

/** Build environment with the resolved toolchain directories in front of PATH. */
export function toolchainEnvironment(
  report: ToolchainReport,
  env: NodeJS.ProcessEnv = process.env
): NodeJS.ProcessEnv {
  const current = env['Path'] ?? env['PATH'] ?? ''
  const merged = [...report.pathEntries, ...current.split(';')]
    .map((entry) => entry.trim())
    .filter((entry, index, all) => entry.length > 0 && all.indexOf(entry) === index)
    .join(';')
  return { ...env, Path: merged, PATH: merged }
}

/** `npm` ships as a `.cmd`, which cannot be spawned without a shell — run its JS entry point. */
export function npmInvocation(
  report: ToolchainReport,
  exists: (path: string) => boolean = (path) => existsSync(path)
): ToolInvocation | null {
  const node = report.tools.find((tool) => tool.id === 'node')?.path
  if (!node) return null
  const candidates = [
    join(dirname(node), 'node_modules', 'npm', 'bin', 'npm-cli.js'),
    join(dirname(node), '..', 'lib', 'node_modules', 'npm', 'bin', 'npm-cli.js')
  ]
  const cli = candidates.find((candidate) => exists(candidate))
  return cli ? { file: node, args: [cli] } : null
}

export function toolPath(report: ToolchainReport, id: ToolId): string | null {
  return report.tools.find((tool) => tool.id === id)?.path ?? null
}

function pathEntriesFor(tools: readonly ToolStatus[]): readonly string[] {
  return tools
    .filter((tool) => tool.path !== null && tool.id !== 'msvc')
    .map((tool) => dirname(tool.path as string))
}

async function locateTool(
  spec: ToolSpec,
  env: NodeJS.ProcessEnv,
  exists: (path: string) => boolean,
  run: CommandRunner
): Promise<string | null> {
  for (const candidate of spec.candidates(env)) {
    if (candidate.length > 0 && exists(candidate)) return candidate
  }
  if (!spec.command) return null
  const probe = await run({
    file: spec.command,
    args: [...spec.probeArgs],
    env,
    timeoutMs: PROBE_TIMEOUT_MS
  })
  return probe.code === 0 ? spec.command : null
}

/**
 * The MSVC linker is found through vswhere, the only supported way to ask which
 * Visual Studio components are installed.
 */
async function locateMsvc(
  env: NodeJS.ProcessEnv,
  exists: (path: string) => boolean,
  run: CommandRunner
): Promise<string | null> {
  const vswhere = join(
    env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)',
    'Microsoft Visual Studio',
    'Installer',
    'vswhere.exe'
  )
  if (!exists(vswhere)) return null
  const result = await run({
    file: vswhere,
    args: [
      '-products',
      '*',
      '-latest',
      '-requires',
      'Microsoft.VisualStudio.Component.VC.Tools.x86.x64',
      '-property',
      'installationPath'
    ],
    env,
    timeoutMs: PROBE_TIMEOUT_MS
  })
  return result.code === 0 && result.tail.length > 0 ? vswhere : null
}
