import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'

import type { ChatMessage, ChatRecord, ChatSummary, WorkbenchPresentation } from '@shared/api'

import { normalizeWorkspacePath } from './workspace-store'

/**
 * Conversations of the shell, grouped by workspace.
 *
 * A chat is a UI grouping, not agent state: it remembers which tasks were run
 * from it and what the user typed, so reopening it shows the conversation
 * again. Core stays the authority for the tasks themselves and re-validates
 * every command; nothing here grants access to anything.
 */

export const MAX_CHATS_PER_WORKSPACE = 100
export const MAX_MESSAGES_PER_CHAT = 500
export const MAX_PROMPT_CHARS = 4_096
export const MAX_TITLE_CHARS = 80
const STORE_VERSION = 1

interface StoredDocument {
  readonly chats: readonly ChatRecord[]
}

const EMPTY: StoredDocument = { chats: [] }
const DEFAULT_WORKBENCH: WorkbenchPresentation = { activeTab: 'tasks', splitRatio: 0.5, collapsed: false }

/** First line of the prompt, bounded — enough to recognise a chat in a list. */
export function titleFromPrompt(prompt: string): string {
  const line = prompt.trim().split('\n')[0]?.trim() ?? ''
  if (line.length === 0) return 'Без названия'
  return line.length > MAX_TITLE_CHARS ? `${line.slice(0, MAX_TITLE_CHARS)}…` : line
}

export class ChatStore {
  constructor(
    private readonly filePath: string,
    private readonly now: () => number = Date.now,
    private readonly newId: () => string = () => globalThis.crypto.randomUUID()
  ) {}

  static defaultPath(dataDirectory: string): string {
    return join(dataDirectory, 'shell', 'chats.json')
  }

  /** Chats of one workspace, or standalone chats, most recently used first. */
  list(workspacePath: string | null): ChatSummary[] {
    const normalized = normalizeChatWorkspacePath(workspacePath)
    if (normalized === undefined) return []
    return this.read()
      .chats.filter((chat) => samePath(chat.workspacePath, normalized))
      .sort((left, right) => right.updatedMs - left.updatedMs)
      .map(summarize)
  }

  create(workspacePath: string | null): ChatRecord | null {
    const normalized = normalizeChatWorkspacePath(workspacePath)
    if (normalized === undefined) return null
    const stamp = this.now()
    const chat: ChatRecord = {
      id: this.newId(),
      workspacePath: normalized,
      title: 'Новый чат',
      createdMs: stamp,
      updatedMs: stamp,
      taskIds: [],
      messages: []
      , workbenchPresentation: DEFAULT_WORKBENCH
    }
    const current = this.read().chats
    const siblings = current.filter((item) => samePath(item.workspacePath, normalized))
    // Oldest chats of this workspace fall off rather than growing without end.
    const dropped =
      siblings.length >= MAX_CHATS_PER_WORKSPACE
        ? new Set(
            [...siblings]
              .sort((left, right) => left.updatedMs - right.updatedMs)
              .slice(0, siblings.length - MAX_CHATS_PER_WORKSPACE + 1)
              .map((item) => item.id)
          )
        : new Set<string>()
    this.write({ chats: [chat, ...current.filter((item) => !dropped.has(item.id))] })
    return chat
  }

  open(chatId: string): ChatRecord | null {
    return this.read().chats.find((chat) => chat.id === chatId) ?? null
  }

  /** Records a prompt and the task it started, naming the chat on first use. */
  appendPrompt(chatId: string, taskId: string, prompt: string, clientMessageId?: string): ChatRecord | null {
    const text = prompt.trim().slice(0, MAX_PROMPT_CHARS)
    if (text.length === 0) return null
    const current = this.read().chats
    const chat = current.find((item) => item.id === chatId)
    if (!chat) return null
    const message: ChatMessage = {
      taskId,
      clientMessageId: clientMessageId?.slice(0, 128) || taskId,
      prompt: text,
      atMs: this.now()
    }
    const next: ChatRecord = {
      ...chat,
      title: chat.messages.length === 0 ? titleFromPrompt(text) : chat.title,
      updatedMs: message.atMs,
      taskIds: [...new Set([...chat.taskIds, taskId])].slice(-MAX_MESSAGES_PER_CHAT),
      messages: [...chat.messages, message].slice(-MAX_MESSAGES_PER_CHAT)
    }
    this.write({ chats: current.map((item) => (item.id === chatId ? next : item)) })
    return next
  }

  remove(chatId: string): void {
    const current = this.read().chats
    this.write({ chats: current.filter((chat) => chat.id !== chatId) })
  }

  getWorkbenchPresentation(chatId: string): WorkbenchPresentation {
    return this.open(chatId)?.workbenchPresentation ?? DEFAULT_WORKBENCH
  }

