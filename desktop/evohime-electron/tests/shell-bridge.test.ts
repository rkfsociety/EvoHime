import { beforeEach, describe, expect, it, vi } from 'vitest'

import type {
  CommandFailure,
  ProviderSummary,
  RendererCommand,
  ShellState
} from '../src/shared/api'
import type { ProviderUpdate } from '../src/main/provider-store'
import type { ListenerRuntimeStatus } from '../src/shared/listener-runtime'
import {
  CLIPBOARD_CHANNEL,
  INVOKE_CHANNEL,
  OPEN_EXTERNAL_CHANNEL
} from '../src/shared/channels'

/**
 * Regression tests for the only path a renderer has into Core.
 *
 * The bridge must forward exactly the allow-listed commands with validated
 * payloads and refuse everything else. Core still re-validates capability,
 * policy and approval — these tests cover the shell's own narrow surface.
 */

type Handler = (event: unknown, ...args: unknown[]) => unknown

const handlers = new Map<string, Handler>()
const clipboardWrites: string[] = []
const openedUrls: string[] = []

vi.mock('electron', () => ({
  ipcMain: {
    handle: (channel: string, handler: Handler) => handlers.set(channel, handler)
  },
  clipboard: {
    writeText: (text: string) => clipboardWrites.push(text)
  },
  shell: {
    openExternal: async (url: string) => {
      openedUrls.push(url)
    }
  },
  BrowserWindow: {
    getAllWindows: () => [],
    getFocusedWindow: () => null
  },
  dialog: {
    showOpenDialog: async (options: { defaultPath?: string }) => {
      dialogOptions.push(options)
      return { canceled: true, filePaths: [] }
    },
    showSaveDialog: async () => ({ canceled: false, filePath: 'G:/evohime-trace.md' })
  }
}))

/** Папки, существующие в тесте: остальные `stat` считает отсутствующими. */
const existingDirectories = new Set<string>()
const dialogOptions: { defaultPath?: string }[] = []

vi.mock('node:fs/promises', () => ({
  readFile: async () => '',
  writeFile: async () => undefined,
  stat: async (path: string) => {
    if (!existingDirectories.has(path)) throw new Error('ENOENT')
    return { isDirectory: () => true }
  }
}))

const { registerShellBridge } = await import('../src/main/shell-bridge')

interface SentCommand {
  readonly [key: string]: unknown
}

const sent: SentCommand[] = []
let enqueueResult: 'queued' | 'queue-full' = 'queued'

const fakeState: ShellState = {
  connection: 'connected',
  protocol: { major: 1, minor: 0 },
  capabilities: ['replay'],
  coreVersion: '0.1.0',
  lastSequence: 4,
  reason: null,
  reconnectAttempts: 0
}

const client = {
  get state(): ShellState {
    return fakeState
  },
  send(command: SentCommand): 'queued' | 'queue-full' {
    sent.push(command)
    return enqueueResult
  },
  requestResync(): 'queued' | 'queue-full' {
    sent.push({ resyncRequest: {} })
    return enqueueResult
  }
}

let providerSummary: ProviderSummary = {
  provider: 'literouter',
  model: '',
  baseUrl: '',
  tier: 'free',
  configured: false
}
const providerWrites: ProviderUpdate[] = []
const restarts: boolean[] = []

/** Stands in for the credential store; encryption is covered by its own tests. */
const providers = {
  summary: () => providerSummary,
  save: (update: ProviderUpdate) => {
    providerWrites.push(update)
    providerSummary = {
      provider: update.provider,
      model: update.model,
      baseUrl: update.baseUrl,
      tier: update.tier,
      configured: update.apiKey.length > 0 || providerSummary.configured
    }
    return providerSummary
  },
  clearKey: () => {
    providerSummary = { ...providerSummary, configured: false }
    return providerSummary
  }
}

/** Stands in for the chat store; persistence is covered by its own tests. */
const chats = {
  list: () => [],
  create: (workspacePath: string) => ({
    id: 'chat-1',
    workspacePath,
    title: 'Новый чат',
    createdMs: 0,
    updatedMs: 0,
    taskIds: [],
    messages: []
  }),
  open: () => null,
  appendPrompt: () => null,
  remove: () => {}
}

let selectedWorkspace: string | null = null

/** Stands in for the workspace service; its own behaviour is tested separately. */
const workspaces = {
  list: () => ({ selected: selectedWorkspace, options: [] }),
  pick: async () => ({ cancelled: true, selection: { selected: null, options: [] } }),
  select: () => 'unknown-workspace' as const,
  forget: () => ({ selected: null, options: [] }),
  setPermissionMode: () => ({ selected: selectedWorkspace, options: [] })
}

/** The updater is owned by the main process; the bridge only relays to it. */
const updateStatus = { phase: 'idle', blocking: false, message: '', steps: [] }
const updateCalls: string[] = []
const updates = {
  get status() {
    updateCalls.push('status')
    return updateStatus
  },
  check: async () => {
    updateCalls.push('check')
    return updateStatus
  },
  prepare: async () => {
    updateCalls.push('prepare')
    return updateStatus
  },
  restart: () => {
    updateCalls.push('restart')
    return true
  },
  skip: () => {
    updateCalls.push('skip')
    return updateStatus
  }
}

