import type { ChatRecord, CoreEvent } from '@shared/api'

import { filterEventsForChat } from './trace-filter'

export type SummaryTaskStatus = 'running' | 'completed' | 'failed' | 'cancelled'

export interface SummaryTaskItem {
  readonly taskId: string
  readonly status: SummaryTaskStatus
  readonly startedAt: string | null
  readonly finishedAt: string | null
  readonly error: string | null
}

export interface SummaryShellItem {
  readonly id: string
  readonly taskId: string
  readonly command: string
  readonly status: 'running' | 'completed'
  readonly output: string | null
  readonly startedAt: string | null
}

export interface SummaryTouchedFile {
  readonly path: string
  readonly action: 'read' | 'write'
  readonly count: number
}

export interface SummaryApprovalItem {
  readonly approvalId: string
  readonly toolName: string
  readonly permission: string
  readonly scope: string
}

export interface SummaryGoalItem {
  readonly goalId: string
  readonly objective: string
  readonly status: string
}

export interface ChatSummarySnapshot {
  readonly chatTitle: string
  readonly messageCount: number
  readonly taskCount: number
  readonly activeTaskId: string | null
  readonly latestTaskId: string | null
  readonly running: boolean
  readonly tasks: readonly SummaryTaskItem[]
  readonly modelName: string | null
  readonly providerName: string | null
  readonly inputTokens: number
  readonly outputTokens: number
  readonly contextUsedTokens: number | null
  readonly contextLimitTokens: number | null
  readonly contextPercent: number | null
  readonly shells: readonly SummaryShellItem[]
  readonly activeShellCount: number
  readonly touchedFiles: readonly SummaryTouchedFile[]
  readonly pendingApprovals: readonly SummaryApprovalItem[]
  readonly goal: SummaryGoalItem | null
}

export interface GitStatusFileItem {
  readonly path: string
  readonly status: string
}

export interface GitStatusSummary {
  readonly files: readonly GitStatusFileItem[]
  readonly rawOutput: string
  readonly truncated: boolean
}

export interface GitDiffSummary {
  readonly path: string | null
  readonly diff: string
  readonly truncated: boolean
}

export interface ContextLedgerEntry {
  readonly id: string
  readonly kind: string
  readonly title: string
  readonly tokens: number | null
  readonly pinned: boolean
}

export interface BackgroundRunItem {
  readonly runId: string
  readonly status: string
  readonly label: string
}

/**
 * Собирает сводку по текущему чату из локальной записи ChatRecord и ленты CoreEvent.
 */
