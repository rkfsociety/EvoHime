import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'

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
  it('keeps chats of different projects apart', async () => {
    const store = newStore()
    await store.create('C:\\work\\alpha')
    await store.create('C:\\work\\beta')

    expect(await store.list('C:\\work\\alpha')).toHaveLength(1)
    expect(await store.list('C:\\work\\beta')).toHaveLength(1)
    // Windows paths are case-insensitive: one project, not two.
    expect(await store.list('c:\\work\\ALPHA')).toHaveLength(1)
  })

  it('keeps standalone chats separate from project chats', async () => {
    const store = newStore()
    const standalone = (await store.create(null))!
    await store.create('C:\\work\\repo')

    expect((await store.list(null)).map((chat) => chat.id)).toEqual([standalone.id])
    expect(await store.list('C:\\work\\repo')).toHaveLength(1)
    expect(standalone.workspacePath).toBeNull()
  })

  it('names a chat after its first prompt and remembers the task', async () => {
    const store = newStore()
    const chat = await store.create('C:\\work\\repo')

    const updated = await store.appendPrompt(chat!.id, 'task-1', 'Изучи проект\nи расскажи о нём')

    expect(updated?.title).toBe('Изучи проект')
    expect(updated?.taskIds).toEqual(['task-1'])
    expect(updated?.messages).toHaveLength(1)

    // A later prompt joins the same chat without renaming it.
    const second = await store.appendPrompt(chat!.id, 'task-2', 'Почини тесты')
    expect(second?.title).toBe('Изучи проект')
    expect(second?.taskIds).toEqual(['task-1', 'task-2'])
  })

  it('orders the list by last use', async () => {
    const store = newStore()
    const first = await store.create('C:\\work\\repo')
    const second = await store.create('C:\\work\\repo')
    await store.appendPrompt(first!.id, 'task-1', 'Снова первый')

    expect((await store.list('C:\\work\\repo')).map((chat) => chat.id)).toEqual([first!.id, second!.id])
  })

  it('drops the oldest chat once the project is full', async () => {
    const store = newStore()
    const created = await Promise.all(Array.from({ length: MAX_CHATS_PER_WORKSPACE }, () =>
      store.create('C:\\work\\repo')
    ))
    await store.create('C:\\work\\repo')

    const ids = new Set((await store.list('C:\\work\\repo')).map((chat) => chat.id))
    expect(ids.size).toBe(MAX_CHATS_PER_WORKSPACE)
    expect(ids.has(created[0]!.id)).toBe(false)
  })

  it('removes a chat and every chat of a forgotten project', async () => {
    const store = newStore()
    const kept = await store.create('C:\\work\\alpha')
    const dropped = await store.create('C:\\work\\alpha')
    await store.create('C:\\work\\beta')

    await store.remove(dropped!.id)
    expect((await store.list('C:\\work\\alpha')).map((chat) => chat.id)).toEqual([kept!.id])

    await store.removeWorkspace('C:\\work\\alpha')
    expect(await store.list('C:\\work\\alpha')).toEqual([])
    expect(await store.list('C:\\work\\beta')).toHaveLength(1)
  })

  it('refuses a relative project path', async () => {
    const store = newStore()
    expect(await store.create('..\\elsewhere')).toBeNull()
    expect(await store.list('..\\elsewhere')).toEqual([])
  })

  it('treats a missing file as empty and creates it on the first mutation', async () => {
    const path = storePath()
    const store = newStore(path)

    expect(await store.list('C:\\work\\repo')).toEqual([])
    await store.create('C:\\work\\repo')

    expect(JSON.parse(readFileSync(path, 'utf8')).version).toBe(1)
  })

  it('preserves a corrupt file and rejects reads and mutations', async () => {
    const path = storePath()
    const original = '{ not json'
    writeFileSync(path, original, 'utf8')
    const store = newStore(path)

    await expect(store.list('C:\\work\\repo')).rejects.toMatchObject({ kind: 'corrupt' })

    const recoveryFiles = readdirSync(dirname(path)).filter((name) => name.startsWith('chats.json.corrupt-'))
    expect(recoveryFiles).toHaveLength(1)
    expect(readFileSync(join(dirname(path), recoveryFiles[0]!), 'utf8')).toBe(original)
    await expect(store.create('C:\\work\\repo')).rejects.toMatchObject({ kind: 'corrupt' })
    expect(readFileSync(path, 'utf8')).toBe(original)
  })

  it('bounds a title taken from a long prompt', () => {
    expect(titleFromPrompt('x'.repeat(200)).length).toBeLessThanOrEqual(81)
    expect(titleFromPrompt('   ')).toBe('Без названия')
  })

  it('keeps bounded Workbench presentation state per conversation', async () => {
    const store = newStore()
    const first = (await store.create('C:\\work\\repo'))!
    const second = (await store.create('C:\\work\\repo'))!

    expect(await store.getWorkbenchPresentation(first.id)).toEqual({ activeTab: 'tasks', splitRatio: 0.5, collapsed: false })
    expect(await store.saveWorkbenchPresentation(first.id, { activeTab: 'usage', splitRatio: 9, collapsed: true })).toEqual({ activeTab: 'usage', splitRatio: 0.8, collapsed: true })
    expect((await store.getWorkbenchPresentation(first.id)).activeTab).toBe('usage')
    expect((await store.getWorkbenchPresentation(second.id)).activeTab).toBe('tasks')
    expect(await store.saveWorkbenchPresentation(first.id, { activeTab: 'secret', splitRatio: -1, collapsed: false })).toEqual({ activeTab: 'tasks', splitRatio: 0.2, collapsed: false })
  })
})
