import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'

import { JsonlLogger } from '../src/main/diagnostics/logger'

const directories: string[] = []

afterEach(async () => {
  await Promise.all(directories.splice(0).map((directory) => rm(directory, { recursive: true, force: true })))
})

async function temporaryDirectory(): Promise<string> {
  const directory = await mkdtemp(join(tmpdir(), 'evohime-logger-'))
  directories.push(directory)
  return directory
}

describe('JsonlLogger', () => {
  it('writes redacted records in order and keeps main and updater files separate', async () => {
    const directory = await temporaryDirectory()
    const main = new JsonlLogger({ directory, stream: 'main' })
    const updater = new JsonlLogger({ directory, stream: 'updater' })
    main.write('info', 'first', { token: 'secret-value' })
    main.write('warn', 'second')
    updater.write('info', 'updater')

    expect(await main.flush()).toBe(true)
    expect(await updater.flush()).toBe(true)
    expect(main.path).not.toBe(updater.path)
    const mainLines = (await readFile(main.path, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as Record<string, unknown>)
    expect(mainLines.map((line) => line.event)).toEqual(['first', 'second'])
    expect(JSON.stringify(mainLines)).not.toContain('secret-value')
    expect(await readFile(updater.path, 'utf8')).toContain('"event":"updater"')
  })

  it('rotates within the configured generation count', async () => {
    const directory = await temporaryDirectory()
    const logger = new JsonlLogger({ directory, stream: 'main', maxBytes: 1, maxFiles: 2 })
    logger.write('info', 'one')
    logger.write('info', 'two')
    logger.write('info', 'three')

    expect(await logger.flush()).toBe(true)
    expect(await readFile(`${logger.path}.1`, 'utf8')).toContain('"event":"two"')
    expect(await readFile(logger.path, 'utf8')).toContain('"event":"three"')
  })

  it('bounds queued records and exposes dropped writes', async () => {
    const directory = await temporaryDirectory()
    let releaseAppend: (() => void) | undefined
    let appendStarted: (() => void) | undefined
    const started = new Promise<void>((resolve) => { appendStarted = resolve })
    const logger = new JsonlLogger({
      directory,
      stream: 'main',
      maxQueueRecords: 1,
      fileSystem: {
        mkdir: async () => undefined,
        stat: async () => { throw Object.assign(new Error('missing'), { code: 'ENOENT' }) },
        appendFile: async () => {
          appendStarted?.()
          await new Promise<void>((resolve) => { releaseAppend = resolve })
        },
        rename: async () => undefined,
        rm: async () => undefined
      }
    })
    logger.write('info', 'kept')
    await started
    logger.write('info', 'dropped')
    expect(logger.status).toMatchObject({ droppedRecords: 1, queuedRecords: 1 })
    releaseAppend?.()
    expect(await logger.flush()).toBe(true)
    expect(logger.status).toMatchObject({ disabled: false, droppedRecords: 1, queuedRecords: 0 })
  })

  it('disables itself after a filesystem failure without rejecting writes', async () => {
    const logger = new JsonlLogger({
      directory: await temporaryDirectory(),
      stream: 'main',
      fileSystem: {
        mkdir: async () => { throw new Error('disk error') },
        stat: async () => ({ size: 0 }),
        appendFile: async () => undefined,
        rename: async () => undefined,
        rm: async () => undefined
      }
    })
    logger.write('error', 'failure')
    expect(await logger.flush()).toBe(false)
    logger.write('info', 'after failure')
    expect(logger.status).toMatchObject({ disabled: true, droppedRecords: 2, queuedRecords: 0 })
  })

  it('bounds flush waiting and can finish the same drain later', async () => {
    let releaseAppend: (() => void) | undefined
    let appendStarted: (() => void) | undefined
    const started = new Promise<void>((resolve) => { appendStarted = resolve })
    const logger = new JsonlLogger({
      directory: await temporaryDirectory(),
      stream: 'main',
      fileSystem: {
        mkdir: async () => undefined,
        stat: async () => { throw Object.assign(new Error('missing'), { code: 'ENOENT' }) },
        appendFile: async () => {
          appendStarted?.()
          await new Promise<void>((resolve) => { releaseAppend = resolve })
        },
        rename: async () => undefined,
        rm: async () => undefined
      }
    })
    logger.write('info', 'slow')
    await started
    expect(await logger.flush(1)).toBe(false)
    releaseAppend?.()
    expect(await logger.flush()).toBe(true)
  })
})
