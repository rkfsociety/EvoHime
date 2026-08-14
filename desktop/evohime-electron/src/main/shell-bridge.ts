import { BrowserWindow, clipboard, ipcMain, shell } from 'electron'

import {
  PROVIDER_KINDS,
  RENDERER_COMMANDS,
  type CommandFailure,
  type ProviderKind,
  type RendererCommand,
  type ShellEvent
} from '@shared/api'
import {
  CLIPBOARD_CHANNEL,
  EVENT_CHANNEL,
  INVOKE_CHANNEL,
  OPEN_EXTERNAL_CHANNEL
} from '@shared/channels'

import type { ShellLog } from './diagnostics/logger'
import type { CorePipeClient } from './ipc/pipe-client'
import type { ChatStore } from './chat-store'
import { resolveIdentity, resolveRepository } from './identity'
import {
  normalizeApiKey,
  normalizeBaseUrl,
  normalizeModel,
  type ProviderStore
} from './provider-store'
import { isAllowedExternalUrl } from './security-policy'
import type { UpdateService } from './update/update-service'
import type { WorkspaceService } from './workspace-service'

/**
 * Translates the narrow renderer API into `desktop-ipc-v1` commands.
 *
 * Every payload is shape-checked here before it is forwarded, but that check is
 * a robustness measure only: Core remains the single security authority and
 * re-validates capability, policy, paths and approvals for each command.
 */

const MAX_TEXT_FIELD_CHARS = 4_096
const MAX_CLIPBOARD_CHARS = 64 * 1024

export interface ShellBridgeOptions {
  readonly client: CorePipeClient
  readonly workspaces: WorkspaceService
  readonly providers: ProviderStore
  readonly chats: ChatStore
  /**
   * Relaunches Core so it picks up the stored credentials: the model gateway
   * is built from the environment at Core startup and has no live setter.
   */
  readonly restartCore: () => Promise<boolean>
  /** Owns the source update; the renderer may only observe and trigger it. */
  readonly updates: UpdateService
  readonly log: ShellLog
}

export function registerShellBridge(options: ShellBridgeOptions): void {
  const { log } = options

  ipcMain.handle(INVOKE_CHANNEL, (_event, command: unknown, payload: unknown) => {
    if (typeof command !== 'string' || !isRendererCommand(command)) {
      log('warn', 'shell.unknown_command', {})
      return failure('unknown-command', 'Команда не поддерживается оболочкой.')
    }
    return dispatch(options, command, payload)
  })

  ipcMain.handle(CLIPBOARD_CHANNEL, (_event, text: unknown) => {
    if (typeof text !== 'string' || text.length === 0 || text.length > MAX_CLIPBOARD_CHARS) {
      return false
    }
    clipboard.writeText(text)
    return true
  })

  ipcMain.handle(OPEN_EXTERNAL_CHANNEL, async (_event, url: unknown) => {
    if (typeof url !== 'string' || !isAllowedExternalUrl(url)) {
      log('warn', 'shell.open_external_denied', {})
      return false
    }
    await shell.openExternal(url)
    return true
  })
}

export function broadcast(event: ShellEvent): void {
  for (const window of BrowserWindow.getAllWindows()) {
    if (!window.isDestroyed()) {
      window.webContents.send(EVENT_CHANNEL, event)
    }
  }
}

