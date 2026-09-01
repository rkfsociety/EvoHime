import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { ChatStore, MAX_CHATS_PER_WORKSPACE, titleFromPrompt } from '../src/main/chat-store'

/**
 * Chats are a UI grouping owned by the shell: they remember which tasks were
 * run from a conversation so reopening it shows the same transcript. These
 * tests pin the scoping rules and the bounds that keep the file from growing
 * without end.
 */

const directories: string[] = []

function storePath(): string {
  const directory = mkdtempSync(join(tmpdir(), 'evohime-chats-'))
  directories.push(directory)
  return join(directory, 'chats.json')
}

function newStore(path = storePath()): ChatStore {
  let clock = 0
  let counter = 0
  return new ChatStore(
    path,
    () => (clock += 1),
    () => `chat-${(counter += 1)}`
  )
}

afterEach(() => {
  for (const directory of directories.splice(0)) {
    rmSync(directory, { recursive: true, force: true })
  }
})

describe('chat store', () => {
  it('keeps chats of different projects apart', () => {
    const store = newStore()
    store.create('C:\\work\\alpha')
    store.create('C:\\work\\beta')

    expect(store.list('C:\\work\\alpha')).toHaveLength(1)
    expect(store.list('C:\\work\\beta')).toHaveLength(1)
    // Windows paths are case-insensitive: one project, not two.
    expect(store.list('c:\\work\\ALPHA')).toHaveLength(1)
  })

  it('names a chat after its first prompt and remembers the task', () => {
    const store = newStore()
    const chat = store.create('C:\\work\\repo')

    const updated = store.appendPrompt(chat!.id, 'task-1', 'Изучи проект\nи расскажи о нём')

    expect(updated?.title).toBe('Изучи проект')
    expect(updated?.taskIds).toEqual(['task-1'])
    expect(updated?.messages).toHaveLength(1)

    // A later prompt joins the same chat without renaming it.
    const second = store.appendPrompt(chat!.id, 'task-2', 'Почини тесты')
    expect(second?.title).toBe('Изучи проект')
    expect(second?.taskIds).toEqual(['task-1', 'task-2'])
  })

  it('orders the list by last use', () => {
    const store = newStore()
    const first = store.create('C:\\work\\repo')
    const second = store.create('C:\\work\\repo')
    store.appendPrompt(first!.id, 'task-1', 'Снова первый')

    expect(store.list('C:\\work\\repo').map((chat) => chat.id)).toEqual([first!.id, second!.id])
  })

  it('drops the oldest chat once the project is full', () => {
    const store = newStore()
    const created = Array.from({ length: MAX_CHATS_PER_WORKSPACE }, () =>
      store.create('C:\\work\\repo')
    )
    store.create('C:\\work\\repo')

    const ids = new Set(store.list('C:\\work\\repo').map((chat) => chat.id))
    expect(ids.size).toBe(MAX_CHATS_PER_WORKSPACE)
    expect(ids.has(created[0]!.id)).toBe(false)
  })

  it('removes a chat and every chat of a forgotten project', () => {
    const store = newStore()
    const kept = store.create('C:\\work\\alpha')
    const dropped = store.create('C:\\work\\alpha')
    store.create('C:\\work\\beta')

    store.remove(dropped!.id)
    expect(store.list('C:\\work\\alpha').map((chat) => chat.id)).toEqual([kept!.id])

    store.removeWorkspace('C:\\work\\alpha')
    expect(store.list('C:\\work\\alpha')).toEqual([])
    expect(store.list('C:\\work\\beta')).toHaveLength(1)
  })

  it('refuses a relative project path', () => {
    const store = newStore()
    expect(store.create('..\\elsewhere')).toBeNull()
    expect(store.list('..\\elsewhere')).toEqual([])
  })

  it('starts empty instead of failing on a corrupt file', () => {
    const path = storePath()
    writeFileSync(path, '{ not json', 'utf8')

    expect(newStore(path).list('C:\\work\\repo')).toEqual([])
  })

  it('bounds a title taken from a long prompt', () => {
    expect(titleFromPrompt('x'.repeat(200)).length).toBeLessThanOrEqual(81)
    expect(titleFromPrompt('   ')).toBe('Без названия')
  })

  it('keeps bounded Workbench presentation state per conversation', () => {
    const store = newStore()
    const first = store.create('C:\\work\\repo')!
    const second = store.create('C:\\work\\repo')!

    expect(store.getWorkbenchPresentation(first.id)).toEqual({ activeTab: 'tasks', splitRatio: 0.5, collapsed: false })
    expect(store.saveWorkbenchPresentation(first.id, { activeTab: 'usage', splitRatio: 9, collapsed: true })).toEqual({ activeTab: 'usage', splitRatio: 0.8, collapsed: true })
    expect(store.getWorkbenchPresentation(first.id).activeTab).toBe('usage')
    expect(store.getWorkbenchPresentation(second.id).activeTab).toBe('tasks')
    expect(store.saveWorkbenchPresentation(first.id, { activeTab: 'secret', splitRatio: -1, collapsed: false })).toEqual({ activeTab: 'tasks', splitRatio: 0.2, collapsed: false })
  })
})
