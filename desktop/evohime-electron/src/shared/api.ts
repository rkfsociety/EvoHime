/**
 * Contract shared by preload and renderer.
 *
 * This file must stay free of Electron and Node imports: it is compiled into
 * the sandboxed renderer bundle, where neither is available.
 */

export const API_NAMESPACE = 'evohime'
export const API_VERSION = 1 as const

/** Renderer-visible lifecycle of the Core connection owned by the main process. */
export type ConnectionState =
  | 'starting'
  | 'connecting'
  | 'connected'
  | 'reconnecting'
  | 'replaying'
  | 'resyncing'
  | 'state-gap'
  | 'version-mismatch'
  | 'degraded'
  | 'fatal'

export interface ProtocolVersion {
  readonly major: number
  readonly minor: number
}

export interface ShellState {
  readonly connection: ConnectionState
  /** Negotiated protocol, present only once a handshake succeeded. */
  readonly protocol: ProtocolVersion | null
  readonly capabilities: readonly string[]
  readonly coreVersion: string | null
  readonly lastSequence: number
  /** Bounded, already redacted reason for the current state. */
  readonly reason: string | null
  readonly reconnectAttempts: number
}

export interface CoreEvent {
  readonly sequenceId: number
  readonly taskId: string
  readonly eventType: string
  /** Redacted UTF-8 payload as produced by Core; never a secret value. */
  readonly payload: string
}

export type ShellEvent =
  | { readonly kind: 'state'; readonly state: ShellState }
  | { readonly kind: 'core-event'; readonly event: CoreEvent }

/** Model providers the shell can configure. */
export const PROVIDER_KINDS = ['literouter', 'openai_compatible', 'mock'] as const

export type ProviderKind = (typeof PROVIDER_KINDS)[number]

/** Which half of the provider catalogue the user works with. */
export type ModelTier = 'free' | 'paid'


/**
 * Secret-free view of the stored provider settings. The key itself never
 * crosses into the renderer — only whether one is stored.
 */
export interface ProviderSummary {
  readonly provider: ProviderKind
  readonly model: string
  readonly baseUrl: string
  readonly tier: ModelTier
  readonly configured: boolean
}

/** Where the shell learned the user's name. */
export type IdentitySource = 'github' | 'git' | 'os'

export interface UserIdentity {
  readonly name: string
  readonly source: IdentitySource
}

/** One prompt the user sent from a chat, and the task it started. */
export interface ChatMessage {
  readonly taskId: string
  readonly prompt: string
  readonly atMs: number
}

/** A conversation of the shell, scoped to one workspace. */
export interface ChatRecord {
  readonly id: string
  readonly workspacePath: string
  readonly title: string
  readonly createdMs: number
  readonly updatedMs: number
  /** Tasks started from this chat; the transcript is filtered by them. */
  readonly taskIds: readonly string[]
  readonly messages: readonly ChatMessage[]
}

/** Row of the chat list: enough to render it without loading transcripts. */
export interface ChatSummary {
  readonly id: string
  readonly workspacePath: string
  readonly title: string
  readonly updatedMs: number
  readonly messageCount: number
}

/**
 * Commands the renderer may ask the main process to forward to Core. The main
 * process only forwards; Core re-validates capability, policy and approval for
 * every one of them.
 */
export const RENDERER_COMMANDS = [
  'shell.getState',
  'shell.requestResync',
  'workspace.list',
  'workspace.pick',
  'workspace.select',
  'workspace.forget',
  'core.startTask',
  'core.stopTask',
  'core.resolveApproval',
  'core.listWorkspace',
  'core.readWorkspaceFile',
  'core.gitStatus',
  'core.gitDiff',
  'core.setPermissionMode',
  'core.runDoctor',
  'core.exportDoctorLogs',
  'core.createDatabaseBackup',
  'core.prepareDatabaseRestore',
  'core.restoreDatabase',
  'core.getModelConfig',
  'core.listModelCatalog',
  'core.selectModel',
  'identity.get',
  'chat.list',
  'chat.create',
  'chat.open',
  'chat.appendPrompt',
  'chat.remove',
  'provider.get',
  'provider.save',
  'provider.clearKey',
  'core.createProject',
  'core.prepareBuild',
  'core.applyApprovedBuild',
  'core.terminalExecute'
] as const

export type RendererCommand = (typeof RENDERER_COMMANDS)[number]

/** One remembered workspace and whether it is still usable on this machine. */
export interface WorkspaceOption {
  readonly path: string
  /** False when the directory is gone or unreadable; the UI says so. */
  readonly available: boolean
  readonly lastUsedMs: number
}

export interface WorkspaceSelection {
  readonly selected: string | null
  readonly options: readonly WorkspaceOption[]
}