export function buildChatSummary(
  events: readonly CoreEvent[],
  chat: ChatRecord | null
): ChatSummarySnapshot {
  const chatEvents = filterEventsForChat(events, chat)
  const chronological = [...chatEvents].sort((a, b) => a.sequenceId - b.sequenceId)

  const tasksMap = new Map<string, SummaryTaskItem>()
  const shells: SummaryShellItem[] = []
  const touchedMap = new Map<string, SummaryTouchedFile>()
  const approvalsMap = new Map<string, SummaryApprovalItem>()

  let modelName: string | null = null
  let providerName: string | null = null
  let inputTokens = 0
  let outputTokens = 0
  let contextUsedTokens: number | null = null
  let contextLimitTokens: number | null = null
  let goal = null as SummaryGoalItem | null

  for (const event of chronological) {
    const payload = parseJsonRecord(event.payload)
    const root = unwrapPayload(payload)
    const eventTimestamp =
      readString(root, ['timestamp', 'created_at']) ??
      readString(payload, ['timestamp', 'created_at'])

    switch (event.eventType) {
      case 'task.started': {
        if (event.taskId) {
          tasksMap.set(event.taskId, {
            taskId: event.taskId,
            status: 'running',
            startedAt: eventTimestamp,
            finishedAt: null,
            error: null
          })
        }
        const selectedModel = readString(root, ['model', 'model_id', 'model_name'])
        const selectedProvider = readString(root, ['provider', 'provider_id'])
        if (selectedModel) modelName = selectedModel
        if (selectedProvider) providerName = selectedProvider
        break
      }

      case 'task.completed': {
        if (event.taskId) {
          const prev = tasksMap.get(event.taskId)
          tasksMap.set(event.taskId, {
            taskId: event.taskId,
            status: 'completed',
            startedAt: prev?.startedAt ?? null,
            finishedAt: eventTimestamp,
            error: null
          })
          closeRunningShellsForTask(shells, event.taskId)
        }
        break
      }

      case 'task.failed': {
        if (event.taskId) {
          const prev = tasksMap.get(event.taskId)
          tasksMap.set(event.taskId, {
            taskId: event.taskId,
            status: 'failed',
            startedAt: prev?.startedAt ?? null,
            finishedAt: eventTimestamp,
            error: readString(root, ['error', 'message', 'reason']) ?? 'Ошибка выполнения'
          })
          closeRunningShellsForTask(shells, event.taskId)
        }
        break
      }

      case 'task.cancelled': {
        if (event.taskId) {
          const prev = tasksMap.get(event.taskId)
          tasksMap.set(event.taskId, {
            taskId: event.taskId,
            status: 'cancelled',
            startedAt: prev?.startedAt ?? null,
            finishedAt: eventTimestamp,
            error: null
          })
          closeRunningShellsForTask(shells, event.taskId)
        }
        break
      }

      case 'model.selected':
      case 'model.config':
      case 'model.routing': {
        const nextModel = readString(root, ['model', 'model_id', 'selected_model', 'model_name'])
        const nextProvider = readString(root, ['provider', 'provider_id', 'route'])
        if (nextModel) modelName = nextModel
        if (nextProvider) providerName = nextProvider
        break
      }

      case 'model.usage': {
        const inTok = readNumber(root, ['input_tokens', 'prompt_tokens'])
        const outTok = readNumber(root, ['output_tokens', 'completion_tokens'])
        if (inTok !== null) inputTokens += inTok
        if (outTok !== null) outputTokens += outTok
        break
      }

      case 'model.context': {
        const used = readNumber(root, ['estimated_tokens'])
        const limit = readNumber(root, ['context_limit_tokens'])
        if (used !== null && limit !== null && limit > 0) {
          contextUsedTokens = used
          contextLimitTokens = limit
        }
        break
      }

      case 'tool.started': {
        const toolName = readString(root, ['tool_name']) ?? ''
        if (isShellTool(toolName)) {
          shells.push({
            id: `${event.sequenceId}:${event.taskId}`,
            taskId: event.taskId,
            command: extractShellCommand(toolName, root),
            status: 'running',
            output: null,
            startedAt: eventTimestamp
          })
        }
        const fileTouch = extractTouchedFile(toolName, root)
        if (fileTouch) {
          const existing = touchedMap.get(fileTouch.path)
          touchedMap.set(fileTouch.path, {
            path: fileTouch.path,
            action: existing?.action === 'write' || fileTouch.action === 'write' ? 'write' : 'read',
            count: (existing?.count ?? 0) + 1
          })
        }
        break
      }

      case 'tool.output': {
        const toolName = readString(root, ['tool_name']) ?? ''
        if (isShellTool(toolName)) {
          const output = readString(root, ['output']) ?? ''
          const runningIdx = findLastRunningShellIndex(shells, event.taskId, toolName)
          if (runningIdx >= 0) {
            const current = shells[runningIdx]
            if (current) {
              shells[runningIdx] = {
                ...current,
                status: 'completed',
                output: clampText(output, 1_500)
              }
            }
          } else {
            shells.push({
              id: `${event.sequenceId}:${event.taskId}`,
              taskId: event.taskId,
              command: extractShellCommand(toolName, root),
              status: 'completed',
              output: clampText(output, 1_500),
              startedAt: eventTimestamp
            })
          }
        }
        break
      }

      case 'approval.required': {
        const approvalId = readString(root, ['approval_id'])
        if (approvalId) {
          approvalsMap.set(approvalId, {
            approvalId,
            toolName: readString(root, ['tool_name']) ?? 'операция Core',
            permission: readString(root, ['permission']) ?? 'требуется разрешение',
            scope: readString(root, ['scope']) ?? ''
          })
        }
        break
      }

      case 'approval.resolved': {
        const approvalId = readString(root, ['approval_id'])
        if (approvalId) approvalsMap.delete(approvalId)
        break
      }

      default: {
        if (event.eventType.startsWith('goal.')) {
          const goalId = readString(root, ['goal_id', 'id'])
          const objective = readString(root, ['objective', 'title'])
          const status = readString(root, ['status']) ?? event.eventType.replace('goal.', '')
          if (goalId && objective) {
            goal = { goalId, objective, status }
          } else if (goal && status) {
            goal = { goalId: goal.goalId, objective: goal.objective, status }
          }
        }
        break
      }
    }
  }

  const tasks = [...tasksMap.values()].reverse()
  const activeTask = tasks.find((task) => task.status === 'running') ?? null
  const latestTask = tasks[0] ?? null
  const reversedShells = [...shells].reverse()
  const activeShellCount = reversedShells.filter((item) => item.status === 'running').length

  const contextPercent =
    contextUsedTokens !== null && contextLimitTokens !== null && contextLimitTokens > 0
      ? Math.min(100, Math.round((contextUsedTokens / contextLimitTokens) * 100))
      : null

  return {
    chatTitle: chat?.title ?? 'Текущий чат',
    messageCount: chat?.messages.length ?? 0,
    taskCount: Math.max(chat?.taskIds.length ?? 0, tasks.length),
    activeTaskId: activeTask?.taskId ?? null,
    latestTaskId: latestTask?.taskId ?? chat?.taskIds.at(-1) ?? null,
    running: activeTask !== null,
    tasks,
    modelName,
    providerName,
    inputTokens,
    outputTokens,
    contextUsedTokens,
    contextLimitTokens,
    contextPercent,
    shells: reversedShells,
    activeShellCount,
    touchedFiles: [...touchedMap.values()].sort((a, b) => {
      if (a.action !== b.action) return a.action === 'write' ? -1 : 1
      return b.count - a.count
    }),
    pendingApprovals: [...approvalsMap.values()],
    goal
  }
}

