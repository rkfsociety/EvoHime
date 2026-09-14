import { createHash } from 'node:crypto'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

import { redactText } from '../diagnostics/redact'
import { githubApiBase } from './commit-status'
import type { UpdateConfig } from './config'
import type { UpdateStatus } from '@shared/update'

const MAX_REPORTS = 32
const REQUEST_TIMEOUT_MS = 15_000
const MARKER_PREFIX = 'evohime-update-error:'
const MAX_INLINE_SUPPORT_BUNDLE_BYTES = 40 * 1024
const MAX_SUPPORT_ISSUE_BODY_CHARS = 60_000

export interface UpdateIssueReporterDeps {
  readonly fetch?: typeof fetch
  readonly token?: string | null
  readonly now?: () => number
}

export interface SupportBundleReportInput {
  readonly archive: Uint8Array
  readonly issueDraft: string
}

/** Creates a user-triggered issue containing the redacted support archive. */
export async function reportSupportBundle(
  config: UpdateConfig,
  input: SupportBundleReportInput,
  deps: UpdateIssueReporterDeps = {}
): Promise<string> {
  const apiBase = githubApiBase(config.repositoryUrl)
  if (!apiBase) throw new Error('Репозиторий для support bundle не является GitHub-репозиторием.')
  if (!deps.token) throw new Error('GitHub-токен не найден. Выполни gh auth login и повтори отправку.')
  if (input.archive.byteLength > MAX_INLINE_SUPPORT_BUNDLE_BYTES) {
    throw new Error('Support bundle слишком большой для отправки в issue. Сначала сохрани ZIP и прикрепи его вручную.')
  }

  const archive = Buffer.from(input.archive)
  const archiveHash = createHash('sha256').update(archive).digest('hex')
  const body = [
    '<!-- evohime-support-bundle-v2 -->',
    '## Диагностический support bundle EvoHime',
    '',
    redactText(input.issueDraft).slice(0, 16_000),
    '',
    '### Архив',
    '',
    '- Размер: `' + archive.length + ' байт`',
    '- SHA-256: `' + archiveHash + '`',
    '- Содержимое ZIP redacted; credentials, raw prompts, workspace files и tool payloads исключены.',
    '',
    '<details><summary>ZIP archive (base64)</summary>',
    '',
    '```text',
    archive.toString('base64'),
    '```',
    '</details>',
    '',
    '_Issue создан по явному нажатию кнопки в EvoHime. Архив можно восстановить из base64-блока по SHA-256._'
  ].join('\n')
  if (body.length > MAX_SUPPORT_ISSUE_BODY_CHARS) {
    throw new Error('Support bundle не помещается в ограничение GitHub issue. Сохрани ZIP и прикрепи его вручную.')
  }

  const response = await (deps.fetch ?? globalThis.fetch)(`${apiBase}/issues`, {
    method: 'POST',
    headers: {
      accept: 'application/vnd.github+json',
      authorization: `Bearer ${deps.token}`,
      'content-type': 'application/json',
      'user-agent': 'EvoHime-Diagnostics',
      'x-github-api-version': '2022-11-28'
    },
    body: JSON.stringify({ title: '[support-bundle] EvoHime diagnostic report', body }),
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
  })
  if (!response.ok) throw new Error(`GitHub issue API ответил ${response.status}`)
  const result = await response.json() as { html_url?: unknown }
  if (typeof result.html_url !== 'string') throw new Error('GitHub не вернул ссылку на созданный issue.')
  return result.html_url
}

/** Reports a deduplicated, redacted update failure to the source repository. */
export async function reportUpdateFailure(
  config: UpdateConfig,
  status: UpdateStatus,
  deps: UpdateIssueReporterDeps = {}
): Promise<string | null> {
  const apiBase = githubApiBase(config.repositoryUrl)
  if (!apiBase || !deps.token) return null
  const message = redactText(status.error ?? status.message ?? 'unknown update failure')
  const fingerprint = createHash('sha256').update(`${status.phase}|${status.remoteCommit ?? ''}|${message}`).digest('hex').slice(0, 16)
  const marker = `${MARKER_PREFIX}${fingerprint}`
  const ledger = join(config.stateDirectory, 'reported-update-errors.json')
  const reported = readLedger(ledger)
  if (reported.includes(fingerprint)) return null

  const body = [
    '<!-- ' + marker + ' -->',
    '## Ошибка автоматического обновления EvoHime',
    '',
    `- Фаза: \`${status.phase}\``,
    `- Установленный commit: \`${safe(status.installedCommit)}\``,
    `- Целевой commit: \`${safe(status.remoteCommit)}\``,
    `- Компоненты: \`${safe((status.selectedComponents ?? []).join(', ') || 'full-installer')}\``,
    `- Платформа: \`${process.platform}/${process.arch}\``,
    `- Версия приложения: \`${safe(process.versions.electron ?? process.version)}\``,
    '',
    '### Ошибка',
    '',
    '```text', message, '```',
    '',
    '### Evidence',
    '',
    '```json',
    JSON.stringify((status.evidence ?? []).slice(-8).map((entry) => ({ phase: entry.phase, result: entry.result, commit: entry.commit, detail: redactText(entry.detail) })), null, 2),
    '```',
    '',
    '_Создано автоматически. Секреты, токены, пути, prompts и пользовательские данные намеренно не включаются._'
  ].join('\n')

  const response = await (deps.fetch ?? globalThis.fetch)(`${apiBase}/issues`, {
    method: 'POST',
    headers: {
      accept: 'application/vnd.github+json',
      authorization: `Bearer ${deps.token}`,
      'content-type': 'application/json',
      'user-agent': 'EvoHime-Updater',
      'x-github-api-version': '2022-11-28'
    },
    body: JSON.stringify({ title: `[auto-update] ${message.slice(0, 120)}`, body, labels: ['bug', 'auto-update'] }),
    signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS)
  })
  if (!response.ok) throw new Error(`GitHub issue API ответил ${response.status}`)
  const result = await response.json() as { html_url?: unknown }
  writeLedger(ledger, [...reported, fingerprint].slice(-MAX_REPORTS))
  return typeof result.html_url === 'string' ? result.html_url : null
}

function safe(value: unknown): string {
  return value == null ? 'unknown' : redactText(String(value)).replace(/[`\r\n]/g, ' ')
}

function readLedger(path: string): string[] {
  try {
    const value = JSON.parse(readFileSync(path, 'utf8'))
    return Array.isArray(value) ? value.filter((item): item is string => /^[a-f0-9]{16}$/.test(item)).slice(-MAX_REPORTS) : []
  } catch { return [] }
}

function writeLedger(path: string, values: string[]): void {
  mkdirSync(join(path, '..'), { recursive: true })
  writeFileSync(path, JSON.stringify(values), 'utf8')
}