/** Речевой рантайм тоже принадлежит main-процессу; мост только передаёт. */
const listenerRuntimeStatus: ListenerRuntimeStatus = {
  state: 'missing' as const,
  installedVersion: null,
  availableVersion: '2026.08',
  progressPct: 0,
  message: 'Распознавание речи не установлено.',
  missingOptional: [] as readonly string[],
  toolsDirectory: 'C:\\tools\\listener'
}
const listenerCalls: string[] = []
let listenerDownloadStatus = listenerRuntimeStatus
const listenerRuntime = {
  get status() {
    listenerCalls.push('status')
    return listenerRuntimeStatus
  },
  check: async () => {
    listenerCalls.push('check')
    return listenerRuntimeStatus
  },
  download: async () => {
    listenerCalls.push('download')
    return listenerDownloadStatus
  }
}

function invoke(command: string, payload?: unknown): unknown {
  const handler = handlers.get(INVOKE_CHANNEL)
  if (!handler) {
    throw new Error('invoke channel is not registered')
  }
  return handler({}, command, payload)
}

beforeEach(() => {
  handlers.clear()
  sent.length = 0
  clipboardWrites.length = 0
  openedUrls.length = 0
  enqueueResult = 'queued'
  providerSummary = { provider: 'literouter', model: '', baseUrl: '', tier: 'free', configured: false }
  providerWrites.length = 0
  restarts.length = 0
  registerShellBridge({
    client: client as never,
    workspaces: workspaces as never,
    providers: providers as never,
    chats: chats as never,
    restartCore: async () => {
      restarts.push(true)
      return true
    },
    updates: updates as never,
    listenerRuntime: listenerRuntime as never,
    ambientHotkey: () => ({ combination: 'Control+Alt+M', registered: true }),
    log: () => {}
  })
  listenerCalls.length = 0
  listenerDownloadStatus = listenerRuntimeStatus
})

