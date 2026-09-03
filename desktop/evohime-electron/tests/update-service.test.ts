import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

import { afterEach, describe, expect, it, vi } from 'vitest'

import type { UpdateStatus } from '@shared/update'

import { loadUpdateConfig, type UpdateConfig } from '../src/main/update/config'
import { UpdateService, type UpdateServiceDeps } from '../src/main/update/update-service'

const INSTALLED = 'a'.repeat(40)
const REMOTE = 'b'.repeat(40)
const RELEASE = 'c'.repeat(40)

const roots: string[] = []

afterEach(() => {
  for (const root of roots.splice(0, roots.length)) {
    rmSync(root, { recursive: true, force: true })
  }
})

interface Harness {
  readonly service: UpdateService
  readonly config: UpdateConfig
  readonly statuses: UpdateStatus[]
  readonly spawnWorker: ReturnType<typeof vi.fn>
  readonly build: ReturnType<typeof vi.fn>
  readonly quit: ReturnType<typeof vi.fn>
  /** Writes a staged package as a finished background rebuild would leave it. */
  stage(commit: string): void
}

function harness(
  overrides: Partial<UpdateServiceDeps> = {},
  installedCommit: string | null = INSTALLED,
  configOverrides: Partial<UpdateConfig> = {}
): Harness {
  const root = mkdtempSync(join(tmpdir(), 'evohime-update-'))
  roots.push(root)
  const install = join(root, 'install')
  mkdirSync(install, { recursive: true })
  writeFileSync(join(install, 'EvoHime.exe'), 'shell')
  writeFileSync(join(install, 'evohime-transaction.exe'), 'worker')
  if (installedCommit) {
    writeFileSync(
      join(install, 'evohime.build.json'),
      JSON.stringify({ commit: installedCommit, branch: 'main', builtAtMs: 1 })
    )
  }

  const config = loadUpdateConfig({
    dataDirectory: root,
    executablePath: join(install, 'EvoHime.exe'),
    readFile: () => null
  })
  const updateConfig: UpdateConfig = { ...config, launchPolicy: 'build', ...configOverrides }

  const statuses: UpdateStatus[] = []
  const spawnWorker = vi.fn()
  const quit = vi.fn()
  const stage = (commit: string): void => {
    mkdirSync(config.stagingDirectory, { recursive: true })
    writeFileSync(join(config.stagingDirectory, 'EvoHime.exe'), 'staged shell')
    writeFileSync(
      join(config.stagingDirectory, 'evohime.build.json'),
      JSON.stringify({ commit, branch: 'main', builtAtMs: 2 })
    )
  }
  // A real build leaves the staged package behind; the stub does the same, so
  // the apply step is exercised exactly as in production.
  const build = vi.fn(async () => {
    stage(REMOTE)
    return { commit: REMOTE, branch: 'main', builtAtMs: 2 }
  })

  const service = new UpdateService({
    config: updateConfig,
    emit: (status) => statuses.push(status),
    log: () => {},
    quit,
    spawnWorker,
    build,
    detect: async () => ({
      complete: true,
      pathEntries: [],
      tools: [
        { id: 'git', label: 'Git', available: true, path: 'git' },
        { id: 'node', label: 'Node.js 22', available: true, path: 'node' },
        { id: 'rust', label: 'Rust (cargo)', available: true, path: 'cargo' },
        { id: 'msvc', label: 'MSVC Build Tools', available: true, path: 'vswhere' }
      ]
    }),
    ensure: async () => ({
      report: { complete: true, pathEntries: [], tools: [{ id: 'git', label: 'Git', available: true, path: 'git' }] },
      error: null
    }),
    remoteHead: async () => REMOTE,
    // The default branch tip is green and touches product code; tests that care
    // override these.
    selectGreen: async () => ({ commit: REMOTE, tipState: 'success' as const }),
    publishedInstallerCommit: async () => REMOTE,
    commitState: async () => 'success',
    productChanges: async () => true,
    // No test may shell out to the real gh CLI for a credential.
    resolveToken: async () => null,
    sync: async () => REMOTE,
    // Timers never fire on their own in tests.
    setTimer: () => ({ cancel: () => {} }),
    ...overrides
  })

  return {
    service,
    config,
    statuses,
    spawnWorker,
    build,
    quit,
    stage
  }
}

