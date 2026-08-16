import { runCommand, type CommandRunner } from './run-command'

/**
 * Credential the update check sends to the GitHub API.
 *
 * Anonymous REST calls are capped at 60 per hour per IP address. One shared
 * address — a NAT, a VPN, a second client on the same machine — burns that
 * budget without the user doing anything, and the check then fails with `403`
 * and the client silently stops updating. The same calls with a token get 5000
 * per hour, so the updater looks for a credential the user already has instead
 * of asking for a new one.
 *
 * Nothing here is required: without a token the check works exactly as before.
 *
 * The token is only ever sent to `api.github.com` — every request URL is built
 * from the base that `githubApiBase` validated — and never logged: the shell's
 * redaction layer treats GitHub token shapes as secrets, and only the presence
 * of a token, never its value, is reported.
 */

/** `gh` is a local process; a hung invocation must not delay the launch gate. */
const GH_TIMEOUT_MS = 10_000

/** Explicit override, ahead of any ambient credential. */
const PRIMARY_VARIABLE = 'EVOHIME_UPDATE_GITHUB_TOKEN'

/** Ambient credentials: CI sets `GITHUB_TOKEN`, the gh CLI honours both. */
const AMBIENT_VARIABLES = ['GH_TOKEN', 'GITHUB_TOKEN'] as const

/**
 * Accepts the alphabet GitHub actually issues (`ghp_`, `gho_`, `github_pat_`,
 * classic 40-character hex) and nothing else: a value from a config file or a
 * child process must never reach a request header unchecked.
 */
export function normalizeGithubToken(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const candidate = value.trim()
  return /^[A-Za-z0-9_-]{20,255}$/.test(candidate) ? candidate : null
}

export type GithubTokenSource = 'environment' | 'config' | 'gh-cli'

export interface GithubToken {
  readonly token: string
  /** Reported in diagnostics so a wrong credential can be located. */
  readonly source: GithubTokenSource
}

export interface TokenLookup {
  /** `githubToken` from `update.json`. */
  readonly configured?: string | null
  readonly environment?: NodeJS.ProcessEnv
  /** Injected in tests; production spawns the real `gh`. */
  readonly run?: CommandRunner
}

/**
 * First usable credential, in order of how explicit it is: the dedicated
 * variable, the config file, an ambient token, and finally the gh CLI — which
 * is what a developer machine normally has and what makes the whole lookup
 * work without the user configuring anything.
 */
export async function resolveGithubToken(lookup: TokenLookup = {}): Promise<GithubToken | null> {
  const environment = lookup.environment ?? process.env

  const explicit = normalizeGithubToken(environment[PRIMARY_VARIABLE])
  if (explicit) return { token: explicit, source: 'environment' }

  const configured = normalizeGithubToken(lookup.configured)
  if (configured) return { token: configured, source: 'config' }

  for (const name of AMBIENT_VARIABLES) {
    const ambient = normalizeGithubToken(environment[name])
    if (ambient) return { token: ambient, source: 'environment' }
  }

  const cli = await readGhCliToken(lookup.run ?? runCommand)
  return cli ? { token: cli, source: 'gh-cli' } : null
}

/**
 * Token stored by `gh auth login`, if the CLI is installed and signed in.
 *
 * Every failure — no gh, not signed in, a hang — is simply "no token": the
 * check then runs anonymously, which is what it did before.
 */
async function readGhCliToken(run: CommandRunner): Promise<string | null> {
  let result
  try {
    result = await run({
      file: 'gh',
      args: ['auth', 'token'],
      timeoutMs: GH_TIMEOUT_MS,
      capture: true
    })
  } catch {
    return null
  }
  if (result.code !== 0) return null
  for (const line of result.raw) {
    const token = normalizeGithubToken(line)
    if (token) return token
  }
  return null
}