describe('renderer command surface', () => {
  it('exposes only the documented channels', () => {
    expect([...handlers.keys()].sort()).toEqual(
      [CLIPBOARD_CHANNEL, INVOKE_CHANNEL, OPEN_EXTERNAL_CHANNEL].sort()
    )
  })

  it('returns the shell state without touching Core', () => {
    expect(invoke('shell.getState', {})).toEqual({ ok: true, value: fakeState })
    expect(sent).toHaveLength(0)
  })

  it('forwards an allow-listed command', () => {
    const outcome = invoke('core.startTask', {
      taskId: 'task-1',
      prompt: 'сделай',
      workspacePath: 'C:\\work'
    })
    expect(outcome).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toEqual([
      { startTask: { taskId: 'task-1', prompt: 'сделай', workspacePath: 'C:\\work', preferredRouteHint: '' } }
    ])
  })

  it('forwards bounded Files and Git reads without exposing filesystem access', () => {
    expect(invoke('core.listWorkspace', {
      workspacePath: 'C:\\work', relativePath: '.', maxEntries: 20
    })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.readWorkspaceFile', {
      workspacePath: 'C:\\work', relativePath: 'src\\main.rs'
    })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.gitStatus', { workspacePath: 'C:\\work' })).toEqual({
      ok: true, value: { accepted: true }
    })
    expect(invoke('core.gitDiff', {
      workspacePath: 'C:\\work', relativePath: 'src\\main.rs'
    })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toEqual([
      { listWorkspace: { workspacePath: 'C:\\work', relativePath: '.', maxEntries: 20 } },
      { readWorkspaceFile: { workspacePath: 'C:\\work', relativePath: 'src\\main.rs', maxBytes: 512 * 1024 } },
      { gitStatus: { workspacePath: 'C:\\work', maxBytes: 512 * 1024 } },
      { gitDiff: { workspacePath: 'C:\\work', relativePath: 'src\\main.rs', maxBytes: 512 * 1024 } }
    ])
  })

  it('forwards stage 01.4 receipt listing, verify and export commands with defaulted filters', () => {
    expect(invoke('core.listReceipts', { taskId: 'task-1' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.verifyReceipts', { taskId: 'task-1', limit: 200 })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.exportReceipts', { destinationPath: 'C:\\export\\bundle' })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toEqual([
      { listReceipts: { taskId: 'task-1', runId: '', actionId: '', fromRfc3339: '', toRfc3339: '', limit: 0 } },
      { verifyReceipts: { taskId: 'task-1', runId: '', actionId: '', fromRfc3339: '', toRfc3339: '', limit: 200, trustKeyId: '' } },
      { exportReceipts: { destinationPath: 'C:\\export\\bundle', taskId: '', runId: '', actionId: '', fromRfc3339: '', toRfc3339: '', limit: 0, replace: false } }
    ])
  })

  it('rejects an out-of-range limit or missing export destination', () => {
    expect((invoke('core.verifyReceipts', { limit: 5_000 }) as CommandFailure).code).toBe('invalid-payload')
    expect((invoke('core.exportReceipts', { destinationPath: '' }) as CommandFailure).code).toBe('invalid-payload')
    expect(sent).toHaveLength(0)
  })

  it('forwards policy, diagnostics and backup commands using the canonical proto fields', () => {
    expect(invoke('core.setPermissionMode', { mode: 'read_only' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.runDoctor', { detailLevel: 1 })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.exportDoctorLogs', { destinationPath: 'C:\\diagnostics.jsonl' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.createDatabaseBackup', { destinationPath: 'C:\\backup.evohime' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.prepareDatabaseRestore', { backupPath: 'C:\\backup.evohime' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.restoreDatabase', { backupPath: 'C:\\backup.evohime', approvalId: 'approval-1' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.cancelDatabaseOperation', { operationId: 'operation-1' })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toContainEqual({ permissionMode: { mode: 'read_only' } })
    expect(sent).toContainEqual({ runDoctor: { projectId: '', detailLevel: 1 } })
    expect(sent).toContainEqual({ exportDoctorLogs: { destinationPath: 'C:\\diagnostics.jsonl' } })
    expect(sent).toContainEqual({ createDatabaseBackup: { destinationPath: 'C:\\backup.evohime' } })
    expect(sent).toContainEqual({ prepareDatabaseRestore: { backupPath: 'C:\\backup.evohime' } })
    expect(sent).toContainEqual({ restoreDatabase: { backupPath: 'C:\\backup.evohime', approvalId: 'approval-1' } })
    expect(sent).toContainEqual({ cancelDatabaseOperation: { operationId: 'operation-1' } })
  })

  it('forwards provider reference requests without accepting secret fields', () => {
    expect(invoke('core.getModelConfig', {})).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.listModelCatalog', { mode: 'free' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.listModelCatalog', { mode: 'paid', apiKey: 'secret' })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toContainEqual({ modelConfig: {} })
    expect(sent).toContainEqual({ modelCatalog: { mode: 'free' } })
    expect(sent).toContainEqual({ modelCatalog: { mode: 'paid' } })
  })

  it('stores a provider key locally and never forwards it to Core', async () => {
    const outcome = (await invoke('provider.save', {
      provider: 'literouter',
      apiKey: 'sk-secret-value',
      model: 'deepseek:free',
      baseUrl: 'https://api.literouter.com/v1',
      tier: 'paid'
    })) as { ok: true; value: { summary: ProviderSummary; restarted: boolean } }

    expect(outcome.value.summary.configured).toBe(true)
    expect(outcome.value.restarted).toBe(true)
    expect(restarts).toHaveLength(1)
    // The key belongs to the main process; no Core command may carry it.
    expect(JSON.stringify(sent)).not.toContain('sk-secret-value')
    expect(invoke('provider.get', {})).toEqual({
      ok: true,
      value: {
        provider: 'literouter',
        model: 'deepseek:free',
        baseUrl: 'https://api.literouter.com/v1',
        tier: 'paid',
        configured: true
      }
    })
  })

  it('rejects a provider endpoint that would leak the key over plain http', async () => {
    const outcome = (await invoke('provider.save', {
      provider: 'literouter',
      apiKey: 'sk-secret-value',
      model: '',
      baseUrl: 'http://example.com/v1',
      tier: 'free'
    })) as CommandFailure

    expect(outcome.ok).toBe(false)
    expect(outcome.code).toBe('invalid-payload')
    expect(providerWrites).toHaveLength(0)
    expect(restarts).toHaveLength(0)
  })

  it('forwards a bounded model selection to Core', () => {
    expect(invoke('core.selectModel', { model: 'deepseek:free' })).toEqual({
      ok: true,
      value: { accepted: true }
    })
    expect(sent).toContainEqual({ selectModel: { model: 'deepseek:free' } })

    const rejected = invoke('core.selectModel', { model: 'two words' }) as CommandFailure
    expect(rejected.ok).toBe(false)
    expect(sent).toHaveLength(1)
  })

  it('forwards the Core-owned editor build flow as bounded protobuf payloads', () => {
    const proposal = JSON.stringify({ scope: {}, changes: [] })
    const approved = JSON.stringify({ intent_hash: 'intent-1', changes: [] })
    expect(invoke('core.createProject', {
      projectId: 'project-1', title: 'Editor', workspacePath: 'C:\\work'
    })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.prepareBuild', { projectId: 'project-1', proposalJson: proposal })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.applyApprovedBuild', {
      projectId: 'project-1', runId: 'run-1', taskId: 'task-1', approvedBuildJson: approved
    })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent[0]).toEqual({ createProject: { projectId: 'project-1', title: 'Editor', workspacePath: 'C:\\work', sourceRef: '' } })
    expect(Buffer.from((sent[1]!.prepareBuild as { proposalJson: Uint8Array }).proposalJson).toString('utf8')).toBe(proposal)
    expect(Buffer.from((sent[2]!.applyApprovedBuild as { approvedBuildJson: Uint8Array }).approvedBuildJson).toString('utf8')).toBe(approved)
  })

  it('bounds Terminal program, arguments and timeout before Core', () => {
    expect(invoke('core.terminalExecute', {
      taskId: 'task-1', workspacePath: 'C:\\work', program: 'git', args: ['status', '--short'], cwd: '', timeoutMs: 30_000
    })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toContainEqual({ terminalExecute: {
      taskId: 'task-1', workspacePath: 'C:\\work', program: 'git', args: ['status', '--short'], cwd: '', timeoutMs: 30_000, approvalId: ''
    } })
    expect((invoke('core.terminalExecute', {
      taskId: 'task-1', workspacePath: 'C:\\work', program: 'git', args: ['status'], timeoutMs: 30_001
    }) as CommandFailure).code).toBe('invalid-payload')
  })

  it('rejects a command outside the allow-list', () => {
    for (const command of ['core.deleteEverything', 'shell.exec', '__proto__', '']) {
      const outcome = invoke(command, {}) as CommandFailure
      expect(outcome.ok).toBe(false)
      expect(outcome.code).toBe('unknown-command')
    }
    expect(sent).toHaveLength(0)
  })

  it('rejects malformed payloads instead of forwarding them', () => {
    const cases: Array<[RendererCommand, unknown]> = [
      ['core.startTask', { taskId: 'task-1' }],
      ['core.startTask', { taskId: '', prompt: 'p', workspacePath: 'C:\\w' }],
      ['core.startTask', null],
      ['core.stopTask', { taskId: 42 }],
      ['core.resolveApproval', { approvalId: 'a-1' }],
      ['core.resolveApproval', { approvalId: 'a-1', granted: 'yes' }],
      ['core.listWorkspace', { workspacePath: 'C:\\work', relativePath: '..\\secret' }],
      ['core.gitStatus', { workspacePath: 'C:\\work', maxBytes: 0 }]
    ]
    for (const [command, payload] of cases) {
      const outcome = invoke(command, payload) as CommandFailure
      expect(outcome.ok, `${command} ${JSON.stringify(payload)}`).toBe(false)
      expect(outcome.code).toBe('invalid-payload')
    }
    expect(sent).toHaveLength(0)
  })

  it('bounds oversized text fields', () => {
    const outcome = invoke('core.stopTask', { taskId: 'x'.repeat(5_000) }) as CommandFailure
    expect(outcome.code).toBe('invalid-payload')
    expect(sent).toHaveLength(0)
  })

  it('surfaces a full queue as a typed failure', () => {
    enqueueResult = 'queue-full'
    const outcome = invoke('core.stopTask', { taskId: 'task-1' }) as CommandFailure
    expect(outcome.ok).toBe(false)
    expect(outcome.code).toBe('queue-full')
  })
})

describe('clipboard and external links', () => {
  it('writes bounded plain text and never reads the clipboard back', async () => {
    const handler = handlers.get(CLIPBOARD_CHANNEL)!
    expect(await handler({}, 'скопируй меня')).toBe(true)
    expect(clipboardWrites).toEqual(['скопируй меня'])

    expect(await handler({}, '')).toBe(false)
    expect(await handler({}, 'x'.repeat(70_000))).toBe(false)
    expect(await handler({}, 42)).toBe(false)
    expect(clipboardWrites).toHaveLength(1)
  })

  it('opens only allow-listed https URLs', async () => {
    const handler = handlers.get(OPEN_EXTERNAL_CHANNEL)!
    expect(await handler({}, 'https://github.com/evohime')).toBe(true)
    expect(await handler({}, 'https://evil.tld/')).toBe(false)
    expect(await handler({}, 'file:///C:/Windows/System32/cmd.exe')).toBe(false)
    expect(await handler({}, 'javascript:alert(1)')).toBe(false)
    expect(openedUrls).toEqual(['https://github.com/evohime'])
  })
})

describe('trace export', () => {
  it('saves large trace content through the native save dialog', async () => {
    const result = await invoke('trace.export', { content: '# trace\n\n' + 'x'.repeat(100_000) })
    expect(result).toEqual({ ok: true, value: { cancelled: false, path: 'G:/evohime-trace.md' } })
  })
})

describe('source update commands', () => {
  it('relays every update command to the service that owns the run', async () => {
    updateCalls.length = 0

    expect((invoke('update.getStatus', {}) as { ok: boolean }).ok).toBe(true)
    await invoke('update.check', {})
    await invoke('update.prepare', {})
    expect(invoke('update.restart', {})).toEqual({ ok: true, value: { accepted: true } })
    expect((invoke('update.skip', {}) as { ok: boolean }).ok).toBe(true)

    expect(updateCalls).toEqual(['status', 'check', 'prepare', 'restart', 'skip'])
    // Nothing about an update goes to Core: it is a shell-owned operation.
    expect(sent).toHaveLength(0)
  })
})

/**
 * План 01.5. Команды контекста доходят до Core через ту же узкую поверхность:
 * оболочка только валидирует полезную нагрузку, а policy, approval и rate limit
 * Core проверяет заново.
 */
describe('context budget commands', () => {
  it('forwards the read-only context ledger request', () => {
    expect(invoke('core.getContextLedger', { taskId: 'task-1', limit: 20 })).toEqual({
      ok: true,
      value: { accepted: true }
    })
    expect(sent).toEqual([{ getContextLedger: { taskId: 'task-1', limit: 20 } }])
  })

  it('forwards a scratchpad read with its category and status filter', () => {
    invoke('core.listTaskScratchpad', {
      taskId: 'task-1',
      category: 'open_questions',
      status: 'confirmed',
      limit: 50
    })
    expect(sent).toEqual([
      {
        listTaskScratchpad: {
          taskId: 'task-1',
          category: 'open_questions',
          status: 'confirmed',
          limit: 50
        }
      }
    ])
  })

  it('treats an omitted filter as "no filter" rather than a rejected payload', () => {
    expect((invoke('core.listTaskScratchpad', { taskId: 'task-1' }) as { ok: boolean }).ok).toBe(
      true
    )
    expect(sent).toEqual([
      { listTaskScratchpad: { taskId: 'task-1', category: '', status: '', limit: 100 } }
    ])
  })

  it('forwards every mutation command that the plan exposes', () => {
    invoke('core.clearTaskScratchpad', { taskId: 'task-1' })
    invoke('core.summarizeContextNow', { taskId: 'task-1' })
    invoke('core.pinContextItem', { taskId: 'task-1', itemId: 'msg-0002-tool', pinned: true })
    invoke('core.readContextArtifact', { taskId: 'task-1', locator: 'artifact://task-1/abc' })
    expect(sent).toEqual([
      { clearTaskScratchpad: { taskId: 'task-1' } },
      { summarizeContextNow: { taskId: 'task-1' } },
      { pinContextItem: { taskId: 'task-1', itemId: 'msg-0002-tool', pinned: true } },
      { readContextArtifact: { taskId: 'task-1', locator: 'artifact://task-1/abc' } }
    ])
  })

  it('refuses malformed context payloads before they reach Core', () => {
    const rejected = [
      invoke('core.getContextLedger', {}),
      invoke('core.pinContextItem', { taskId: 'task-1', itemId: 'i', pinned: 'yes' }),
      invoke('core.readContextArtifact', { taskId: 'task-1' }),
      invoke('core.summarizeContextNow', {})
    ]
    for (const outcome of rejected) {
      expect((outcome as CommandFailure).ok).toBe(false)
      expect((outcome as CommandFailure).code).toBe('invalid-payload')
    }
    expect(sent).toHaveLength(0)
  })
})

describe('workspace knowledge commands', () => {
  it('forwards indexing, status and bounded retrieval without filesystem access in renderer', () => {
    invoke('core.indexWorkspace', { workspacePath: 'C:\\work', enableEmbeddings: false })
    invoke('core.rebuildIndex', { workspacePath: 'C:\\work', enableEmbeddings: true })
    invoke('core.getIndexStatus', { workspacePath: 'C:\\work' })
    invoke('core.cancelWorkspaceIndex', { workspacePath: 'C:\\work' })
    invoke('core.searchWorkspaceKnowledge', {
      workspacePath: 'C:\\work',
      query: 'validate_token',
      pathFilter: 'src',
      languageFilter: 'rust',
      hybrid: true
    })
    expect(sent).toEqual([
      { indexWorkspace: { workspacePath: 'C:\\work', enableEmbeddings: false } },
      { rebuildIndex: { workspacePath: 'C:\\work', enableEmbeddings: true } },
      { getIndexStatus: { workspacePath: 'C:\\work' } },
      { cancelWorkspaceIndex: { workspacePath: 'C:\\work' } },
      {
        searchWorkspaceKnowledge: {
          workspacePath: 'C:\\work',
          query: 'validate_token',
          pathFilter: 'src',
          languageFilter: 'rust',
          hybrid: true
        }
      }
    ])
  })

  /**
   * Без подсказки Electron открывает диалог в папке загрузок — планов там не
   * бывает. Порядок предпочтений: прошлый выбор, затем `docs/plans` рабочей
   * папки, затем сама рабочая папка.
   */
  it('opens the plan dialog where plans actually live', async () => {
    dialogOptions.length = 0
    existingDirectories.clear()
    selectedWorkspace = null

    // Ничего не известно — подсказывать нечем.
    await invoke('review.pickPlan', { directory: '' })
    expect(dialogOptions.at(-1)?.defaultPath).toBeUndefined()

    // Рабочая папка есть, отдельной папки планов в ней нет.
    selectedWorkspace = 'C:\\work'
    existingDirectories.add('C:\\work')
    await invoke('review.pickPlan', { directory: '' })
    expect(dialogOptions.at(-1)?.defaultPath).toBe('C:\\work')

    // Появилась docs/plans — она точнее рабочей папки.
    existingDirectories.add('C:\\work\\docs\\plans')
    await invoke('review.pickPlan', { directory: '' })
    expect(dialogOptions.at(-1)?.defaultPath).toBe('C:\\work\\docs\\plans')

    // Прошлый выбор пользователя важнее любых догадок.
    existingDirectories.add('D:\\другие-планы')
    await invoke('review.pickPlan', { directory: 'D:\\другие-планы' })
    expect(dialogOptions.at(-1)?.defaultPath).toBe('D:\\другие-планы')

    // Папку могли удалить или переименовать — тогда работает запасной путь.
    await invoke('review.pickPlan', { directory: 'D:\\удалённая' })
    expect(dialogOptions.at(-1)?.defaultPath).toBe('C:\\work\\docs\\plans')
    selectedWorkspace = null
  })

  it('forwards a plan revision and rejects an empty one', async () => {
    const outcome = await invoke('review.revise', {
      revisionId: 'revision-1',
      reviewId: 'review-1',
      fileName: 'plan.md',
      sourceMarkdown: '# Plan',
      model: 'main'
    })

    expect(outcome).toEqual({ ok: true, value: { accepted: true } })
    // Путь может отсутствовать: план мог прийти перетаскиванием из источника
    // без файловой системы, и правка обязана работать без соседних планов.
    expect(sent).toEqual([{
      revisePlan: { revisionId: 'revision-1', reviewId: 'review-1', fileName: 'plan.md', sourceMarkdown: '# Plan', model: 'main', sourcePath: '' }
    }])

    const empty = await invoke('review.revise', {
      revisionId: 'revision-2',
      reviewId: 'review-1',
      fileName: 'plan.md',
      sourceMarkdown: '   ',
      model: 'main'
    })
    expect((empty as CommandFailure).ok).toBe(false)
    expect(sent).toHaveLength(1)
  })

  // Путь исходного файла — единственное, по чему ядро находит соседние планы:
  // потеряв его в оболочке, правка молча снова станет слепой.
  it('forwards the plan path so the core can read the neighbouring plans', async () => {
    await invoke('review.revise', {
      revisionId: 'revision-3',
      reviewId: 'review-1',
      fileName: '04-7.md',
      sourceMarkdown: '# План',
      model: 'main',
      sourcePath: 'C:/docs/plans/04-7.md'
    })
    expect(sent.at(-1)).toEqual({
      revisePlan: { revisionId: 'revision-3', reviewId: 'review-1', fileName: '04-7.md', sourceMarkdown: '# План', model: 'main', sourcePath: 'C:/docs/plans/04-7.md' }
    })

    await invoke('review.start', {
      reviewId: 'review-2',
      fileName: '04-7.md',
      fileNames: ['04-7.md'],
      sourceMarkdown: '# План',
      reviewerModels: ['one', 'two'],
      synthesisModel: 'main',
      sourcePaths: ['C:/docs/plans/04-7.md']
    })
    expect(sent.at(-1)).toEqual({
      startPlanReview: { reviewId: 'review-2', fileName: '04-7.md', fileNames: ['04-7.md'], sourceMarkdown: '# План', reviewerModels: ['one', 'two'], synthesisModel: 'main', sourcePaths: ['C:/docs/plans/04-7.md'] }
    })
  })

  it('asks where to save a revised plan only when no path was chosen', async () => {
    await invoke('review.saveRevision', { revisionId: 'revision-1', destinationPath: 'C:\plans\plan.md' })
    expect(sent).toEqual([{ saveRevisedPlan: { revisionId: 'revision-1', destinationPath: 'C:\plans\plan.md' } }])

    // Пустой путь — просьба показать диалог сохранения.
    await invoke('review.saveRevision', { revisionId: 'revision-1', destinationPath: '', fileName: 'plan.md' })
    expect(sent.at(-1)).toEqual({ saveRevisedPlan: { revisionId: 'revision-1', destinationPath: 'G:/evohime-trace.md' } })
  })

  it('rejects malformed workspace knowledge payloads', () => {
    const outcomes = [
      invoke('core.indexWorkspace', {}),
      invoke('core.searchWorkspaceKnowledge', { workspacePath: 'C:\\work' }),
      invoke('core.getIndexStatus', {})
    ]
    for (const outcome of outcomes) {
      expect((outcome as CommandFailure).ok).toBe(false)
    }
    expect(sent).toHaveLength(0)
  })
})

describe('listener runtime bridge', () => {
  /** Renderer не считает состояние сам: он показывает ответ main-процесса. */
  it('relays status, check and download to the owning service', async () => {
    expect(await invoke('listener.getRuntimeStatus', {})).toEqual({
      ok: true,
      value: listenerRuntimeStatus
    })
    expect(await invoke('listener.checkRuntime', {})).toEqual({
      ok: true,
      value: listenerRuntimeStatus
    })
    expect(await invoke('listener.downloadRuntime', {})).toEqual({
      ok: true,
      value: listenerRuntimeStatus
    })
    expect(listenerCalls).toEqual(['status', 'check', 'download'])
  })

  it('restarts the listener after a runtime installation succeeds', async () => {
    listenerDownloadStatus = {
      ...listenerRuntimeStatus,
      state: 'ready',
      installedVersion: 'whisper-v1.9.3-r1',
      message: 'Установлена версия whisper-v1.9.3-r1.'
    }

    const outcome = await invoke('listener.downloadRuntime', {})

    expect(outcome).toEqual({ ok: true, value: listenerDownloadStatus })
    expect(restarts).toEqual([true])
  })
})

describe('ambient listening bridge', () => {
  /** Оболочка только пересылает: ядро заново проверяет всё, что важно. */
  it('forwards the three-field listening command as one Core command', () => {
    expect(invoke('ambient.setListening', { enabled: true, paused: false, deviceId: 'mic-2' })).toEqual({
      ok: true,
      value: { accepted: true }
    })
    expect(sent).toEqual([
      { setAmbientListening: { enabled: true, paused: false, deviceId: 'mic-2' } }
    ])
  })

  it('refuses a listening command whose fields are not booleans', () => {
    expect(invoke('ambient.setListening', { enabled: 'да', paused: false })).toMatchObject({
      ok: false,
      code: 'invalid-payload'
    })
    expect(sent).toHaveLength(0)
  })

  /**
   * Подтверждение проверяется и здесь, и в ядре. Отправить удаление без него
   * нельзя даже в обход панели.
   */
  it('never forwards an unconfirmed deletion', () => {
    expect(invoke('ambient.deleteTranscripts', { all: true, confirmed: false })).toMatchObject({
      ok: false,
      code: 'invalid-payload'
    })
    expect(invoke('ambient.forgetWindow', { windowMs: 300_000, confirmed: false })).toMatchObject({
      ok: false,
      code: 'invalid-payload'
    })
    expect(sent).toHaveLength(0)
  })

  it('forwards a confirmed deletion and a confirmed forget window', () => {
    invoke('ambient.deleteTranscripts', { episodeIds: ['ep-1'], confirmed: true })
    invoke('ambient.forgetWindow', { windowMs: 300_000, confirmed: true })
    expect(sent).toEqual([
      { deleteAmbientTranscripts: { episodeIds: ['ep-1'], all: false, confirmed: true } },
      { forgetAmbientWindow: { windowMs: 300_000, confirmed: true } }
    ])
  })

  it('forwards the policy whole and refuses a malformed quiet window', () => {
    invoke('ambient.savePolicy', {
      quietHours: [{ startMinute: 1380, endMinute: 420 }],
      blocklistPatterns: ['zoom*.exe'],
      windowTitleBlocklist: [],
      retentionDays: 14
    })
    expect(sent).toEqual([
      {
        saveAmbientPolicy: {
          policy: {
            quietHours: [{ startMinute: 1380, endMinute: 420 }],
            blocklistPatterns: ['zoom*.exe'],
            windowTitleBlocklist: [],
            retentionDays: 14
          }
        }
      }
    ])
    sent.length = 0
    expect(
      invoke('ambient.savePolicy', {
        quietHours: [{ startMinute: 5000, endMinute: 10 }],
        blocklistPatterns: [],
        windowTitleBlocklist: [],
        retentionDays: 14
      })
    ).toMatchObject({ ok: false, code: 'invalid-payload' })
    expect(sent).toHaveLength(0)
  })

  it('forwards voice-command listing and a decision, and refuses a nameless one', () => {
    invoke('ambient.listVoiceCommands', {})
    invoke('ambient.resolveVoiceCommand', { commandId: 'voice-1', accepted: true })
    expect(sent).toEqual([
      { listVoiceCommands: { limit: 8 } },
      { resolveVoiceCommand: { commandId: 'voice-1', accepted: true } }
    ])
    sent.length = 0
    expect(invoke('ambient.resolveVoiceCommand', { accepted: true })).toMatchObject({
      ok: false,
      code: 'invalid-payload'
    })
    expect(sent).toHaveLength(0)
  })

  /**
   * Старый вызов без голосовых полей не должен молча их выключать: поля не
   * попадают в сообщение вовсе, и Core подставляет сохранённое значение.
   */
  it('omits the voice fields when the caller did not set them', () => {
    invoke('ambient.savePolicy', {
      quietHours: [],
      blocklistPatterns: [],
      windowTitleBlocklist: [],
      retentionDays: 7
    })
    invoke('ambient.savePolicy', {
      quietHours: [],
      blocklistPatterns: [],
      windowTitleBlocklist: [],
      retentionDays: 7,
      voiceCommands: false,
      voiceCommandsAutorun: true
    })
    expect(sent).toEqual([
      {
        saveAmbientPolicy: {
          policy: {
            quietHours: [],
            blocklistPatterns: [],
            windowTitleBlocklist: [],
            retentionDays: 7
          }
        }
      },
      {
        saveAmbientPolicy: {
          policy: {
            quietHours: [],
            blocklistPatterns: [],
            windowTitleBlocklist: [],
            retentionDays: 7,
            voiceCommands: false,
            voiceCommandsAutorun: true
          }
        }
      }
    ])
  })

  it('reads the status, the episodes, one episode and the policy', () => {
    invoke('ambient.getStatus', {})
    invoke('ambient.listEpisodes', { limit: 10 })
    invoke('ambient.getEpisode', { episodeId: 'ep-1' })
    invoke('ambient.getPolicy', {})
    invoke('ambient.listProposals', { limit: 25 })
    invoke('ambient.resolveProposal', {
      proposalId: 'prop-1',
      accepted: true,
      idempotencyKey: 'idem-1'
    })
    expect(sent).toEqual([
      { getAmbientStatus: {} },
      { listAmbientEpisodes: { sinceMs: 0, limit: 10, cursor: '' } },
      { getAmbientEpisode: { episodeId: 'ep-1' } },
      { getAmbientPolicy: {} },
      { listAmbientProposals: { limit: 25 } },
      {
        resolveAmbientProposal: {
          proposalId: 'prop-1',
          accepted: true,
          idempotencyKey: 'idem-1',
          mute: false
        }
      }
    ])
  })

  /**
   * Принятие создаёт задачу, поэтому запрос без ключа идемпотентности не
   * должен доходить до ядра вовсе: двойной клик породил бы две задачи.
   */
  it('refuses a proposal decision without an idempotency key', () => {
    expect(invoke('ambient.resolveProposal', { proposalId: 'prop-1', accepted: true })).toMatchObject({
      ok: false,
      code: 'invalid-payload'
    })
    expect(sent).toHaveLength(0)
  })

  /** Доступность хоткея знает только main; ядру этот вопрос не задаётся. */
  it('answers the hotkey question without touching Core', () => {
    expect(invoke('ambient.hotkeyStatus', {})).toEqual({
      ok: true,
      value: { combination: 'Control+Alt+M', registered: true }
    })
    expect(sent).toHaveLength(0)
  })
})

describe('workflow orchestration bridge', () => {
  /**
   * Оболочка только пересылает намерение. Ни граф, ни порядок узлов, ни
   * зависимости здесь не вычисляются: всё это принадлежит ядру.
   */
  it('forwards the six workflow commands unchanged', () => {
    expect(invoke('workflow.listTemplates', {})).toEqual({ ok: true, value: { accepted: true } })
    invoke('workflow.getDefinition', { templateId: 'repository-research' })
    invoke('workflow.start', {
      templateId: 'repository-research',
      workspacePath: 'C:\\work',
      inputs: { question: 'как устроен supervisor' },
      idempotencyKey: 'key-1'
    })
    invoke('workflow.getRun', { runId: 'run-1' })
    invoke('workflow.listEvents', { runId: 'run-1', afterSequence: 3, limit: 50 })
    invoke('workflow.cancel', { runId: 'run-1' })

    expect(sent).toEqual([
      { listWorkflowTemplates: {} },
      { getWorkflowDefinition: { templateId: 'repository-research' } },
      {
        startWorkflow: {
          templateId: 'repository-research',
          taskId: '',
          workspacePath: 'C:\\work',
          inputs: [{ name: 'question', value: 'как устроен supervisor' }],
          idempotencyKey: 'key-1'
        }
      },
      { getWorkflowRun: { runId: 'run-1' } },
      { listWorkflowEvents: { runId: 'run-1', afterSequence: 3, limit: 50 } },
      { cancelWorkflow: { runId: 'run-1' } }
    ])
  })

  /** Запуск без ключа идемпотентности не доходит до ядра. */
  it('refuses a start without an idempotency key or a workspace', () => {
    expect(
      invoke('workflow.start', {
        templateId: 'repository-research',
        workspacePath: 'C:\\work',
        inputs: {}
      })
    ).toMatchObject({ ok: false, code: 'invalid-payload' })
    expect(
      invoke('workflow.start', {
        templateId: 'repository-research',
        inputs: {},
        idempotencyKey: 'key-1'
      })
    ).toMatchObject({ ok: false, code: 'invalid-payload' })
    expect(sent).toHaveLength(0)
  })

  /**
   * Входы шаблона — плоская карта строк. Вложенный объект или список длиннее
   * лимита не должен уходить в очередь команд.
   */
  it('refuses inputs that are not a bounded flat string map', () => {
    expect(
      invoke('workflow.start', {
        templateId: 'repository-research',
        workspacePath: 'C:\\work',
        inputs: { nested: { evil: true } } as unknown as Record<string, string>,
        idempotencyKey: 'key-1'
      })
    ).toMatchObject({ ok: false, code: 'invalid-payload' })

    const tooMany: Record<string, string> = {}
    for (let index = 0; index < 17; index += 1) tooMany[`field-${index}`] = 'x'
    expect(
      invoke('workflow.start', {
        templateId: 'repository-research',
        workspacePath: 'C:\\work',
        inputs: tooMany,
        idempotencyKey: 'key-1'
      })
    ).toMatchObject({ ok: false, code: 'invalid-payload' })
    expect(sent).toHaveLength(0)
  })

  /** Позиция replay не может быть отрицательной сверх `-1` или дробной. */
  it('refuses a malformed replay position or run id', () => {
    expect(
      invoke('workflow.listEvents', { runId: 'run-1', afterSequence: -5 })
    ).toMatchObject({ ok: false, code: 'invalid-payload' })
    expect(invoke('workflow.getRun', { runId: '' })).toMatchObject({
      ok: false,
      code: 'invalid-payload'
    })
    expect(invoke('workflow.cancel', {})).toMatchObject({ ok: false, code: 'invalid-payload' })
    expect(sent).toHaveLength(0)
  })
})