describe('update check', () => {
  it('reports an up-to-date installation without touching the sources', async () => {
    const { service, build } = harness({
      remoteHead: async () => INSTALLED,
      selectGreen: async () => ({ commit: INSTALLED, tipState: 'success' })
    })

    const status = await service.check()

    expect(status.phase).toBe('up-to-date')
    expect(status.installedCommit).toBe(INSTALLED)
    expect(build).not.toHaveBeenCalled()
  })

  it('reports an available update when the branch moved ahead', async () => {
    const status = await harness().service.check()

    expect(status.phase).toBe('available')
    expect(status.remoteCommit).toBe(REMOTE)
  })

  it('treats an unknown installed commit as outdated', async () => {
    const status = await harness({}, null).service.check()

    expect(status.installedCommit).toBeNull()
    expect(status.phase).toBe('available')
  })

  it('surfaces a failed check without claiming the install is current', async () => {
    const status = await harness({
      remoteHead: async () => {
        throw new Error('нет сети')
      }
    }).service.check()

    expect(status.phase).toBe('failed')
    expect(status.error).toContain('Не удалось проверить обновления')
    expect(status.evidence?.at(-1)).toMatchObject({ phase: 'failed', result: 'failed' })
  })
})

describe('launch gate', () => {
  it('does not block startup while a CI installer downloads', async () => {
    const downloadInstaller = vi.fn(async (_repository: string, _branch: string, commit: string, staging: string) => {
      mkdirSync(staging, { recursive: true })
      writeFileSync(join(staging, 'EvoHime-Setup.exe'), 'installer')
      writeFileSync(join(staging, 'evohime.build.json'), JSON.stringify({ commit, branch: 'main', builtAtMs: 2 }))
      return { installer: join(staging, 'EvoHime-Setup.exe'), marker: { commit, branch: 'main', builtAtMs: 2 } }
    })
    const test = harness({ downloadInstaller }, INSTALLED, { launchPolicy: 'installer' })

    await expect(test.service.runLaunchGate()).resolves.toBe('continue')
    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(downloadInstaller).toHaveBeenCalledTimes(1)
    expect(test.quit).not.toHaveBeenCalled()
    expect(test.service.status.phase).toBe('ready')
    expect(test.service.status.restartRequired).toBe(true)
    expect(test.service.status.blocking).toBe(false)
  })

  it('uses a green published installer even when main already has a newer green commit', async () => {
    const selectGreen = vi.fn(async () => ({ commit: REMOTE, tipState: 'success' as const }))
    const test = harness({
      selectGreen,
      publishedInstallerCommit: async () => RELEASE,
      commitState: async (_apiBase, commit) => commit === RELEASE ? 'success' : 'failure'
    }, INSTALLED, { launchPolicy: 'installer' })

    const status = await test.service.check()

    expect(status.phase).toBe('available')
    expect(status.remoteCommit).toBe(RELEASE)
    expect(status.message).toContain('опубликованного release')
    expect(selectGreen).not.toHaveBeenCalled()
  })

  it('does not use a published installer whose CI is not green', async () => {
    const test = harness({
      publishedInstallerCommit: async () => RELEASE,
      commitState: async () => 'pending'
    }, INSTALLED, { launchPolicy: 'installer' })

    const status = await test.service.check()

    expect(status.phase).toBe('up-to-date')
    expect(status.remoteCommit).toBeNull()
    expect(status.message).toContain('ожидает завершения')
  })

  it('does not keep showing a staged installer after that commit is installed', async () => {
    const downloadInstaller = vi.fn()
    const test = harness({
      downloadInstaller,
      publishedInstallerCommit: async () => REMOTE
    }, REMOTE, { launchPolicy: 'installer' })
    mkdirSync(test.config.stagingDirectory, { recursive: true })
    writeFileSync(join(test.config.stagingDirectory, 'EvoHime-Setup.exe'), 'installer')
    writeFileSync(
      join(test.config.stagingDirectory, 'evohime.build.json'),
      JSON.stringify({ commit: REMOTE, branch: 'main', builtAtMs: 2 })
    )

    const outcome = await test.service.runLaunchGate()

    expect(outcome).toBe('continue')
    expect(test.service.status.phase).toBe('up-to-date')
    expect(test.service.status.restartRequired).toBe(false)
    expect(downloadInstaller).not.toHaveBeenCalled()
    expect(test.quit).not.toHaveBeenCalled()
  })

  it('does nothing when updates are disabled', async () => {
    const { config, build } = harness()
    const disabled = new UpdateService({
      config: { ...config, enabled: false, launchPolicy: 'off' },
      emit: () => {},
      log: () => {},
      quit: () => {},
      build
    })

    await expect(disabled.runLaunchGate()).resolves.toBe('continue')
    expect(disabled.status.phase).toBe('disabled')
    expect(build).not.toHaveBeenCalled()
  })

  it('rebuilds and applies before Core is started', async () => {
    const test = harness()

    const outcome = await test.service.runLaunchGate()

    expect(test.build).toHaveBeenCalledTimes(1)
    expect(outcome).toBe('applying')
    expect(test.quit).toHaveBeenCalledTimes(1)
    const [worker, args] = test.spawnWorker.mock.calls[0] as [string, string[]]
    expect(worker).toBe(join(test.config.installDirectory, 'evohime-transaction.exe'))
    expect(args).toContain('--apply-staging')
    expect(args[args.indexOf('--staging') + 1]).toBe(test.config.stagingDirectory)
    expect(args[args.indexOf('--install-dir') + 1]).toBe(test.config.installDirectory)
    expect(args[args.indexOf('--wait-pid') + 1]).toBe(String(process.pid))
  })

  it('applies a package a previous background run already staged', async () => {
    const test = harness()
    test.stage(REMOTE)
    // The staged commit is the tip already, so no rebuild is needed.
    const outcome = await test.service.runLaunchGate()

    expect(outcome).toBe('applying')
    expect(test.build).not.toHaveBeenCalled()
  })

  it('starts the installed build when the rebuild fails', async () => {
    const test = harness({
      build: vi.fn(async () => {
        throw new Error('cargo упал')
      })
    })

    await expect(test.service.runLaunchGate()).resolves.toBe('continue')
    expect(test.service.status.phase).toBe('failed')
    expect(test.service.status.blocking).toBe(false)
    expect(test.quit).not.toHaveBeenCalled()
  })

  it('starts the installed build when the check fails', async () => {
    const test = harness({
      remoteHead: async () => {
        throw new Error('нет сети')
      }
    })

    await expect(test.service.runLaunchGate()).resolves.toBe('continue')
    expect(test.build).not.toHaveBeenCalled()
    expect(test.service.status.blocking).toBe(false)
  })

  it('only swaps a staged package under apply-ready policy', async () => {
    const test = harness()
    const service = new UpdateService({
      config: { ...test.config, launchPolicy: 'apply-ready' },
      emit: () => {},
      log: () => {},
      quit: test.quit,
      build: test.build,
      spawnWorker: test.spawnWorker,
      detect: async () => ({ complete: true, pathEntries: [], tools: [{ id: 'git', label: 'Git', available: true, path: 'git' }] }),
      remoteHead: async () => REMOTE,
      selectGreen: async () => ({ commit: REMOTE, tipState: 'success' }),
      productChanges: async () => true,
      resolveToken: async () => null,
      setTimer: () => ({ cancel: () => {} })
    })

    await expect(service.runLaunchGate()).resolves.toBe('continue')
    expect(test.build).not.toHaveBeenCalled()
  })

  it('releases the gate as soon as the user skips the rebuild', async () => {
    const test = harness({
      build: vi.fn(
        () =>
          new Promise<never>((_resolve, reject) => {
            setTimeout(() => reject(new Error('прервано')), 5)
          })
      )
    })

    const gate = test.service.runLaunchGate()
    // The skip arrives while the rebuild is running, as it does in the UI.
    await new Promise((resolve) => setTimeout(resolve, 1))
    test.service.skip()

    await expect(gate).resolves.toBe('continue')
    expect(test.quit).not.toHaveBeenCalled()
    expect(test.service.status.blocking).toBe(false)
  })
})