function dispatch(
  options: ShellBridgeOptions,
  command: RendererCommand,
  payload: unknown
): unknown {
  const { client, workspaces, providers, chats, restartCore, updates, log } = options
  switch (command) {
    case 'shell.getState':
      return { ok: true, value: client.state }

    case 'shell.requestResync':
      return accepted(client.requestResync(true))

    case 'workspace.list':
      return { ok: true, value: workspaces.list() }

    case 'workspace.pick':
      // The native folder dialog lives in the main process; the renderer only
      // ever receives the resulting path.
      return workspaces.pick().then((value) => ({ ok: true, value }))

    case 'workspace.select': {
      const path = asBoundedString(asRecord(payload)['path'])
      if (path === null) {
        return failure('invalid-payload', 'Некорректный путь рабочей папки.')
      }
      const selection = workspaces.select(path)
      if (selection === 'unknown-workspace') {
        log('warn', 'shell.workspace_select_rejected', {})
        return failure('workspace-unavailable', 'Эта папка не выбрана ранее — выбери её заново.')
      }
      log('info', 'shell.workspace_selected', {})
      return { ok: true, value: selection }
    }

    case 'workspace.forget': {
      const path = asBoundedString(asRecord(payload)['path'])
      if (path === null) {
        return failure('invalid-payload', 'Некорректный путь рабочей папки.')
      }
      log('info', 'shell.workspace_forgotten', {})
      return { ok: true, value: workspaces.forget(path) }
    }

    case 'core.startTask': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const prompt = asBoundedString(value['prompt'])
      const workspacePath = asBoundedString(value['workspacePath'])
      if (taskId === null || prompt === null || workspacePath === null) {
        return failure('invalid-payload', 'Некорректные параметры задачи.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ startTask: { taskId, prompt, workspacePath } }))
    }

    case 'core.stopTask': {
      const taskId = asBoundedString(asRecord(payload)['taskId'])
      if (taskId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор задачи.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ stopTask: { taskId } }))
    }

    case 'core.resolveApproval': {
      const value = asRecord(payload)
      const approvalId = asBoundedString(value['approvalId'])
      const granted = value['granted']
      if (approvalId === null || typeof granted !== 'boolean') {
        return failure('invalid-payload', 'Некорректное решение по approval.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ resolveApproval: { approvalId, granted } }))
    }

    case 'core.listWorkspace': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const relativePath = asRelativePath(value['relativePath'])
      const maxEntries = asBoundedNumber(value['maxEntries'], 200)
      if (workspacePath === null || relativePath === null || maxEntries === null) {
        return failure('invalid-payload', 'Некорректные параметры списка файлов.')
      }
      return accepted(client.send({ listWorkspace: { workspacePath, relativePath, maxEntries } }))
    }

    case 'core.readWorkspaceFile': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const relativePath = asRelativePath(value['relativePath'])
      const maxBytes = asBoundedNumber(value['maxBytes'], 512 * 1024)
      if (workspacePath === null || relativePath === null || maxBytes === null) {
        return failure('invalid-payload', 'Некорректные параметры чтения файла.')
      }
      return accepted(client.send({ readWorkspaceFile: { workspacePath, relativePath, maxBytes } }))
    }

    case 'core.gitStatus': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const maxBytes = asBoundedNumber(value['maxBytes'], 512 * 1024)
      if (workspacePath === null || maxBytes === null) {
        return failure('invalid-payload', 'Некорректные параметры Git status.')
      }
      return accepted(client.send({ gitStatus: { workspacePath, maxBytes } }))
    }

    case 'core.gitDiff': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const relativePath = value['relativePath'] === undefined ? '' : asRelativePath(value['relativePath'])
      const maxBytes = asBoundedNumber(value['maxBytes'], 512 * 1024)
      if (workspacePath === null || relativePath === null || maxBytes === null) {
        return failure('invalid-payload', 'Некорректные параметры Git diff.')
      }
      return accepted(client.send({ gitDiff: { workspacePath, relativePath, maxBytes } }))
    }

    case 'core.setPermissionMode': {
      const mode = asPermissionMode(asRecord(payload)['mode'])
      if (mode === null) return failure('invalid-payload', 'Некорректный режим разрешений.')
      return accepted(client.send({ permissionMode: { mode } }))
    }

    case 'core.runDoctor': {
      const value = asRecord(payload)
      const projectId = value['projectId'] === undefined ? '' : asBoundedString(value['projectId'])
      const detailLevel = value['detailLevel'] === undefined ? 0 : value['detailLevel']
      if (projectId === null || (detailLevel !== 0 && detailLevel !== 1)) {
        return failure('invalid-payload', 'Некорректные параметры диагностики.')
      }
      return accepted(client.send({ runDoctor: { projectId, detailLevel } }))
    }

    case 'core.exportDoctorLogs': {
      const destinationPath = asBoundedString(asRecord(payload)['destinationPath'])
      if (destinationPath === null) return failure('invalid-payload', 'Некорректный путь экспорта диагностики.')
      return accepted(client.send({ exportDoctorLogs: { destinationPath } }))
    }

    case 'core.createDatabaseBackup': {
      const value = asRecord(payload)
      const destinationPath = asBoundedString(value['destinationPath'])
      if (destinationPath === null) {
        return failure('invalid-payload', 'Некорректные параметры backup.')
      }
      return accepted(client.send({ createDatabaseBackup: { destinationPath } }))
    }

    case 'core.prepareDatabaseRestore': {
      const value = asRecord(payload)
      const backupPath = asBoundedString(value['backupPath'])
      if (backupPath === null) {
        return failure('invalid-payload', 'Некорректные параметры проверки backup.')
      }
      return accepted(client.send({ prepareDatabaseRestore: { backupPath } }))
    }

    case 'core.restoreDatabase': {
      const value = asRecord(payload)
      const backupPath = asBoundedString(value['backupPath'])
      const approvalId = asBoundedString(value['approvalId'])
      if (backupPath === null || approvalId === null) {
        return failure('invalid-payload', 'Некорректные параметры восстановления.')
      }
      return accepted(client.send({ restoreDatabase: { backupPath, approvalId } }))
    }

    case 'core.cancelDatabaseOperation': {
      const value = asRecord(payload)
      const operationId = asBoundedString(value['operationId'])
      if (operationId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор операции.')
      }
      return accepted(client.send({ cancelDatabaseOperation: { operationId } }))
    }

    case 'core.getModelConfig':
      return accepted(client.send({ modelConfig: {} }))

    case 'core.listModelCatalog': {
      const mode = asModelCatalogMode(asRecord(payload)['mode'])
      if (mode === null) return failure('invalid-payload', 'Некорректный режим каталога моделей.')
      return accepted(client.send({ modelCatalog: { mode } }))
    }

    case 'core.selectModel': {
      const model = normalizeModel(asRecord(payload)['model'])
      if (model === null) {
        return failure('invalid-payload', 'Некорректный идентификатор модели.')
      }
      return accepted(client.send({ selectModel: { model } }))
    }

    case 'identity.get':
      return resolveIdentity().then((value) => ({ ok: true, value }))

    case 'repository.get': {
      const workspacePath = asBoundedString(asRecord(payload)['workspacePath'])
      if (workspacePath === null) return failure('invalid-payload', 'Некорректный путь проекта.')
      return resolveRepository(workspacePath).then((value) => ({ ok: true, value }))
    }

    case 'chat.list': {
      const workspacePath = asBoundedString(asRecord(payload)['workspacePath'])
      if (workspacePath === null) return failure('invalid-payload', 'Некорректный путь проекта.')
      return { ok: true, value: chats.list(workspacePath) }
    }

    case 'chat.create': {
      const workspacePath = asBoundedString(asRecord(payload)['workspacePath'])
      const chat = workspacePath === null ? null : chats.create(workspacePath)
      if (chat === null) return failure('invalid-payload', 'Некорректный путь проекта.')
      return { ok: true, value: chat }
    }

    case 'chat.open': {
      const chatId = asBoundedString(asRecord(payload)['chatId'])
      if (chatId === null) return failure('invalid-payload', 'Некорректный идентификатор чата.')
      return { ok: true, value: chats.open(chatId) }
    }

    case 'chat.appendPrompt': {
      const value = asRecord(payload)
      const chatId = asBoundedString(value['chatId'])
      const taskId = asBoundedString(value['taskId'])
      const prompt = asBoundedString(value['prompt'])
      if (chatId === null || taskId === null || prompt === null) {
        return failure('invalid-payload', 'Некорректное сообщение чата.')
      }
      return { ok: true, value: chats.appendPrompt(chatId, taskId, prompt) }
    }

    case 'chat.remove': {
      const value = asRecord(payload)
      const chatId = asBoundedString(value['chatId'])
      if (chatId === null) return failure('invalid-payload', 'Некорректный идентификатор чата.')
      const chat = chats.open(chatId)
      chats.remove(chatId)
      return { ok: true, value: chat ? chats.list(chat.workspacePath) : [] }
    }

    case 'provider.get':
      return { ok: true, value: providers.summary() }

    case 'provider.save': {
      const value = asRecord(payload)
      const provider = asProviderKind(value['provider'])
      const apiKey = normalizeApiKey(value['apiKey'])
      const model = normalizeModel(value['model'])
      const baseUrl = normalizeBaseUrl(value['baseUrl'])
      const tier = asModelCatalogMode(value['tier'])
      if (
        provider === null ||
        apiKey === null ||
        model === null ||
        baseUrl === null ||
        tier === null
      ) {
        return failure('invalid-payload', 'Проверь ключ, модель и адрес: адрес должен быть https.')
      }
      const summary = providers.save({ provider, apiKey, model, baseUrl, tier })
      if (summary === null) {
        log('error', 'shell.provider_encryption_unavailable', {})
        return failure('protocol-error', 'Windows не даёт зашифровать ключ — он не сохранён.')
      }
      // Never log the value, only that a write happened.
      log('info', 'shell.provider_saved', { provider, configured: summary.configured })
      return restartCore().then((restarted) => ({ ok: true, value: { summary, restarted } }))
    }

    case 'provider.clearKey': {
      const summary = providers.clearKey()
      log('info', 'shell.provider_key_cleared', {})
      return restartCore().then((restarted) => ({ ok: true, value: { summary, restarted } }))
    }

    case 'core.createProject': {
      const value = asRecord(payload)
      const projectId = asBoundedString(value['projectId'])
      const title = asBoundedString(value['title'])
      const workspacePath = asBoundedString(value['workspacePath'])
      const sourceRef = value['sourceRef'] === undefined ? '' : asBoundedString(value['sourceRef'])
      if (projectId === null || title === null || workspacePath === null || sourceRef === null) {
        return failure('invalid-payload', 'Некорректные параметры проекта.')
      }
      return accepted(client.send({ createProject: { projectId, title, workspacePath, sourceRef } }))
    }

    case 'core.prepareBuild': {
      const value = asRecord(payload)
      const projectId = asBoundedString(value['projectId'])
      const proposalJson = asBoundedPayload(value['proposalJson'])
      if (projectId === null || proposalJson === null) {
        return failure('invalid-payload', 'Некорректное Build-предложение.')
      }
      return accepted(client.send({ prepareBuild: { projectId, proposalJson: Buffer.from(proposalJson, 'utf8') } }))
    }

    case 'core.applyApprovedBuild': {
      const value = asRecord(payload)
      const projectId = asBoundedString(value['projectId'])
      const runId = asBoundedString(value['runId'])
      const taskId = asBoundedString(value['taskId'])
      const approvedBuildJson = asBoundedPayload(value['approvedBuildJson'])
      if (projectId === null || runId === null || taskId === null || approvedBuildJson === null) {
        return failure('invalid-payload', 'Некорректные параметры применения Build.')
      }
      return accepted(client.send({ applyApprovedBuild: { projectId, runId, taskId, approvedBuildJson: Buffer.from(approvedBuildJson, 'utf8') } }))
    }

    case 'core.terminalExecute': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const workspacePath = asBoundedString(value['workspacePath'])
      const program = asBoundedString(value['program'])
      const args = asArguments(value['args'])
      const cwd = asOptionalBoundedString(value['cwd'])
      const approvalId = asOptionalBoundedString(value['approvalId'])
      const timeoutMs = value['timeoutMs'] === undefined ? 30_000 : asBoundedNumber(value['timeoutMs'], 30_000)
      if (taskId === null || workspacePath === null || program === null || args === null || cwd === null || approvalId === null || timeoutMs === null) {
        return failure('invalid-payload', 'Некорректные параметры Terminal.')
      }
      return accepted(client.send({ terminalExecute: { taskId, workspacePath, program, args, cwd, timeoutMs, approvalId } }))
    }

    case 'update.getStatus':
      return { ok: true, value: updates.status }

    case 'update.check':
      return updates.check().then((value) => ({ ok: true, value }))

    case 'update.prepare':
      return updates.prepare().then((value) => ({ ok: true, value }))

    case 'update.restart':
      return { ok: true, value: { accepted: updates.restart() } }

    case 'update.skip':
      return { ok: true, value: updates.skip() }
  }
}

