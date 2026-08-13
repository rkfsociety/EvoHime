import { beforeEach, describe, expect, it, vi } from 'vitest'

import type {
  CommandFailure,
  ProviderSummary,
  RendererCommand,
  ShellState
} from '../src/shared/api'
import type { ProviderUpdate } from '../src/main/provider-store'
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
    getAllWindows: () => []
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
      configured: update.apiKey.length > 0 || providerSummary.configured
    }
    return providerSummary
  },
  clearKey: () => {
    providerSummary = { ...providerSummary, configured: false }
    return providerSummary
  }
}

/** Stands in for the workspace service; its own behaviour is tested separately. */
const workspaces = {
  list: () => ({ selected: null, options: [] }),
  pick: async () => ({ cancelled: true, selection: { selected: null, options: [] } }),
  select: () => 'unknown-workspace' as const,
  forget: () => ({ selected: null, options: [] })
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
  providerSummary = { provider: 'literouter', model: '', baseUrl: '', configured: false }
  providerWrites.length = 0
  restarts.length = 0
  registerShellBridge({
    client: client as never,
    workspaces: workspaces as never,
    providers: providers as never,
    restartCore: async () => {
      restarts.push(true)
      return true
    },
    log: () => {}
  })
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
      { startTask: { taskId: 'task-1', prompt: 'сделай', workspacePath: 'C:\\work' } }
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

  it('forwards policy, diagnostics and backup commands using the canonical proto fields', () => {
    expect(invoke('core.setPermissionMode', { mode: 'read_only' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.runDoctor', { detailLevel: 1 })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.exportDoctorLogs', { destinationPath: 'C:\\diagnostics.jsonl' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.createDatabaseBackup', { destinationPath: 'C:\\backup.evohime' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.prepareDatabaseRestore', { backupPath: 'C:\\backup.evohime' })).toEqual({ ok: true, value: { accepted: true } })
    expect(invoke('core.restoreDatabase', { backupPath: 'C:\\backup.evohime', approvalId: 'approval-1' })).toEqual({ ok: true, value: { accepted: true } })
    expect(sent).toContainEqual({ permissionMode: { mode: 'read_only' } })
    expect(sent).toContainEqual({ runDoctor: { projectId: '', detailLevel: 1 } })
    expect(sent).toContainEqual({ exportDoctorLogs: { destinationPath: 'C:\\diagnostics.jsonl' } })
    expect(sent).toContainEqual({ createDatabaseBackup: { destinationPath: 'C:\\backup.evohime' } })
    expect(sent).toContainEqual({ prepareDatabaseRestore: { backupPath: 'C:\\backup.evohime' } })
    expect(sent).toContainEqual({ restoreDatabase: { backupPath: 'C:\\backup.evohime', approvalId: 'approval-1' } })
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
      baseUrl: 'https://api.literouter.com/v1'
    })) as { ok: true; value: { summary: ProviderSummary; restarted: boolean } }

    expect(outcome.value.summary.configured).toBe(true)
    expect(outcome.value.restarted).toBe(true)
    expect(restarts).toHaveLength(1)
    // The key belongs to the main process; no Core command may carry it.
    expect(JSON.stringify(sent)).not.toContain('sk-secret-value')
    expect(invoke('provider.get', {})).toEqual({
      ok: true,
      value: { provider: 'literouter', model: 'deepseek:free', baseUrl: 'https://api.literouter.com/v1', configured: true }
    })
  })

  it('rejects a provider endpoint that would leak the key over plain http', async () => {
    const outcome = (await invoke('provider.save', {
      provider: 'literouter',
      apiKey: 'sk-secret-value',
      model: '',
      baseUrl: 'http://example.com/v1'
    })) as CommandFailure

    expect(outcome.ok).toBe(false)
    expect(outcome.code).toBe('invalid-payload')
    expect(providerWrites).toHaveLength(0)
    expect(restarts).toHaveLength(0)
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