describe('restart', () => {
  it('refuses to restart when nothing is staged', () => {
    const test = harness()

    expect(test.service.restart()).toBe(false)
    expect(test.quit).not.toHaveBeenCalled()
  })

  it('hands the staged package to the transaction worker', () => {
    const test = harness()
    test.stage(REMOTE)

    expect(test.service.restart()).toBe(true)
    expect(test.spawnWorker).toHaveBeenCalledTimes(1)
    expect(test.quit).toHaveBeenCalledTimes(1)
    expect(test.service.status.phase).toBe('applying')
  })
})

describe('background pass', () => {
  it('rebuilds quietly and asks for a restart instead of forcing one', async () => {
    const test = harness()

    const status = await test.service.prepare()

    expect(status.phase).toBe('ready')
    expect(status.blocking).toBe(false)
    expect(status.restartRequired).toBe(true)
    expect(test.quit).not.toHaveBeenCalled()
    expect(test.statuses.some((entry) => entry.phase === 'preparing')).toBe(true)
  })

  it('does not rebuild twice for the same commit', async () => {
    const test = harness()

    await test.service.prepare()
    await test.service.prepare()

    expect(test.build).toHaveBeenCalledTimes(1)
  })
})

describe('resilience', () => {
  it('keeps the client running when the transaction worker cannot start', () => {
    const test = harness({
      spawnWorker: vi.fn(() => {
        throw new Error('ENOENT')
      })
    })
    test.stage(REMOTE)

    expect(test.service.restart()).toBe(false)
    expect(test.quit).not.toHaveBeenCalled()
    expect(test.service.status.phase).toBe('failed')
  })

  it('releases the gate when the toolchain cannot be installed', async () => {
    const test = harness({
      ensure: async () => ({
        report: { complete: false, pathEntries: [], tools: [] },
        error: 'Не удалось установить: Rust (cargo).'
      })
    })

    await expect(test.service.runLaunchGate()).resolves.toBe('continue')
    expect(test.build).not.toHaveBeenCalled()
    expect(test.service.status.blocking).toBe(false)
    expect(test.service.status.error).toContain('Инструменты сборки не готовы')
    expect(test.service.status.steps.find((step) => step.id === 'toolchain')?.state).toBe('failed')
  })
})