function accepted(result: 'queued' | 'queue-full'): unknown {
  return result === 'queued'
    ? { ok: true, value: { accepted: true } }
    : failure('queue-full', 'Очередь команд переполнена, повтори позже.')
}

function failure(code: CommandFailure['code'], message: string): CommandFailure {
  return { ok: false, code, message }
}

function isRendererCommand(value: string): value is RendererCommand {
  return (RENDERER_COMMANDS as readonly string[]).includes(value)
}

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {}
}

function asBoundedString(value: unknown): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > MAX_TEXT_FIELD_CHARS) {
    return null
  }
  return value
}

function asOptionalBoundedString(value: unknown): string | null {
  if (value === undefined || value === '') return ''
  return asBoundedString(value)
}

function asBoundedNumber(value: unknown, maximum: number): number | null {
  if (value === undefined) return maximum
  return typeof value === 'number' && Number.isInteger(value) && value > 0 && value <= maximum ? value : null
}

function asRelativePath(value: unknown): string | null {
  if (typeof value !== 'string' || value.length === 0 || value.length > MAX_TEXT_FIELD_CHARS) return null
  if (value.includes('\\') && value.split('\\').includes('..')) return null
  if (value.includes('/') && value.split('/').includes('..')) return null
  return value
}

function asPermissionMode(value: unknown): 'ask' | 'read_only' | 'full' | null {
  return value === 'ask' || value === 'read_only' || value === 'full' ? value : null
}

function asModelCatalogMode(value: unknown): 'free' | 'paid' | null {
  return value === 'free' || value === 'paid' ? value : null
}

function asProviderKind(value: unknown): ProviderKind | null {
  return typeof value === 'string' && (PROVIDER_KINDS as readonly string[]).includes(value)
    ? (value as ProviderKind)
    : null
}

function asArguments(value: unknown): string[] | null {
  if (!Array.isArray(value) || value.length > 64) return null
  const args = value.map((item) => asBoundedString(item))
  return args.every((item): item is string => item !== null) ? args : null
}

function asBoundedPayload(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 && value.length <= 256 * 1024 ? value : null
}
