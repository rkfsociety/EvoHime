import { spawn } from 'node:child_process'

import { buildStagedPackage, type BuildDeps, type BuildInputs } from './builder'
import { detectToolchain } from './toolchain'

export const BUILD_WORKER_FLAG = '--evohime-update-build-worker'

/**
 * Marker the worker prints when a build step starts.
 *
 * The worker runs in its own process, so the `onStep` callback cannot reach the
 * shell directly: without this the checklist stayed at «источники» for the whole
 * rebuild and a working build looked frozen.
 */
export const BUILD_STEP_PREFIX = '::evohime-step::'

type BuildStep = 'core' | 'shell' | 'package'

const BUILD_STEPS: readonly BuildStep[] = ['core', 'shell', 'package']

function parseStep(line: string): BuildStep | null {
  if (!line.startsWith(BUILD_STEP_PREFIX)) return null
  const value = line.slice(BUILD_STEP_PREFIX.length).trim()
  return BUILD_STEPS.find((step) => step === value) ?? null
}

/** Runs the rebuild outside the interactive Electron main process. */
export function runBuildWorker(inputs: BuildInputs, deps: BuildDeps = {}): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(
      process.execPath,
      [
        BUILD_WORKER_FLAG,
        '--source', inputs.sourceDirectory,
        '--staging', inputs.stagingDirectory,
        '--install', inputs.installDirectory,
        '--commit', inputs.commit,
        '--branch', inputs.branch
      ],
      { windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] }
    )
    let lastLine = ''
    let settled = false
    const abort = (): void => {
      child.kill()
      finish(new Error('Сборка обновления отменена.'))
    }
    const finish = (error?: Error): void => {
      if (settled) return
      settled = true
      deps.signal?.removeEventListener('abort', abort)
      if (error) reject(error)
      else resolve()
    }
    const consume = (chunk: Buffer | string): void => {
      for (const line of String(chunk).split(/\r?\n/)) {
        const trimmed = line.trim()
        if (!trimmed) continue
        const step = parseStep(trimmed)
        if (step) {
          deps.onStep?.(step)
          continue
        }
        lastLine = trimmed
        deps.onLine?.(trimmed)
      }
    }

    child.stdout?.on('data', consume)
    child.stderr?.on('data', consume)
    child.once('error', (error) => finish(error))
    child.once('close', (code) => {
      if (code === 0) finish()
      else finish(new Error(lastLine || `worker завершился с кодом ${code ?? -1}`))
    })
    deps.signal?.addEventListener('abort', abort, { once: true })
  })
}

/** Entry point used only by the detached build worker process. */
export async function runBuildWorkerProcess(args: readonly string[]): Promise<void> {
  const report = await detectToolchain({})
  if (!report.complete) throw new Error('Инструменты сборки недоступны в worker-процессе.')
  await buildStagedPackage(
    {
      sourceDirectory: required(args, '--source'),
      stagingDirectory: required(args, '--staging'),
      installDirectory: required(args, '--install'),
      commit: required(args, '--commit'),
      branch: required(args, '--branch'),
      toolchain: report
    },
    {
      onLine: (line) => console.log(line),
      onStep: (step) => console.log(`${BUILD_STEP_PREFIX}${step}`)
    }
  )
}

function required(args: readonly string[], name: string): string {
  const index = args.indexOf(name)
  const value = index >= 0 ? args[index + 1] : undefined
  if (!value) throw new Error(`Не указан аргумент worker: ${name}`)
  return value
}