export function parseGitStatusEvent(events: readonly CoreEvent[]): GitStatusSummary | null {
  const latest = events.find((event) => event.eventType === 'git.status')
  if (!latest) return null
  const record = parseJsonRecord(latest.payload)
  if (!record) return null

  const rawOutput = typeof record['output'] === 'string' ? record['output'] : ''
  const truncated = record['truncated'] === true
  const files: GitStatusFileItem[] = []

  const structured = record['structured']
  if (structured && typeof structured === 'object') {
    const entries = (structured as Record<string, unknown>)['files'] ?? (structured as Record<string, unknown>)['entries']
    if (Array.isArray(entries)) {
      for (const entry of entries) {
        if (!entry || typeof entry !== 'object') continue
        const item = entry as Record<string, unknown>
        const path = readString(item, ['path', 'file', 'relative_path'])
        const status = readString(item, ['status', 'xy', 'code']) ?? 'M'
        if (path) files.push({ path, status })
      }
    }
  }

  if (files.length === 0 && rawOutput.trim().length > 0) {
    for (const line of rawOutput.split(/\r?\n/)) {
      const match = line.trimEnd().match(/^\s*([MADRCU?!]{1,2})\s+(.+)$/)
      if (!match) continue
      const code = match[1]
      const path = match[2]?.trim() ?? ''
      if (code && path.length > 0) files.push({ path, status: code })
    }
  }

  return { files, rawOutput, truncated }
}

export function parseGitDiffEvent(events: readonly CoreEvent[]): GitDiffSummary | null {
  const latest = events.find((event) => event.eventType === 'git.diff')
  if (!latest) return null
  const record = parseJsonRecord(latest.payload)
  if (!record) return null

  const diff = typeof record['output'] === 'string' ? record['output'] : ''
  const truncated = record['truncated'] === true
  const structured = record['structured']
  const path = structured && typeof structured === 'object'
    ? readString(structured as Record<string, unknown>, ['path', 'relative_path'])
    : null

  return { path, diff, truncated }
}

export function parseContextLedgerEvent(
  events: readonly CoreEvent[],
  taskId: string | null
): readonly ContextLedgerEntry[] {
  const latest = events.find(
    (event) => event.eventType === 'context.ledger' && (!taskId || event.taskId === taskId || event.taskId === '')
  )
  if (!latest) return []
  const record = parseJsonRecord(latest.payload)
  if (!record) return []

  const itemsRaw = record['entries'] ?? record['items'] ?? record['records']
  if (!Array.isArray(itemsRaw)) return []

  const entries: ContextLedgerEntry[] = []
  for (let idx = 0; idx < itemsRaw.length; idx += 1) {
    const raw = itemsRaw[idx]
    if (!raw || typeof raw !== 'object') continue
    const item = raw as Record<string, unknown>
    const id = readString(item, ['id', 'entry_id']) ?? `ledger-${idx}`
    const kind = readString(item, ['kind', 'category', 'source']) ?? 'context'
    const title = readString(item, ['summary', 'title', 'label', 'content']) ?? id
    const tokens = readNumber(item, ['tokens', 'estimated_tokens', 'token_count'])
    const pinned = item['pinned'] === true
    entries.push({ id, kind, title: clampText(title, 180), tokens, pinned })
  }
  return entries
}