export interface CommandPayloads {
  'shell.getState': Record<string, never>
  'shell.requestResync': Record<string, never>
  'workspace.list': Record<string, never>
  'workspace.pick': Record<string, never>
  'workspace.select': { path: string }
  'workspace.forget': { path: string }
  'core.startTask': { taskId: string; prompt: string; workspacePath: string }
  'core.stopTask': { taskId: string }
  'core.resolveApproval': { approvalId: string; granted: boolean }
  'core.listWorkspace': { workspacePath: string; relativePath: string; maxEntries?: number }
  'core.readWorkspaceFile': { workspacePath: string; relativePath: string; maxBytes?: number }
  'core.gitStatus': { workspacePath: string; maxBytes?: number }
  'core.gitDiff': { workspacePath: string; relativePath?: string; maxBytes?: number }
  'core.setPermissionMode': { mode: 'ask' | 'read_only' | 'full' }
  'core.runDoctor': { projectId?: string; detailLevel?: 0 | 1 }
  'core.exportDoctorLogs': { destinationPath: string }
  'core.createDatabaseBackup': { destinationPath: string }
  'core.prepareDatabaseRestore': { backupPath: string }
  'core.restoreDatabase': { backupPath: string; approvalId: string }
  'core.getModelConfig': Record<string, never>
  'core.listModelCatalog': { mode: ModelTier }
  'core.selectModel': { model: string }
  'identity.get': Record<string, never>
  'chat.list': { workspacePath: string }
  'chat.create': { workspacePath: string }
  'chat.open': { chatId: string }
  'chat.appendPrompt': { chatId: string; taskId: string; prompt: string }
  'chat.remove': { chatId: string }
  'provider.get': Record<string, never>
  'provider.save': { provider: ProviderKind; apiKey: string; model: string; baseUrl: string; tier: ModelTier }
  'provider.clearKey': Record<string, never>
  'core.createProject': { projectId: string; title: string; workspacePath: string; sourceRef?: string }
  'core.prepareBuild': { projectId: string; proposalJson: string }
  'core.applyApprovedBuild': { projectId: string; runId: string; taskId: string; approvedBuildJson: string }
  'core.terminalExecute': { taskId: string; workspacePath: string; program: string; args: readonly string[]; cwd?: string; timeoutMs?: number; approvalId?: string }
}

export interface CommandResults {
  'shell.getState': ShellState
  'shell.requestResync': { accepted: boolean }
  'workspace.list': WorkspaceSelection
  /** `cancelled` when the user closed the native folder dialog. */
  'workspace.pick': { cancelled: boolean; selection: WorkspaceSelection }
  'workspace.select': WorkspaceSelection
  'workspace.forget': WorkspaceSelection
  'core.startTask': { accepted: boolean }
  'core.stopTask': { accepted: boolean }
  'core.resolveApproval': { accepted: boolean }
  'core.listWorkspace': { accepted: boolean }
  'core.readWorkspaceFile': { accepted: boolean }
  'core.gitStatus': { accepted: boolean }
  'core.gitDiff': { accepted: boolean }
  'core.setPermissionMode': { accepted: boolean }
  'core.runDoctor': { accepted: boolean }
  'core.exportDoctorLogs': { accepted: boolean }
  'core.createDatabaseBackup': { accepted: boolean }
  'core.prepareDatabaseRestore': { accepted: boolean }
  'core.restoreDatabase': { accepted: boolean }
  'core.getModelConfig': { accepted: boolean }
  'core.listModelCatalog': { accepted: boolean }
  'core.selectModel': { accepted: boolean }
  'identity.get': UserIdentity
  'chat.list': readonly ChatSummary[]
  'chat.create': ChatRecord
  'chat.open': ChatRecord | null
  'chat.appendPrompt': ChatRecord | null
  'chat.remove': readonly ChatSummary[]
  'provider.get': ProviderSummary
  /** `restarted` is false when Core could not be relaunched with the new key. */
  'provider.save': { summary: ProviderSummary; restarted: boolean }
  'provider.clearKey': { summary: ProviderSummary; restarted: boolean }
  'core.createProject': { accepted: boolean }
  'core.prepareBuild': { accepted: boolean }
  'core.applyApprovedBuild': { accepted: boolean }
  'core.terminalExecute': { accepted: boolean }
}

export type CommandFailureCode =
  | 'unknown-command'
  | 'invalid-payload'
  | 'workspace-unavailable'
  | 'not-connected'
  | 'queue-full'
  | 'timeout'
  | 'protocol-error'

export interface CommandFailure {
  readonly ok: false
  readonly code: CommandFailureCode
  /** Bounded, redacted message safe to show in the UI. */
  readonly message: string
}

export type CommandOutcome<C extends RendererCommand> =
  | { readonly ok: true; readonly value: CommandResults[C] }
  | CommandFailure

/** The only object exposed on `window`. */
export interface EvoHimeApiV1 {
  readonly apiVersion: typeof API_VERSION
  invoke<C extends RendererCommand>(
    command: C,
    payload: CommandPayloads[C]
  ): Promise<CommandOutcome<C>>
  /** Returns an unsubscribe function. */
  subscribe(listener: (event: ShellEvent) => void): () => void
  /** Copies bounded plain text; never reads the clipboard back. */
  writeClipboardText(text: string): Promise<boolean>
  /** Opens an https URL that passed the main-process allow-list. */
  openExternal(url: string): Promise<boolean>
}

declare global {
  interface Window {
    readonly evohime?: { readonly v1: EvoHimeApiV1 }
  }
}
