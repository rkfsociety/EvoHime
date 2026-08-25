import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

import { RepairService } from '../src/main/repair-service'

function event(error: string) {
  return {
    sequenceId: 1,
    taskId: 'task-1',
    eventType: 'task.failed',
    payload: JSON.stringify({ error }),
    executionEvent: null
  }
}

function makeService(directory: string): RepairService {
  return new RepairService({
    filePath: join(directory, 'repair.json'),
    repairRoot: join(directory, 'repair'),
    config: {
      enabled: true,
      repositoryUrl: 'https://github.com/rkfsociety/EvoHime.git',
      branch: 'main',
      launchPolicy: 'installer',
      checkIntervalMs: 1_800_000,
      requireGreenCommit: true,
      greenCommitDepth: 10,
      githubToken: null,
      sourceDirectory: join(directory, 'source'),
      stagingDirectory: join(directory, 'staging'),
      stateDirectory: join(directory, 'state'),
      installDirectory: join(directory, 'install')
    },
    startTask: () => true,
    stopTask: () => true,
    emit: () => undefined,
    log: () => undefined
  })
}

describe('RepairService', () => {
  it('только после порога ошибок показывает кнопку repair', () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-repair-'))
    try {
      const service = makeService(directory)
      service.observe(event('tool failed: first'))
      service.observe(event('tool failed: second'))
      expect(service.status.phase).toBe('idle')
      service.observe(event('tool failed: third'))
      expect(service.status.phase).toBe('available')
      expect(service.status.errorCount).toBe(3)
      expect(service.status.repeatedPatterns).toBe(1)
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  })

  it('сохраняет error digest между запусками main', () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-repair-'))
    try {
      const service = makeService(directory)
      service.observe(event('same failure'))
      service.observe(event('same failure'))
      service.observe(event('same failure'))
      expect(service.status.phase).toBe('available')
      expect(service.status.evidence?.at(-1)).toMatchObject({ phase: 'available', result: 'pending' })
      const reopened = makeService(directory)
      expect(reopened.status.errorCount).toBe(3)
      expect(reopened.status.phase).toBe('available')
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  })

  it('переводит незавершённый run в recoverable failure после перезапуска main', () => {
    const directory = mkdtempSync(join(tmpdir(), 'evohime-repair-'))
    try {
      const service = makeService(directory)
      service.observe(event('same failure'))
      service.observe(event('same failure'))
      service.observe(event('same failure'))
      const file = join(directory, 'repair.json')
      const state = JSON.parse(readFileSync(file, 'utf8'))
      state.phase = 'diagnosing'
      state.taskId = 'interrupted'
      writeFileSync(file, JSON.stringify(state))
      const reopened = makeService(directory)
      expect(reopened.status.phase).toBe('failed')
      expect(reopened.status.taskId).toBeNull()
      expect(reopened.status.error).toContain('прерван')
    } finally {
      rmSync(directory, { recursive: true, force: true })
    }
  })
})