describe('transient build failures', () => {
  it('clears the derived state and builds once more before giving up', async () => {
    let attempts = 0
    const reset = vi.fn(async () => {})
    const test = harness({
      reset,
      build: vi.fn(async () => {
        attempts += 1
        // An interrupted Electron download unpacks into a broken package; the
        // next attempt succeeds once the cache is gone.
        if (attempts === 1) throw new Error('Invalid package')
        return { commit: REMOTE, branch: 'main', builtAtMs: 2 }
      })
    })

    const status = await test.service.prepare()

    expect(attempts).toBe(2)
    expect(reset).toHaveBeenCalledTimes(1)
    expect(status.phase).toBe('ready')
    expect(status.error).toBeNull()
  })

  it('reports the second failure instead of retrying forever', async () => {
    const reset = vi.fn(async () => {})
    const build = vi.fn(async () => {
      throw new Error('cargo упал')
    })
    const test = harness({ reset, build })

    const status = await test.service.prepare()

    expect(build).toHaveBeenCalledTimes(2)
    expect(reset).toHaveBeenCalledTimes(1)
    expect(status.phase).toBe('failed')
    expect(status.error).toContain('cargo')
  })

  it('retries after a failed attempt instead of returning the stale failure', async () => {
    let attempts = 0
    const test = harness({
      reset: vi.fn(async () => {}),
      // Обе попытки первого запуска падают: статус остаётся «failed», а
      // remoteCommit уже известен — ровно то состояние, в котором пользователь
      // жмёт «Повторить».
      build: vi.fn(async () => {
        attempts += 1
        if (attempts <= 2) throw new Error('git fetch: нет сети')
        return { commit: REMOTE, branch: 'main', builtAtMs: 2 }
      })
    })

    expect((await test.service.prepare()).phase).toBe('failed')

    const retried = await test.service.prepare()

    expect(attempts).toBe(3)
    expect(retried.phase).toBe('ready')
    expect(retried.error).toBeNull()
  })

  it('does not retry a build the user skipped', async () => {
    const reset = vi.fn(async () => {})
    const test = harness({
      reset,
      build: vi.fn(
        () =>
          new Promise<never>((_resolve, reject) => {
            setTimeout(() => reject(new Error('прервано')), 5)
          })
      )
    })

    const running = test.service.prepare()
    await new Promise((resolve) => setTimeout(resolve, 1))
    test.service.skip()
    await running

    expect(reset).not.toHaveBeenCalled()
  })
})

