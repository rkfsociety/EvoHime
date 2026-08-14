import { normalizeCommit } from './config'

/**
 * CI verdict of a commit, read from the GitHub REST API.
 *
 * An update rebuilds whatever the branch points at, so a broken commit would be
 * compiled and installed on the user's machine. The client therefore only ever
 * moves to a commit whose checks finished green; a commit that is still running
 * is simply not ready yet, and the next check picks it up.
 *
 * Everything read here is untrusted network input: only object names that match
 * git's own format are accepted, arrays are bounded, and no field is used for
 * anything but the verdict.
 */

export type CommitCheckState = 'success' | 'pending' | 'failure' | 'unknown'

const REQUEST_TIMEOUT_MS = 15_000
const MAX_RESPONSE_ITEMS = 100

/** Conclusions that do not make a commit red — a skipped job is not a failure. */
const PASSING_CONCLUSIONS = new Set(['success', 'skipped', 'neutral'])

/**
 * A run that was cancelled never delivered a verdict — usually because a newer
 * push superseded it. That is not a failure, but it is not proof of health
 * either, so such a commit is `unknown`: never installed, and not reported to
 * the user as broken.
 */
const INCONCLUSIVE_CONCLUSIONS = new Set(['cancelled', 'stale', 'action_required'])

export interface CommitStatusDeps {
  readonly fetch?: typeof globalThis.fetch
}

/**
 * `https://github.com/owner/name.git` → `https://api.github.com/repos/owner/name`.
 * Returns `null` for anything that is not github.com, so a self-hosted remote
 * does not silently get its checks "verified" against the wrong host.
 */
export function githubApiBase(repositoryUrl: string): string | null {
  let url: URL
  try {
    url = new URL(repositoryUrl)
  } catch {
    return null
  }
  if (url.protocol !== 'https:' || (url.hostname !== 'github.com' && url.hostname !== 'www.github.com')) {
    return null
  }
  const segments = url.pathname.replace(/\.git$/, '').split('/').filter((part) => part.length > 0)
  const [owner, name] = segments
  if (segments.length !== 2 || !isRepositorySegment(owner) || !isRepositorySegment(name)) {
    return null
  }
  return `https://api.github.com/repos/${owner}/${name}`
}

/** Verdict of every check that ran for one commit. */
export async function readCommitState(
  apiBase: string,
  commit: string,
  deps: CommitStatusDeps = {}
): Promise<CommitCheckState> {
  const sha = normalizeCommit(commit)
  if (!sha) return 'unknown'

  const runs = await getJson(`${apiBase}/commits/${sha}/check-runs`, deps)
  const checkRuns = asArray(asRecord(runs)['check_runs'])
  if (checkRuns.length > 0) {
    return summarize(
      checkRuns.map((run) => {
        const record = asRecord(run)
        const conclusion = String(record['conclusion'] ?? '')
        return {
          finished: record['status'] === 'completed',
          passed: PASSING_CONCLUSIONS.has(conclusion),
          inconclusive: INCONCLUSIVE_CONCLUSIONS.has(conclusion)
        }
      })
    )
  }

  // A repository can still use the older commit-status API, and a commit with
  // no checks at all is unknown rather than green.
  const status = asRecord(await getJson(`${apiBase}/commits/${sha}/status`, deps))
  const state = status['state']
  if (state === 'success') return 'success'
  if (state === 'pending') return 'pending'
  if (state === 'failure' || state === 'error') return 'failure'
  return 'unknown'
}

export interface GreenCommit {
  /** Newest commit whose checks are green, or `null` when none qualifies. */
  readonly commit: string | null
  /** Verdict of the branch tip, so the UI can say why it is waiting. */
  readonly tipState: CommitCheckState
}

/**
 * Newest green commit on the branch, looking no further back than `depth`.
 *
 * Walking back matters: while CI runs on the tip, the commit before it is
 * usually already green, and refusing to update at all until the newest commit
 * finishes would leave the client stuck behind every push.
 */
