import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it } from 'vitest'

import { CodexService, normalizeModels, resolveCodexExecutableForEnvironment } from '../src/main/codex-service'

const directories: string[] = []

afterEach(() => {
  for (const directory of directories.splice(0)) rmSync(directory, { recursive: true, force: true })
})

describe('Codex CLI installation', () => {
  it('finds the versioned WinGet installation under LocalAppData', () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-codex-'))
    directories.push(directory)
    const executable = join(directory, 'OpenAI', 'Codex', 'bin', 'versioned', 'codex.exe')
    mkdirSync(join(executable, '..'), { recursive: true })
    writeFileSync(executable, 'codex')

    expect(resolveCodexExecutableForEnvironment({ LOCALAPPDATA: directory, PATH: '' })).toBe(executable)
  })

  it('keeps the Codex 5.6 family visible when app-server has not published it yet', () => {
    const models = normalizeModels({ data: [{ id: 'gpt-5.5', model: 'gpt-5.5', displayName: 'GPT-5.5', hidden: false }] })

    expect(models.slice(0, 3).map((model) => [model.id, model.displayName])).toEqual([
      ['gpt-5.6-sol', '5.6 Sol'],
      ['gpt-5.6-terra', '5.6 Terra'],
      ['gpt-5.6-luna', '5.6 Luna']
    ])
  })

  it('uses the pinned winget package and reports a bounded failure', async () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-codex-'))
    directories.push(directory)
    const calls: { file: string; args: readonly string[] }[] = []
    const service = new CodexService(join(directory, 'codex.json'), () => undefined, undefined, async (options) => {
      calls.push({ file: options.file, args: options.args })
      return { code: 1, tail: ['winget unavailable'], raw: [], timedOut: false }
    }, undefined, { LOCALAPPDATA: directory, APPDATA: directory, PATH: '' })

    const status = await service.install()
    expect(calls[0]).toEqual(expect.objectContaining({ file: 'winget' }))
    expect(calls[0]?.args).toContain('OpenAI.Codex')
    expect(calls[0]?.args).toContain('--disable-interactivity')
    expect(status.installing).toBe(false)
    expect(status.error).toContain('Установка Codex CLI не выполнена')
  })
})
