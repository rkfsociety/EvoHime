import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { CodexService } from '../src/main/codex-service'

const directories: string[] = []

afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true })
})

describe('Codex CLI installation', () => {
  it('uses the pinned winget package and reports a bounded failure', async () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-codex-'))
    directories.push(directory)
    const calls: { file: string; args: readonly string[] }[] = []
    const service = new CodexService(join(directory, 'codex.json'), () => undefined, undefined, async (options) => {
      calls.push({ file: options.file, args: options.args })
      return { code: 1, tail: ['winget unavailable'], raw: [], timedOut: false }
    })

    const status = await service.install()
    expect(calls[0]).toEqual(expect.objectContaining({ file: 'winget' }))
    expect(calls[0]?.args).toContain('OpenAI.Codex')
    expect(calls[0]?.args).toContain('--disable-interactivity')
    expect(status.installing).toBe(false)
    expect(status.error).toContain('Установка Codex CLI не выполнена')
  })
})