export async function selectGreenCommit(
  apiBase: string,
  branch: string,
  depth: number,
  deps: CommitStatusDeps = {}
): Promise<GreenCommit> {
  const commits = await listRecentCommits(apiBase, branch, depth, deps)
  let tipState: CommitCheckState = 'unknown'

  for (const [index, commit] of commits.entries()) {
    const state = await readCommitState(apiBase, commit, deps)
    if (index === 0) tipState = state
    if (state === 'success') {
      return { commit, tipState }
    }
  }
  return { commit: null, tipState }
}

/** Newest commits of the branch, newest first. */
export async function listRecentCommits(
  apiBase: string,
  branch: string,
  depth: number,
  deps: CommitStatusDeps = {}
): Promise<readonly string[]> {
  const limit = Math.min(Math.max(1, Math.trunc(depth)), MAX_RESPONSE_ITEMS)
  const payload = await getJson(
    `${apiBase}/commits?sha=${encodeURIComponent(branch)}&per_page=${limit}`,
    deps
  )
  return asArray(payload)
    .slice(0, limit)
    .map((entry) => normalizeCommit(asRecord(entry)['sha']))
    .filter((sha): sha is string => sha !== null)
}

/**
 * Paths that cannot change the built product.
 *
 * Everything else counts as product code, so an unrecognised path always leads
 * to a rebuild: being wrong here means shipping a stale client, and that is the
 * costlier mistake.
 */
const NON_PRODUCT_PATTERNS = [
  /^docs\//,
  /^\.github\//,
  /\.md$/,
  /^\.gitignore$/,
  /^\.gitattributes$/,
  /^LICENSE$/
]

export function isProductPath(path: string): boolean {
  return !NON_PRODUCT_PATTERNS.some((pattern) => pattern.test(path))
}

/**
 * Whether anything between two commits can change the built client.
 *
 * A rebuild takes minutes of the user's machine, so a commit that only touched
 * documentation or plans is not worth one. The answer is `true` whenever it
 * cannot be established — a truncated diff, an unreadable range or an
 * unfamiliar path — because skipping a real change is worse than an extra
 * rebuild.
 */
export async function touchesProductCode(
  apiBase: string,
  base: string,
  head: string,
  deps: CommitStatusDeps = {}
): Promise<boolean> {
  const from = normalizeCommit(base)
  const to = normalizeCommit(head)
  if (!from || !to) return true
  if (from === to) return false

  const payload = asRecord(await getJson(`${apiBase}/compare/${from}...${to}`, deps))
  const files = payload['files']
  if (!Array.isArray(files) || files.length === 0) {
    return true
  }
  // The API caps the file list; a truncated diff cannot prove anything.
  if (files.length >= MAX_RESPONSE_ITEMS) {
    return true
  }
  return files.some((entry) => {
    const filename = asRecord(entry)['filename']
    return typeof filename !== 'string' || isProductPath(filename)
  })
}

async function getJson(url: string, deps: CommitStatusDeps): Promise<unknown> {
  const request = deps.fetch ?? globalThis.fetch
  const response = await request(url, {
    headers: {
      accept: 'application/vnd.github+json',
      'user-agent': 'EvoHime-Updater',
      'x-github-api-version': '2022-11-28'
    },
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
  })
  if (!response.ok) {
    throw new Error(`GitHub API ответил ${response.status}`)
  }
  return response.json()
}

function summarize(
  checks: readonly { finished: boolean; passed: boolean; inconclusive: boolean }[]
): CommitCheckState {
  if (checks.some((check) => check.finished && !check.passed && !check.inconclusive)) {
    return 'failure'
  }
  if (checks.some((check) => !check.finished)) return 'pending'
  if (checks.some((check) => check.inconclusive)) return 'unknown'
  return 'success'
}

function isRepositorySegment(value: string | undefined): value is string {
  return typeof value === 'string' && /^[A-Za-z0-9._-]{1,100}$/.test(value)
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

function asArray(value: unknown): readonly unknown[] {
  return Array.isArray(value) ? value.slice(0, MAX_RESPONSE_ITEMS) : []
}