export function parseBackgroundRunsEvent(events: readonly CoreEvent[]): readonly BackgroundRunItem[] {
  const latest = events.find((event) => event.eventType === 'background_execution.result')
  if (!latest) return []
  const record = parseJsonRecord(latest.payload)
  if (!record) return []

  const runsRaw = record['runs'] ?? record['items']
  if (!Array.isArray(runsRaw)) return []

  const runs: BackgroundRunItem[] = []
  for (const raw of runsRaw) {
    if (!raw || typeof raw !== 'object') continue
    const item = raw as Record<string, unknown>
    const runId = readString(item, ['run_id', 'runId', 'id'])
    if (!runId) continue
    const status = readString(item, ['status', 'state']) ?? 'unknown'
    const label = readString(item, ['title', 'label', 'owner_scope', 'command']) ?? runId
    runs.push({ runId, status, label })
  }
  return runs
}

function isShellTool(toolName: string): boolean {
  return toolName === 'shell.execute' || toolName.startsWith('shell.execute:') || toolName === 'terminal.execute'
}

function extractShellCommand(toolName: string, root: Record<string, unknown> | null): string {
  if (toolName.startsWith('shell.execute: ')) {
    return toolName.slice('shell.execute: '.length).trim()
  }
  const program = readString(root, ['program', 'command', 'cmd'])
  if (program) {
    const argsRaw = root?.['args']
    if (Array.isArray(argsRaw)) {
      const args = argsRaw.filter((arg): arg is string => typeof arg === 'string')
      return [program, ...args].join(' ')
    }
    return program
  }
  const input = readString(root, ['input', 'arguments'])
  return input ? clampText(input, 120) : toolName
}

function extractTouchedFile(
  toolName: string,
  root: Record<string, unknown> | null
): { readonly path: string; readonly action: 'read' | 'write' } | null {
  const isWrite =
    toolName.startsWith('filesystem.write')
    || toolName.startsWith('filesystem.patch')
    || toolName.startsWith('filesystem.create')
    || toolName.startsWith('filesystem.delete')
    || toolName === 'apply_patch'
  const isRead = toolName.startsWith('filesystem.read')
  if (!isWrite && !isRead) return null

  const path = readString(root, ['path', 'relative_path', 'file_path', 'target_path'])
  if (!path) return null
  return { path, action: isWrite ? 'write' : 'read' }
}

function closeRunningShellsForTask(shells: SummaryShellItem[], taskId: string): void {
  for (let idx = 0; idx < shells.length; idx += 1) {
    const item = shells[idx]
    if (item && item.taskId === taskId && item.status === 'running') {
      shells[idx] = { ...item, status: 'completed' }
    }
  }
}

function findLastRunningShellIndex(
  shells: readonly SummaryShellItem[],
  taskId: string,
  toolName: string
): number {
  const normalizedCmd = toolName.startsWith('shell.execute: ')
    ? toolName.slice('shell.execute: '.length).trim()
    : null
  for (let idx = shells.length - 1; idx >= 0; idx -= 1) {
    const item = shells[idx]
    if (!item || item.status !== 'running' || item.taskId !== taskId) continue
    if (!normalizedCmd || item.command === normalizedCmd) return idx
  }
  return -1
}

function parseJsonRecord(payload: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(payload)
    return typeof parsed === 'object' && parsed !== null ? (parsed as Record<string, unknown>) : null
  } catch {
    return null
  }
}

function unwrapPayload(record: Record<string, unknown> | null): Record<string, unknown> | null {
  if (!record) return null
  for (const key of Object.keys(record)) {
    const value = record[key]
    if (value && typeof value === 'object' && !Array.isArray(value) && /^[A-Z]/.test(key)) {
      return value as Record<string, unknown>
    }
  }
  return record
}

function readString(record: Record<string, unknown> | null, keys: readonly string[]): string | null {
  if (!record) return null
  for (const key of keys) {
    const value = record[key]
    if (typeof value === 'string' && value.trim().length > 0) return value.trim()
  }
  return null
}

function readNumber(record: Record<string, unknown> | null, keys: readonly string[]): number | null {
  if (!record) return null
  for (const key of keys) {
    const value = record[key]
    if (typeof value === 'number' && Number.isFinite(value) && value >= 0) return value
  }
  return null
}

function clampText(text: string, maxLength: number): string {
  return text.length <= maxLength ? text : `${text.slice(0, maxLength - 1)}…`
}