  saveWorkbenchPresentation(chatId: string, value: WorkbenchPresentation): WorkbenchPresentation {
    const current = this.read().chats
    const chat = current.find((item) => item.id === chatId)
    if (!chat) return DEFAULT_WORKBENCH
    const presentation: WorkbenchPresentation = {
      activeTab: ['files', 'diff', 'tasks', 'terminal', 'browser', 'usage'].includes(value.activeTab) ? value.activeTab : 'tasks',
      splitRatio: Number.isFinite(value.splitRatio) ? Math.min(0.8, Math.max(0.2, value.splitRatio)) : 0.5,
      collapsed: Boolean(value.collapsed)
    }
    this.write({ chats: current.map((item) => item.id === chatId ? { ...item, workbenchPresentation: presentation } : item) })
    return presentation
  }

  /** Drops every chat of a workspace the user stopped tracking. */
  removeWorkspace(workspacePath: string): void {
    const normalized = normalizeWorkspacePath(workspacePath)
    if (normalized === null) return
    const current = this.read().chats
    this.write({ chats: current.filter((chat) => !samePath(chat.workspacePath, normalized)) })
  }

  private read(): StoredDocument {
    let raw: string
    try {
      raw = readFileSync(this.filePath, 'utf8')
    } catch {
      return EMPTY
    }
    let parsed: unknown
    try {
      parsed = JSON.parse(raw)
    } catch {
      // A corrupt file must not take the shell down; the user starts empty.
      return EMPTY
    }
    if (typeof parsed !== 'object' || parsed === null) return EMPTY
    const list = (parsed as Record<string, unknown>)['chats']
    if (!Array.isArray(list)) return EMPTY
    const chats: ChatRecord[] = []
    for (const item of list) {
      const chat = parseChat(item)
      if (chat) chats.push(chat)
    }
    return { chats }
  }

  private write(document: StoredDocument): void {
    mkdirSync(dirname(this.filePath), { recursive: true })
    const temporary = `${this.filePath}.tmp`
    writeFileSync(temporary, JSON.stringify({ version: STORE_VERSION, ...document }), 'utf8')
    renameSync(temporary, this.filePath)
  }
}

function summarize(chat: ChatRecord): ChatSummary {
  return {
    id: chat.id,
    workspacePath: chat.workspacePath,
    title: chat.title,
    updatedMs: chat.updatedMs,
    messageCount: chat.messages.length
  }
}

function parseChat(value: unknown): ChatRecord | null {
  if (typeof value !== 'object' || value === null) return null
  const record = value as Record<string, unknown>
  const id = typeof record['id'] === 'string' ? record['id'] : ''
  const workspacePath = normalizeChatWorkspacePath(record['workspacePath'])
  if (id.length === 0 || workspacePath === undefined) return null
  const messages: ChatMessage[] = []
  for (const item of Array.isArray(record['messages']) ? record['messages'] : []) {
    if (typeof item !== 'object' || item === null) continue
    const message = item as Record<string, unknown>
    const prompt = typeof message['prompt'] === 'string' ? message['prompt'] : ''
    const taskId = typeof message['taskId'] === 'string' ? message['taskId'] : ''
    if (prompt.length === 0 || taskId.length === 0) continue
    messages.push({
      taskId,
      clientMessageId:
        typeof message['clientMessageId'] === 'string' && message['clientMessageId'].length > 0
          ? message['clientMessageId'].slice(0, 128)
          : taskId,
      prompt: prompt.slice(0, MAX_PROMPT_CHARS),
      atMs: numeric(message['atMs'])
    })
    if (messages.length >= MAX_MESSAGES_PER_CHAT) break
  }
  const taskIds = Array.isArray(record['taskIds'])
    ? record['taskIds'].filter((item): item is string => typeof item === 'string')
    : []
  return {
    id,
    workspacePath,
    title:
      typeof record['title'] === 'string' && record['title'].length > 0
        ? record['title'].slice(0, MAX_TITLE_CHARS)
        : 'Без названия',
    createdMs: numeric(record['createdMs']),
    updatedMs: numeric(record['updatedMs']),
    taskIds: taskIds.slice(-MAX_MESSAGES_PER_CHAT),
      messages
      , workbenchPresentation: parseWorkbenchPresentation(record['workbenchPresentation'])
  }
}

function parseWorkbenchPresentation(value: unknown): WorkbenchPresentation {
  if (typeof value !== 'object' || value === null) return DEFAULT_WORKBENCH
  const record = value as Record<string, unknown>
  return {
    activeTab: typeof record['activeTab'] === 'string' && ['files', 'diff', 'tasks', 'terminal', 'browser', 'usage'].includes(record['activeTab']) ? record['activeTab'] : 'tasks',
    splitRatio: typeof record['splitRatio'] === 'number' && Number.isFinite(record['splitRatio']) ? Math.min(0.8, Math.max(0.2, record['splitRatio'])) : 0.5,
    collapsed: record['collapsed'] === true
  }
}

function numeric(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : 0
}

function normalizeChatWorkspacePath(value: unknown): string | null | undefined {
  if (value === null) return null
  return normalizeWorkspacePath(value) ?? undefined
}

function samePath(left: string | null, right: string | null): boolean {
  if (left === null || right === null) return left === right
  // Windows paths are case-insensitive; raw comparison would split one
  // workspace into two.
  return left.toLowerCase() === right.toLowerCase()
}