describe('build log', () => {
  it('keeps the whole build output, not just the line the UI shows', async () => {
    const lines: string[] = []
    const test = harness({
      buildLog: { start: (header) => lines.push(header), append: (line) => lines.push(line) },
      build: vi.fn(async (_inputs, deps) => {
        deps?.onLine?.('Compiling evohime-core')
        deps?.onLine?.('Finished release profile')
        return { commit: REMOTE, branch: 'main', builtAtMs: 2 }
      })
    })

    await test.service.prepare()

    expect(lines[0]).toContain(REMOTE)
    expect(lines).toContain('Compiling evohime-core')
    expect(lines).toContain('Finished release profile')
  })
})

describe('green commits only', () => {
  it('waits instead of rebuilding a commit whose checks are still running', async () => {
    const test = harness({
      selectGreen: async () => ({ commit: null, tipState: 'pending' })
    })

    const status = await test.service.check()

    expect(status.phase).toBe('up-to-date')
    expect(status.remoteCommit).toBeNull()
    expect(status.message).toContain('проверяется')
    await test.service.prepare()
    expect(test.build).not.toHaveBeenCalled()
  })

  it('never installs a commit that failed its checks', async () => {
    const test = harness({
      selectGreen: async () => ({ commit: null, tipState: 'failure' })
    })

    const outcome = await test.service.runLaunchGate()

    expect(outcome).toBe('continue')
    expect(test.build).not.toHaveBeenCalled()
    expect(test.quit).not.toHaveBeenCalled()
    expect(test.service.status.message).toContain('не прошли проверки')
  })

  it('refuses to guess when the checks cannot be read at all', async () => {
    const { config } = harness()
    const statuses: UpdateStatus[] = []
    const build = vi.fn()
    const service = new UpdateService({
      // A non-GitHub remote has no checks API the client could trust.
      config: { ...config, repositoryUrl: 'https://git.example.invalid/evo.git' },
      emit: (status) => statuses.push(status),
      log: () => {},
      quit: () => {},
      build: build as never,
      detect: async () => ({
        complete: true,
        pathEntries: [],
        tools: [{ id: 'git', label: 'Git', available: true, path: 'git' }]
      }),
      remoteHead: async () => REMOTE,
      setTimer: () => ({ cancel: () => {} })
    })

    const status = await service.check()

    expect(status.phase).toBe('up-to-date')
    expect(status.remoteCommit).toBeNull()
    expect(status.message).toContain('недоступны')
  })

  it('follows the branch tip when the check requirement is switched off', async () => {
    const { config } = harness()
    const service = new UpdateService({
      config: { ...config, launchPolicy: 'build', requireGreenCommit: false },
      emit: () => {},
      log: () => {},
      quit: () => {},
      detect: async () => ({
        complete: true,
        pathEntries: [],
        tools: [{ id: 'git', label: 'Git', available: true, path: 'git' }]
      }),
      remoteHead: async () => REMOTE,
      selectGreen: async () => {
        throw new Error('checks must not be consulted')
      },
      productChanges: async () => true,
      resolveToken: async () => null,
      setTimer: () => ({ cancel: () => {} })
    })

    const status = await service.check()

    expect(status.phase).toBe('available')
    expect(status.remoteCommit).toBe(REMOTE)
  })
})

describe('inconclusive checks', () => {
  it('does not call a cancelled run a failure', async () => {
    const test = harness({
      selectGreen: async () => ({ commit: null, tipState: 'unknown' })
    })

    const status = await test.service.check()

    expect(status.phase).toBe('up-to-date')
    expect(status.message).toContain('не завершились')
    expect(status.message).not.toContain('не прошли')
  })
})

describe('documentation-only commits', () => {
  it('rebuilds when the range cannot be compared', async () => {
    const test = harness({
      productChanges: async () => {
        throw new Error('GitHub API ответил 502')
      }
    })

    const status = await test.service.check()

    // Не смогли доказать, что менялись только доки — значит собираем.
    expect(status.phase).toBe('available')
  })

  it('always rebuilds when the installed commit is unknown', async () => {
    const compare = vi.fn(async () => false)
    const test = harness({ productChanges: compare }, null)

    const status = await test.service.check()

    expect(status.phase).toBe('available')
    expect(compare).not.toHaveBeenCalled()
  })
})
