import { BrowserWindow, clipboard, dialog, ipcMain, shell } from 'electron'
import { readFile, stat, writeFile } from 'node:fs/promises'
import { randomUUID } from 'node:crypto'
import { basename, dirname, extname, isAbsolute, join, win32 } from 'node:path'

import {
  PROVIDER_KINDS,
  RENDERER_COMMANDS,
  type AmbientHotkeyStatus,
  type CommandFailure,
  type PermissionMode,
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
import type { CodexService } from './codex-service'
import type { RepairService } from './repair-service'
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
import type { ListenerRuntimeService } from './update/listener-runtime'
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
const MAX_TRACE_EXPORT_BYTES = 16 * 1024 * 1024
const MAX_REVIEW_PLAN_BYTES = 512 * 1024

function applyWorkspacePermissionMode(client: CorePipeClient, mode: PermissionMode | undefined): void {
  if (mode === undefined) return
  client.send({ permissionMode: { mode } })
}

export interface ShellBridgeOptions {
  readonly client: CorePipeClient
  readonly workspaces: WorkspaceService
  readonly providers: ProviderStore
  readonly codex: CodexService
  readonly repair: RepairService
  readonly chats: ChatStore
  /**
   * Relaunches Core so it picks up the stored credentials: the model gateway
   * is built from the environment at Core startup and has no live setter.
   */
  readonly restartCore: () => Promise<boolean>
  /** Owns the source update; the renderer may only observe and trigger it. */
  readonly updates: UpdateService
  /** Owns the speech runtime download; the renderer only observes and asks. */
  readonly listenerRuntime: ListenerRuntimeService
  /**
   * Доступен ли глобальный хоткей паузы. Знает только main: комбинацию мог
   * занять другой процесс, и тогда третья точка входа честно объявляется
   * недоступной, а не изображается работающей.
   */
  readonly ambientHotkey: () => AmbientHotkeyStatus
  readonly exportDiagnostics: () => Promise<{ cancelled: boolean; path: string }>
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
  const { client, workspaces, providers, codex, repair, chats, restartCore, updates, listenerRuntime, ambientHotkey, exportDiagnostics, log } =
    options
  switch (command) {
    case 'shell.getState':
      return { ok: true, value: client.state }

    case 'shell.requestResync':
      return accepted(client.requestResync(true))

    case 'shell.exportDiagnostics':
      return exportDiagnostics()
        .then((value) => ({ ok: true, value }))
        .catch(() => failure('protocol-error', 'Не удалось сохранить диагностический bundle.'))

    case 'trace.export': {
      const content = asTraceContent(asRecord(payload)['content'])
      if (content === null) return failure('invalid-payload', 'Трейс пуст или слишком большой для экспорта.')
      const window = BrowserWindow.getFocusedWindow()
      const saveOptions: Electron.SaveDialogOptions = {
        defaultPath: 'evohime-trace.md',
        filters: [{ name: 'Markdown', extensions: ['md'] }]
      }
      const save = window ? dialog.showSaveDialog(window, saveOptions) : dialog.showSaveDialog(saveOptions)
      return save.then(async (selected) => {
        if (selected.canceled || !selected.filePath) return { ok: true, value: { cancelled: true, path: '' } }
        try {
          await writeFile(selected.filePath, content, 'utf8')
          return { ok: true, value: { cancelled: false, path: selected.filePath } }
        } catch {
          return failure('protocol-error', 'Не удалось сохранить Markdown-файл трейса.')
        }
      })
    }

    case 'workspace.list':
      return { ok: true, value: workspaces.list() }

    case 'workspace.pick':
      // The native folder dialog lives in the main process; the renderer only
      // ever receives the resulting path.
      return workspaces.pick().then((value) => {
        applyWorkspacePermissionMode(client, value.selection.permissionMode)
        return { ok: true, value }
      })

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
      applyWorkspacePermissionMode(client, selection.permissionMode)
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
      const taskId = asGoalToken(value['taskId'])
      const prompt = asBoundedString(value['prompt'])
      const workspacePath = asBoundedString(value['workspacePath'])
      const preferredRouteHint = value['preferredRouteHint'] === undefined || value['preferredRouteHint'] === null
        ? null
        : value['preferredRouteHint'] === 'local' || value['preferredRouteHint'] === 'cloud' || value['preferredRouteHint'] === 'codex_cli' ? value['preferredRouteHint'] : undefined
      const executionKind = value['executionKind'] === undefined ? 'dialogue'
        : value['executionKind'] === 'dialogue' || value['executionKind'] === 'coding' ? value['executionKind'] : undefined
      if (taskId === null || prompt === null || workspacePath === null || preferredRouteHint === undefined || executionKind === undefined) {
        return failure('invalid-payload', 'Некорректные параметры задачи.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ startTask: { taskId, prompt, workspacePath, preferredRouteHint: preferredRouteHint ?? '', executionKind } }))
    }

    case 'core.getTaskCheckpoint': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const workspacePath = asBoundedString(value['workspacePath'])
      const maxReplayEvents = value['maxReplayEvents'] === undefined
        ? 0
        : asBoundedNumber(value['maxReplayEvents'], 256)
      if (taskId === null || workspacePath === null || maxReplayEvents === null) {
        return failure('invalid-payload', 'Некорректные параметры checkpoint.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ getTaskCheckpoint: { taskId, workspacePath, maxReplayEvents } }))
    }

    case 'core.resolveTaskCheckpoint': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const workspacePath = asBoundedString(value['workspacePath'])
      const checkpointId = asBoundedString(value['checkpointId'])
      const expectedSourceEventSeq = asNonNegativeInteger(value['expectedSourceEventSeq'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const action = value['action'] === 'acknowledge_recovery' || value['action'] === 'request_resume'
        ? value['action']
        : null
      if (taskId === null || workspacePath === null || checkpointId === null || expectedSourceEventSeq === null || idempotencyKey === null || action === null) {
        return failure('invalid-payload', 'Некорректное действие checkpoint.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ resolveTaskCheckpoint: { taskId, workspacePath, checkpointId, expectedSourceEventSeq, action, idempotencyKey } }))
    }

    case 'core.createAnalysisKernel': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const workspaceId = asBoundedString(value['workspaceId'])
      const runtimeVersion = asBoundedString(value['runtimeVersion'])
      const packageManifestHash = asBoundedString(value['packageManifestHash'])
      const policyHash = asBoundedString(value['policyHash'])
      const limitsJson = value['limitsJson'] === undefined ? '' : asBoundedString(value['limitsJson'])
      if (taskId === null || workspaceId === null || runtimeVersion === null || packageManifestHash === null || policyHash === null || limitsJson === null) {
        return failure('invalid-payload', 'Некорректные параметры analysis kernel.')
      }
      return accepted(client.send({ createAnalysisKernel: { taskId, workspaceId, runtimeVersion, packageManifestHash, policyHash, limitsJson: Buffer.from(limitsJson, 'utf8') } }))
    }

    case 'core.getAnalysisKernel': {
      const value = asRecord(payload)
      const kernelId = asBoundedString(value['kernelId'])
      const maxObjects = value['maxObjects'] === undefined ? 0 : asBoundedNumber(value['maxObjects'], 1024)
      if (kernelId === null || maxObjects === null) return failure('invalid-payload', 'Некорректный идентификатор kernel.')
      return accepted(client.send({ getAnalysisKernel: { kernelId, maxObjects } }))
    }

    case 'core.executeAnalysisKernel': {
      const value = asRecord(payload)
      const kernelId = asBoundedString(value['kernelId'])
      const requestId = asBoundedString(value['requestId'])
      const operation = asBoundedString(value['operation'])
      const args = asBoundedString(value['args'])
      const requestedCapability = value['requestedCapability'] === undefined ? '' : asBoundedString(value['requestedCapability'])
      const correlationId = asBoundedString(value['correlationId'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const contextRefs = Array.isArray(value['contextRefs']) && value['contextRefs'].every((ref): ref is string => typeof ref === 'string' && ref.length <= 256) ? value['contextRefs'] : []
      if (kernelId === null || requestId === null || operation === null || args === null || requestedCapability === null || correlationId === null || idempotencyKey === null || contextRefs.length > 32) {
        return failure('invalid-payload', 'Некорректный запрос analysis kernel.')
      }
      return accepted(client.send({ executeAnalysisKernel: { kernelId, requestId, operation, args: Buffer.from(args, 'utf8'), requestedCapability, contextRefs, correlationId, idempotencyKey } }))
    }

    case 'core.resetAnalysisKernel': {
      const value = asRecord(payload)
      const kernelId = asBoundedString(value['kernelId'])
      const expectedRevision = asNonNegativeInteger(value['expectedRevision'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (kernelId === null || expectedRevision === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректный reset analysis kernel.')
      return accepted(client.send({ resetAnalysisKernel: { kernelId, expectedRevision, idempotencyKey } }))
    }

    case 'core.listRefinementCandidates': {
      const value = asRecord(payload)
      const ownerScope = asBoundedString(value['ownerScope'])
      const limit = value['limit'] === undefined ? 0 : asBoundedNumber(value['limit'], 128)
      if (ownerScope === null || limit === null) return failure('invalid-payload', 'Некорректный scope refinement.')
      return accepted(client.send({ listRefinementCandidates: { ownerScope, limit } }))
    }

    case 'core.getRefinementCandidate': {
      const value = asRecord(payload)
      const candidateId = asBoundedString(value['candidateId'])
      const revision = asNonNegativeInteger(value['revision'])
      if (candidateId === null || revision === null) return failure('invalid-payload', 'Некорректный candidate refinement.')
      return accepted(client.send({ getRefinementCandidate: { candidateId, revision } }))
    }

    case 'core.refinementAction': {
      const value = asRecord(payload)
      const candidateId = asBoundedString(value['candidateId'])
      const revision = asNonNegativeInteger(value['revision'])
      const expectedVersion = asNonNegativeInteger(value['expectedVersion'])
      const action = ['approve', 'reject', 'activate', 'rollback'].includes(String(value['action'])) ? String(value['action']) : null
      const approvalToken = value['approvalToken'] === undefined ? '' : asBoundedString(value['approvalToken'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (candidateId === null || revision === null || expectedVersion === null || action === null || approvalToken === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректное действие refinement.')
      return accepted(client.send({ refinementAction: { candidateId, revision, expectedVersion, action, approvalToken, idempotencyKey } }))
    }

    case 'core.createGoal': {
      const value = asRecord(payload)
      const goalId = asGoalToken(value['goalId'])
      const workspacePath = asBoundedString(value['workspacePath'])
      const chatId = value['chatId'] === undefined || value['chatId'] === null || value['chatId'] === ''
        ? ''
        : asGoalToken(value['chatId'])
      const objective = asBoundedString(value['objective'])
      const successCriteria = asGoalCriteria(value['successCriteria'])
      const tokenBudget = asOptionalGoalBudget(value['tokenBudget'])
      const costBudgetMicros = asOptionalGoalBudget(value['costBudgetMicros'])
      const continuationBudget = asOptionalGoalBudget(value['continuationBudget'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (goalId === null || workspacePath === null || chatId === null || objective === null || successCriteria === null || tokenBudget === null || costBudgetMicros === null || continuationBudget === null || idempotencyKey === null) {
        return failure('invalid-payload', 'Некорректные параметры цели.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ createGoal: { goalId, workspacePath, chatId, objective, successCriteria: [...successCriteria], tokenBudget, costBudgetMicros, continuationBudget, idempotencyKey } }))
    }

    case 'core.getGoal': {
      const goalId = asGoalToken(asRecord(payload)['goalId'])
      if (goalId === null) return failure('invalid-payload', 'Некорректный идентификатор цели.')
      return accepted(client.send({ getGoal: { goalId } }))
    }

    case 'core.listGoals': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const limit = value['limit'] === undefined ? 0 : asOptionalLimit(value['limit'], 128)
      if (workspacePath === null || limit === null) return failure('invalid-payload', 'Некорректный список целей.')
      return accepted(client.send({ listGoals: { workspacePath, limit } }))
    }

    case 'core.pauseGoal':
    case 'core.resumeGoal':
    case 'core.cancelGoal': {
      const value = asRecord(payload)
      const goalId = asGoalToken(value['goalId'])
      const expectedVersion = asNonNegativeInteger(value['expectedVersion'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (goalId === null || expectedVersion === null || expectedVersion === 0 || idempotencyKey === null) return failure('invalid-payload', 'Некорректное действие цели.')
      const wireCommand = command === 'core.pauseGoal' ? 'pauseGoal' : command === 'core.resumeGoal' ? 'resumeGoal' : 'cancelGoal'
      if (wireCommand === 'pauseGoal') return accepted(client.send({ pauseGoal: { goalId, expectedVersion, idempotencyKey } }))
      if (wireCommand === 'resumeGoal') return accepted(client.send({ resumeGoal: { goalId, expectedVersion, idempotencyKey } }))
      return accepted(client.send({ cancelGoal: { goalId, expectedVersion, idempotencyKey } }))
    }

    case 'core.updateGoal': {
      const value = asRecord(payload)
      const goalId = asGoalToken(value['goalId'])
      const expectedVersion = asNonNegativeInteger(value['expectedVersion'])
      const objective = value['objective'] === undefined ? '' : asBoundedString(value['objective'])
      const successCriteria = value['successCriteria'] === undefined ? [] : asGoalCriteria(value['successCriteria'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (goalId === null || expectedVersion === null || expectedVersion === 0 || objective === null || successCriteria === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректное обновление цели.')
      return accepted(client.send({ updateGoal: { goalId, expectedVersion, objective, successCriteria: [...successCriteria], idempotencyKey } }))
    }

    case 'core.verifyGoalCriterion': {
      const value = asRecord(payload)
      const goalId = asGoalToken(value['goalId'])
      const expectedVersion = asNonNegativeInteger(value['expectedVersion'])
      const criterionId = asGoalToken(value['criterionId'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (goalId === null || expectedVersion === null || expectedVersion === 0 || criterionId === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректное подтверждение критерия.')
      return accepted(client.send({ verifyGoalCriterion: { goalId, expectedVersion, criterionId, idempotencyKey } }))
    }

    case 'core.linkGoalReference': {
      const value = asRecord(payload)
      const goalId = asGoalToken(value['goalId'])
      const expectedVersion = asNonNegativeInteger(value['expectedVersion'])
      const kind = asGoalReferenceKind(value['kind'])
      const referenceId = asGoalToken(value['referenceId'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (goalId === null || expectedVersion === null || expectedVersion === 0 || kind === null || referenceId === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректная связь цели.')
      return accepted(client.send({ linkGoalReference: { goalId, expectedVersion, kind, referenceId, idempotencyKey } }))
    }

    case 'core.saveContinuationPolicy': {
      const value = asRecord(payload)
      const policyJson = asBoundedString(value['policyJson'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const actor = asBoundedString(value['actor'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (policyJson === null || ownerScope === null || actor === null || idempotencyKey === null || policyJson.length > 65536) {
        return failure('invalid-payload', 'Некорректная Continuation Policy.')
      }
      return accepted(client.send({ saveContinuationPolicy: { policyJson: Buffer.from(policyJson, 'utf8'), ownerScope, actor, idempotencyKey } }))
    }

    case 'core.startContinuationRun': {
      const value = asRecord(payload)
      const runId = asGoalToken(value['runId'])
      const policyId = asGoalToken(value['policyId'])
      const policyRevision = asNonNegativeInteger(value['policyRevision'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const taskId = asBoundedString(value['taskId'])
      const goalId = value['goalId'] === undefined ? '' : asGoalToken(value['goalId'])
      const goalVersion = value['goalVersion'] === undefined ? 0 : asNonNegativeInteger(value['goalVersion'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (runId === null || policyId === null || policyRevision === null || policyRevision === 0 || ownerScope === null || taskId === null || goalId === null || goalVersion === null || idempotencyKey === null) {
        return failure('invalid-payload', 'Некорректный запуск continuation.')
      }
      return accepted(client.send({ startContinuationRun: { runId, policyId, policyRevision, ownerScope, taskId, goalId, goalVersion, idempotencyKey } }))
    }

    case 'core.getContinuationRun': {
      const runId = asGoalToken(asRecord(payload)['runId'])
      if (runId === null) return failure('invalid-payload', 'Некорректный continuation run.')
      return accepted(client.send({ getContinuationRun: { runId } }))
    }

    case 'core.stopContinuation': {
      const value = asRecord(payload)
      const runId = asGoalToken(value['runId'])
      const expectedState = value['expectedState'] === undefined ? 'running' : asBoundedString(value['expectedState'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (runId === null || expectedState !== 'running' || idempotencyKey === null) return failure('invalid-payload', 'Некорректная остановка continuation.')
      return accepted(client.send({ stopContinuation: { runId, expectedState, idempotencyKey } }))
    }

    case 'core.pauseContinuation': {
      const value = asRecord(payload)
      const runId = asGoalToken(value['runId'])
      const expectedState = value['expectedState'] === undefined ? 'running' : value['expectedState']
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (runId === null || expectedState !== 'running' || idempotencyKey === null) return failure('invalid-payload', 'Некорректная пауза continuation.')
      return accepted(client.send({ pauseContinuation: { runId, expectedState, idempotencyKey } }))
    }

    case 'core.resumeContinuation': {
      const value = asRecord(payload)
      const runId = asGoalToken(value['runId'])
      const expectedState = value['expectedState'] === undefined ? 'paused' : value['expectedState']
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      if (runId === null || expectedState !== 'paused' || idempotencyKey === null) return failure('invalid-payload', 'Некорректное возобновление continuation.')
      return accepted(client.send({ resumeContinuation: { runId, expectedState, idempotencyKey } }))
    }


    case 'core.listRetainedChildren': {
      const value = asRecord(payload)
      const limit = value['limit'] === undefined ? 0 : asBoundedNumber(value['limit'], 100)
      if (limit === null) return failure('invalid-payload', 'Некорректный лимит retained children.')
      return accepted(client.send({ listRetainedChildren: { parentId: '', limit } }))
    }

    case 'core.getRetainedChild': {
      const childId = asGoalToken(asRecord(payload)['childId'])
      if (childId === null) return failure('invalid-payload', 'Некорректный child id.')
      return accepted(client.send({ getRetainedChild: { parentId: '', childId } }))
    }

    case 'core.retainChild': {
      const value = asRecord(payload)
      const childId = asGoalToken(value['childId'])
      const familyRootId = value['familyRootId'] === undefined ? '' : asGoalToken(value['familyRootId'])
      const role = asBoundedString(value['role'])
      const stableName = value['stableName'] === undefined ? '' : asBoundedString(value['stableName'])
      const revision = value['revision'] === undefined ? 0 : asBoundedNumber(value['revision'], Number.MAX_SAFE_INTEGER)
      const grantSnapshotHash = asBoundedString(value['grantSnapshotHash'])
      const contextScopeHash = asBoundedString(value['contextScopeHash'])
      const workspaceStateRef = value['workspaceStateRef'] === undefined ? '' : asBoundedString(value['workspaceStateRef'])
      const lastReportRef = value['lastReportRef'] === undefined ? '' : asBoundedString(value['lastReportRef'])
      const retainedUntilMs = value['retainedUntilMs'] === undefined ? 0 : asBoundedNumber(value['retainedUntilMs'], Number.MAX_SAFE_INTEGER)
      const createdAtMs = value['createdAtMs'] === undefined ? 0 : asBoundedNumber(value['createdAtMs'], Number.MAX_SAFE_INTEGER)
      const lastActiveAtMs = value['lastActiveAtMs'] === undefined ? 0 : asBoundedNumber(value['lastActiveAtMs'], Number.MAX_SAFE_INTEGER)
      const expectedRegistryVersion = value['expectedRegistryVersion'] === undefined ? 0 : asBoundedNumber(value['expectedRegistryVersion'], Number.MAX_SAFE_INTEGER)
      if (childId === null || familyRootId === null || role === null || stableName === null || revision === null || grantSnapshotHash === null || contextScopeHash === null || workspaceStateRef === null || lastReportRef === null || retainedUntilMs === null || createdAtMs === null || lastActiveAtMs === null || expectedRegistryVersion === null) return failure('invalid-payload', 'Некорректный retained child.')
      return accepted(client.send({ retainChild: { parentId: '', childId, familyRootId, role, stableName, revision, grantSnapshotHash, contextScopeHash, workspaceStateRef, lastReportRef, retainedUntilMs, createdAtMs, lastActiveAtMs, expectedRegistryVersion } }))
    }

    case 'core.sendChildFollowUp': {
      const value = asRecord(payload)
      const childId = asGoalToken(value['childId'])
      const idempotencyKey = asGoalToken(value['idempotencyKey'])
      const expectedChildRevision = asBoundedNumber(value['expectedChildRevision'], Number.MAX_SAFE_INTEGER)
      const instruction = asBoundedString(value['instruction'])
      const contextRefs = value['contextRefs'] === undefined ? [] : asRetainedStringArray(value['contextRefs'], 32, 512)
      const requestedGrants = value['requestedGrants'] === undefined ? [] : asRetainedStringArray(value['requestedGrants'], 16, 256)
      const budgetJson = value['budgetJson'] === undefined ? '' : asBoundedString(value['budgetJson'])
      const mode = value['mode'] === undefined ? 'follow_up' : value['mode']
      const correlationId = asGoalToken(value['correlationId'])
      if (childId === null || idempotencyKey === null || expectedChildRevision === null || instruction === null || contextRefs === null || requestedGrants === null || budgetJson === null || !['follow_up', 'steer', 'auto'].includes(String(mode)) || correlationId === null) return failure('invalid-payload', 'Некорректный follow-up.')
      return accepted(client.send({ sendChildFollowUp: { parentId: '', childId, idempotencyKey, expectedChildRevision, instruction, contextRefs, requestedGrants, budgetJson, mode: String(mode), correlationId } }))
    }

    case 'core.deleteRetainedChild': {
      const value = asRecord(payload)
      const childId = asGoalToken(value['childId'])
      const expectedRegistryVersion = value['expectedRegistryVersion'] === undefined ? 0 : asBoundedNumber(value['expectedRegistryVersion'], Number.MAX_SAFE_INTEGER)
      if (childId === null || expectedRegistryVersion === null) return failure('invalid-payload', 'Некорректное удаление retained child.')
      return accepted(client.send({ deleteRetainedChild: { parentId: '', childId, expectedRegistryVersion } }))
    }

    case 'core.listSkills': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const limit = value['limit'] === undefined ? 0 : asBoundedNumber(value['limit'], 128)
      if (workspacePath === null || limit === null) {
        return failure('invalid-payload', 'Некорректные параметры каталога skills.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ listSkills: { workspacePath, limit } }))
    }

    case 'core.loadSkill': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const skillId = asBoundedString(value['skillId'])
      const maxBytes = value['maxBytes'] === undefined ? 0 : asBoundedNumber(value['maxBytes'], 256 * 1024)
      if (workspacePath === null || skillId === null || !isSafeSkillId(skillId) || maxBytes === null) {
        return failure('invalid-payload', 'Некорректные параметры загрузки skill.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ loadSkill: { workspacePath, skillId, maxBytes } }))
    }

    case 'core.loadSkillReference': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const skillId = asBoundedString(value['skillId'])
      const reference = asBoundedString(value['reference'])
      const maxBytes = value['maxBytes'] === undefined ? 0 : asBoundedNumber(value['maxBytes'], 64 * 1024)
      if (workspacePath === null || skillId === null || !isSafeSkillId(skillId) || reference === null || !isSafeSkillReference(reference) || maxBytes === null) {
        return failure('invalid-payload', 'Некорректные параметры reference skill.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ loadSkillReference: { workspacePath, skillId, reference, maxBytes } }))
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
      const idempotencyKey = asOptionalBoundedString(value['idempotencyKey'])
      const rejectionReason = asOptionalBoundedString(value['rejectionReason'])
      const cancel = value['cancel']
      if (approvalId === null || typeof granted !== 'boolean' || (cancel !== undefined && typeof cancel !== 'boolean')) {
        return failure('invalid-payload', 'Некорректное решение по approval.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ resolveApproval: { approvalId, granted, idempotencyKey, rejectionReason, cancel: cancel ?? null } }))
    }

    case 'core.resolveRoutingDecision': {
      const value = asRecord(payload)
      const traceId = asBoundedString(value['traceId'])
      const approve = value['approve']
      if (traceId === null || typeof approve !== 'boolean') {
        return failure('invalid-payload', 'Некорректное решение по маршруту.')
      }
      log('info', 'shell.routing_decision_forwarded', { command })
      return accepted(client.send({ resolveRoutingDecision: { traceId, approve } }))
    }

    // План 01.5: read-only проекция состава контекста и Core-команды над
    // scratchpad. Renderer получает только bounded projection: ids, счётчики,
    // причины и hash, без сырого prompt, тела памяти и raw tool output.
    case 'core.getContextLedger': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const limit = asBoundedNumber(value['limit'], 100)
      if (taskId === null || limit === null) {
        return failure('invalid-payload', 'Некорректные параметры журнала контекста.')
      }
      return accepted(client.send({ getContextLedger: { taskId, limit } }))
    }

    case 'core.listTaskScratchpad': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const category = value['category'] === undefined ? '' : asBoundedString(value['category'])
      const status = value['status'] === undefined ? '' : asBoundedString(value['status'])
      const limit = asBoundedNumber(value['limit'], 100)
      if (taskId === null || category === null || status === null || limit === null) {
        return failure('invalid-payload', 'Некорректные параметры чтения заметок задачи.')
      }
      return accepted(client.send({ listTaskScratchpad: { taskId, category, status, limit } }))
    }

    case 'core.clearTaskScratchpad': {
      const taskId = asBoundedString(asRecord(payload)['taskId'])
      if (taskId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор задачи.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ clearTaskScratchpad: { taskId } }))
    }

    case 'core.summarizeContextNow': {
      const taskId = asBoundedString(asRecord(payload)['taskId'])
      if (taskId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор задачи.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ summarizeContextNow: { taskId } }))
    }

    case 'core.pinContextItem': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const itemId = asBoundedString(value['itemId'])
      const pinned = value['pinned']
      if (taskId === null || itemId === null || typeof pinned !== 'boolean') {
        return failure('invalid-payload', 'Некорректные параметры закрепления элемента.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ pinContextItem: { taskId, itemId, pinned } }))
    }

    case 'core.readContextArtifact': {
      const value = asRecord(payload)
      const taskId = asBoundedString(value['taskId'])
      const locator = asBoundedString(value['locator'])
      if (taskId === null || locator === null) {
        return failure('invalid-payload', 'Некорректные параметры чтения артефакта.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ readContextArtifact: { taskId, locator } }))
    }

    case 'core.indexWorkspace':
    case 'core.rebuildIndex': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const enableEmbeddings = value['enableEmbeddings'] ?? false
      if (workspacePath === null || typeof enableEmbeddings !== 'boolean') {
        return failure('invalid-payload', 'Некорректные параметры индекса workspace.')
      }
      log('info', 'shell.command_forwarded', { command })
      return command === 'core.rebuildIndex'
        ? accepted(client.send({ rebuildIndex: { workspacePath, enableEmbeddings } }))
        : accepted(client.send({ indexWorkspace: { workspacePath, enableEmbeddings } }))
    }

    case 'core.searchWorkspaceKnowledge': {
      const value = asRecord(payload)
      const workspacePath = asBoundedString(value['workspacePath'])
      const query = asBoundedString(value['query'])
      const pathFilter = value['pathFilter'] === undefined ? '' : asBoundedString(value['pathFilter'])
      const languageFilter = value['languageFilter'] === undefined ? '' : asBoundedString(value['languageFilter'])
      const hybrid = value['hybrid'] ?? false
      if (workspacePath === null || query === null || pathFilter === null || languageFilter === null || typeof hybrid !== 'boolean') {
        return failure('invalid-payload', 'Некорректные параметры поиска по workspace.')
      }
      return accepted(client.send({
        searchWorkspaceKnowledge: { workspacePath, query, pathFilter, languageFilter, hybrid },
      }))
    }

    case 'core.getIndexStatus': {
      const workspacePath = asBoundedString(asRecord(payload)['workspacePath'])
      if (workspacePath === null) {
        return failure('invalid-payload', 'Некорректный путь workspace.')
      }
      return accepted(client.send({ getIndexStatus: { workspacePath } }))
    }

    case 'core.cancelWorkspaceIndex': {
      const workspacePath = asBoundedString(asRecord(payload)['workspacePath'])
      if (workspacePath === null) {
        return failure('invalid-payload', 'Некорректный путь workspace.')
      }
      return accepted(client.send({ cancelWorkspaceIndex: { workspacePath } }))
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
      workspaces.setPermissionMode(mode)
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
      if (destinationPath.length === 0) {
        const window = BrowserWindow.getFocusedWindow()
        const options: Electron.SaveDialogOptions = {
          defaultPath: 'evohime-backup.evohime',
          filters: [{ name: 'EvoHime backup', extensions: ['evohime'] }]
        }
        const save = window ? dialog.showSaveDialog(window, options) : dialog.showSaveDialog(options)
        return save.then((selected) => selected.canceled || !selected.filePath
          ? { ok: true, value: { accepted: false } }
          : accepted(client.send({ createDatabaseBackup: { destinationPath: selected.filePath } })))
      }
      return accepted(client.send({ createDatabaseBackup: { destinationPath } }))
    }

    case 'core.prepareDatabaseRestore': {
      const value = asRecord(payload)
      const backupPath = asBoundedString(value['backupPath'])
      if (backupPath === null) {
        return failure('invalid-payload', 'Некорректные параметры проверки backup.')
      }
      if (backupPath.length === 0) {
        const window = BrowserWindow.getFocusedWindow()
        const options: Electron.OpenDialogOptions = {
          properties: ['openFile'],
          filters: [{ name: 'EvoHime backup', extensions: ['evohime'] }]
        }
        const open = window ? dialog.showOpenDialog(window, options) : dialog.showOpenDialog(options)
        return open.then((selected) => selected.canceled || selected.filePaths.length === 0
          ? { ok: true, value: { accepted: false } }
          : accepted(client.send({ prepareDatabaseRestore: { backupPath: selected.filePaths[0] as string } })))
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

    case 'core.getReceiptKeyStatus':
      return accepted(client.send({ getReceiptKeyStatus: {} }))

    case 'core.trustReceiptGenesis': {
      const value = asRecord(payload)
      const genesisKeyId = asBoundedString(value['genesisKeyId'])
      const approvalId = asBoundedString(value['approvalId']) ?? randomUUID()
      const source = value['source'] === undefined ? '' : asBoundedString(value['source'])
      if (genesisKeyId === null || approvalId === null || source === null) {
        return failure('invalid-payload', 'Некорректные параметры доверенного genesis.')
      }
      return accepted(client.send({ trustReceiptGenesis: { genesisKeyId, approvalId, source } }))
    }

    case 'core.rotateReceiptKey': {
      const value = asRecord(payload)
      const reason = value['reason'] === 'compromise' ? 'compromise' : value['reason'] === 'manual' ? 'manual' : null
      const approvalId = asBoundedString(value['approvalId']) ?? randomUUID()
      if (reason === null || approvalId === null) {
        return failure('invalid-payload', 'Некорректные параметры ротации ключа.')
      }
      return accepted(client.send({ rotateReceiptKey: { reason, approvalId } }))
    }

    case 'core.createNewReceiptGenesis': {
      const value = asRecord(payload)
      const approvalId = asBoundedString(value['approvalId']) ?? randomUUID()
      const source = value['source'] === undefined ? '' : asBoundedString(value['source'])
      if (approvalId === null || source === null) {
        return failure('invalid-payload', 'Некорректные параметры восстановления ключа.')
      }
      return accepted(client.send({ createNewReceiptGenesis: { approvalId, source } }))
    }

    case 'core.listReceipts': {
      const value = asRecord(payload)
      const taskId = asOptionalBoundedString(value['taskId'])
      const runId = asOptionalBoundedString(value['runId'])
      const actionId = asOptionalBoundedString(value['actionId'])
      const fromRfc3339 = asOptionalBoundedString(value['fromRfc3339'])
      const toRfc3339 = asOptionalBoundedString(value['toRfc3339'])
      const limit = asOptionalLimit(value['limit'], 500)
      if (taskId === null || runId === null || actionId === null || fromRfc3339 === null || toRfc3339 === null || limit === null) {
        return failure('invalid-payload', 'Некорректный фильтр списка receipts.')
      }
      return accepted(client.send({ listReceipts: { taskId, runId, actionId, fromRfc3339, toRfc3339, limit } }))
    }

    case 'core.verifyReceipts': {
      const value = asRecord(payload)
      const taskId = asOptionalBoundedString(value['taskId'])
      const runId = asOptionalBoundedString(value['runId'])
      const actionId = asOptionalBoundedString(value['actionId'])
      const fromRfc3339 = asOptionalBoundedString(value['fromRfc3339'])
      const toRfc3339 = asOptionalBoundedString(value['toRfc3339'])
      const trustKeyId = asOptionalBoundedString(value['trustKeyId'])
      const limit = asOptionalLimit(value['limit'], 2000)
      if (taskId === null || runId === null || actionId === null || fromRfc3339 === null || toRfc3339 === null || trustKeyId === null || limit === null) {
        return failure('invalid-payload', 'Некорректный фильтр проверки receipts.')
      }
      return accepted(client.send({ verifyReceipts: { taskId, runId, actionId, fromRfc3339, toRfc3339, limit, trustKeyId } }))
    }

    case 'core.exportReceipts': {
      const value = asRecord(payload)
      const destinationPath = asBoundedString(value['destinationPath'])
      const taskId = asOptionalBoundedString(value['taskId'])
      const runId = asOptionalBoundedString(value['runId'])
      const actionId = asOptionalBoundedString(value['actionId'])
      const fromRfc3339 = asOptionalBoundedString(value['fromRfc3339'])
      const toRfc3339 = asOptionalBoundedString(value['toRfc3339'])
      const limit = asOptionalLimit(value['limit'], 100_000)
      if (destinationPath === null || taskId === null || runId === null || actionId === null || fromRfc3339 === null || toRfc3339 === null || limit === null) {
        return failure('invalid-payload', 'Некорректные параметры экспорта receipts.')
      }
      return accepted(client.send({ exportReceipts: { destinationPath, taskId, runId, actionId, fromRfc3339, toRfc3339, limit, replace: false } }))
    }

    case 'core.listMemoryPending':
    case 'core.getMemoryConflicts': {
      const value = asRecord(payload)
      const scopeKind = asMemoryScopeKind(value['scopeKind'])
      const projectId = asBoundedString(value['projectId'])
      const secondaryId = asOptionalBoundedString(value['secondaryId'])
      const limit = asBoundedNumber(value['limit'], 100)
      const workspacePath = asOptionalBoundedString(value['workspacePath'])
      if (
        scopeKind === null ||
        projectId === null ||
        secondaryId === null ||
        limit === null ||
        workspacePath === null
      ) {
        return failure('invalid-payload', 'Некорректные параметры очереди памяти.')
      }
      const request = { scopeKind, projectId, secondaryId, limit, workspacePath }
      return accepted(
        client.send(
          command === 'core.listMemoryPending'
            ? { listMemoryPending: request }
            : { getMemoryConflicts: request }
        )
      )
    }

    case 'core.getMemory': {
      const id = asBoundedString(asRecord(payload)['id'])
      if (id === null) return failure('invalid-payload', 'Некорректный идентификатор памяти.')
      return accepted(client.send({ getMemory: { id } }))
    }

    case 'core.confirmMemory':
    case 'core.rejectMemory': {
      const value = asRecord(payload)
      const ids = asMemoryIds(value['ids'])
      const approvalId = asBoundedString(value['approvalId'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (ids === null || approvalId === null || idempotencyKey === null) {
        return failure('invalid-payload', 'Некорректное решение по памяти.')
      }
      log('info', 'shell.command_forwarded', { command })
      const request = { ids: [...ids], approvalId, idempotencyKey }
      return accepted(
        client.send(
          command === 'core.confirmMemory' ? { confirmMemory: request } : { rejectMemory: request }
        )
      )
    }

    case 'core.supersedeMemory': {
      const value = asRecord(payload)
      const oldId = asBoundedString(value['oldId'])
      const newId = asBoundedString(value['newId'])
      const reason = asSupersessionReason(value['reason'])
      const approvalId = asBoundedString(value['approvalId'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (oldId === null || newId === null || reason === null || approvalId === null || idempotencyKey === null) {
        return failure('invalid-payload', 'Некорректное разрешение конфликта памяти.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(client.send({ supersedeMemory: { oldId, newId, reason, approvalId, idempotencyKey } }))
    }

    case 'core.reviseMemoryCandidate': {
      const value = asRecord(payload)
      const id = asBoundedString(value['id'])
      // An empty statement is allowed: a session-only note keeps the text Core
      // already holds instead of asking the user to retype it.
      const statement = asOptionalBoundedString(value['statement'])
      const sessionOnly = value['sessionOnly']
      const sessionId = asOptionalBoundedString(value['sessionId'])
      const approvalId = asBoundedString(value['approvalId'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (
        id === null ||
        statement === null ||
        typeof sessionOnly !== 'boolean' ||
        sessionId === null ||
        approvalId === null ||
        idempotencyKey === null
      ) {
        return failure('invalid-payload', 'Некорректная правка кандидата в память.')
      }
      log('info', 'shell.command_forwarded', { command })
      return accepted(
        client.send({
          reviseMemoryCandidate: { id, statement, sessionOnly, sessionId, approvalId, idempotencyKey }
        })
      )
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

    case 'review.pickPlan':
      return pickReviewPlan(asRecord(payload)['directory'], workspaces.list().selected)

    case 'review.start': {
      const value = asRecord(payload)
      const reviewId = asBoundedString(value['reviewId'])
      const fileName = asBoundedString(value['fileName'])
      const fileNames = asReviewFileNames(value['fileNames'])
      const sourceMarkdown = asReviewMarkdown(value['sourceMarkdown'])
      const reviewerModels = asReviewModels(value['reviewerModels'])
      const synthesisModel = asBoundedString(value['synthesisModel'])
      // Путей может не быть вовсе: план могли перетащить из источника без
      // файловой системы. Тогда ядро читает только сам план.
      const sourcePaths = asReviewSourcePaths(value['sourcePaths'])
      if (reviewId === null || fileName === null || fileNames === null || sourceMarkdown === null || reviewerModels === null || synthesisModel === null || sourcePaths === null) {
        return failure('invalid-payload', 'Некорректные параметры ревью плана.')
      }
      return accepted(client.send({
        startPlanReview: { reviewId, fileName, fileNames: [...fileNames], sourceMarkdown, reviewerModels: [...reviewerModels], synthesisModel, sourcePaths: [...sourcePaths] }
      }))
    }

    case 'review.stop': {
      const reviewId = asBoundedString(asRecord(payload)['reviewId'])
      if (reviewId === null) return failure('invalid-payload', 'Некорректный идентификатор ревью.')
      return accepted(client.send({ stopPlanReview: { reviewId } }))
    }

    case 'review.list': {
      const limit = asBoundedNumber(asRecord(payload)['limit'], 50)
      if (limit === null) return failure('invalid-payload', 'Некорректный лимит истории ревью.')
      return accepted(client.send({ listPlanReviews: { limit } }))
    }

    case 'review.get': {
      const reviewId = asBoundedString(asRecord(payload)['reviewId'])
      if (reviewId === null) return failure('invalid-payload', 'Некорректный идентификатор ревью.')
      return accepted(client.send({ getPlanReview: { reviewId } }))
    }

    case 'review.clearHistory':
      return accepted(client.send({ clearPlanReviewHistory: {} }))

    case 'review.export': {
      const value = asRecord(payload)
      const reviewId = asBoundedString(value['reviewId'])
      const destinationPath = asReviewDestinationPath(value['destinationPath'])
      const includeReviewers = value['includeReviewers'] === undefined ? false : value['includeReviewers']
      if (reviewId === null || destinationPath === null || typeof includeReviewers !== 'boolean') {
        return failure('invalid-payload', 'Некорректные параметры экспорта ревью.')
      }
      if (destinationPath.length === 0) {
        const window = BrowserWindow.getFocusedWindow()
        const saveOptions: Electron.SaveDialogOptions = {
          defaultPath: 'review-final.md',
          filters: [{ name: 'Markdown', extensions: ['md'] }]
        }
        const save = window ? dialog.showSaveDialog(window, saveOptions) : dialog.showSaveDialog(saveOptions)
        return save.then((selected) => selected.canceled || !selected.filePath
          ? { ok: true, value: { cancelled: true } }
          : accepted(client.send({ exportPlanReview: { reviewId, destinationPath: selected.filePath, includeReviewers } })))
      }
      return accepted(client.send({ exportPlanReview: { reviewId, destinationPath, includeReviewers } }))
    }

    case 'review.revise': {
      const value = asRecord(payload)
      const revisionId = asBoundedString(value['revisionId'])
      const reviewId = asBoundedString(value['reviewId'])
      const fileName = asBoundedString(value['fileName'])
      const sourceMarkdown = asReviewMarkdown(value['sourceMarkdown'])
      const model = asBoundedString(value['model'])
      // Путь может быть пустым — план мог прийти перетаскиванием из источника
      // без файловой системы. Тогда ядро правит без соседних планов.
      const sourcePath = asOptionalBoundedString(value['sourcePath'])
      if (revisionId === null || reviewId === null || fileName === null || sourceMarkdown === null || model === null || sourcePath === null) {
        return failure('invalid-payload', 'Некорректные параметры правки плана.')
      }
      return accepted(client.send({ revisePlan: { revisionId, reviewId, fileName, sourceMarkdown, model, sourcePath } }))
    }

    case 'review.stopRevision': {
      const revisionId = asBoundedString(asRecord(payload)['revisionId'])
      if (revisionId === null) return failure('invalid-payload', 'Некорректный идентификатор правки.')
      return accepted(client.send({ stopRevision: { revisionId } }))
    }

    case 'review.saveRevision': {
      const value = asRecord(payload)
      const revisionId = asBoundedString(value['revisionId'])
      const destinationPath = asReviewDestinationPath(value['destinationPath'])
      const fileName = value['fileName'] === undefined ? '' : asBoundedString(value['fileName'])
      if (revisionId === null || destinationPath === null || fileName === null) {
        return failure('invalid-payload', 'Некорректные параметры сохранения плана.')
      }
      if (destinationPath.length === 0) {
        const window = BrowserWindow.getFocusedWindow()
        const saveOptions: Electron.SaveDialogOptions = {
          defaultPath: fileName.length > 0 ? fileName : 'plan-revised.md',
          filters: [{ name: 'Markdown', extensions: ['md'] }]
        }
        const save = window ? dialog.showSaveDialog(window, saveOptions) : dialog.showSaveDialog(saveOptions)
        return save.then((selected) => selected.canceled || !selected.filePath
          ? { ok: true, value: { cancelled: true } }
          : accepted(client.send({ saveRevisedPlan: { revisionId, destinationPath: selected.filePath } })))
      }
      return accepted(client.send({ saveRevisedPlan: { revisionId, destinationPath } }))
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

    case 'provider.select': {
      const provider = asProviderKind(asRecord(payload)['provider'])
      if (provider === null) return failure('invalid-payload', 'Некорректный провайдер.')
      const profile = providers.summary().profiles[provider]
      if (!profile.configured) return failure('invalid-payload', 'Сначала настрой ключ выбранного провайдера.')
      const summary = providers.select(provider)
      log('info', 'shell.provider_selected', { provider })
      return restartCore().then((restarted) => ({ ok: true, value: { summary, restarted } }))
    }

    case 'provider.clearKey': {
      const requestedProvider = asProviderKind(asRecord(payload)['provider'])
      const summary = providers.clearKey(requestedProvider ?? undefined)
      log('info', 'shell.provider_key_cleared', {})
      return restartCore().then((restarted) => ({ ok: true, value: { summary, restarted } }))
    }

    case 'codex.getStatus':
      return codex.getStatus().then((value) => ({ ok: true, value }))

    case 'codex.refresh':
      return codex.refresh().then((value) => ({ ok: true, value }))

    case 'codex.install':
      return codex.install().then((value) => ({ ok: true, value }))

    case 'codex.login':
      return codex.login().then((value) => ({ ok: true, value }))

    case 'codex.selectModel': {
      const model = asBoundedString(asRecord(payload)['model'])
      if (model === null) return failure('invalid-payload', 'Некорректная модель Codex.')
      return codex.selectModel(model)
        .then((value) => ({ ok: true, value }))
        .catch(() => failure('invalid-payload', 'Эта модель Codex недоступна.'))
    }

    case 'repair.getStatus':
      return { ok: true, value: repair.status }

    case 'repair.start': {
      // Repair always creates its own isolated checkout. An empty value is
      // intentional: the selected workspace must neither affect nor receive
      // repair changes.
      const workspacePath = asOptionalBoundedString(asRecord(payload)['workspacePath'])
      if (workspacePath === null) return failure('invalid-payload', 'Некорректный workspace для repair-run.')
      return repair.start(workspacePath).then((value) => ({ ok: true, value }))
    }

    case 'repair.cancel':
      return { ok: true, value: repair.cancel() }

    case 'repair.commit':
      return { ok: true, value: repair.commit() }

    case 'repair.push':
      return { ok: true, value: repair.push() }

    case 'repair.refreshCI':
      return repair.refreshCi().then((value) => ({ ok: true, value }))

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

    case 'listener.getRuntimeStatus':
      return { ok: true, value: listenerRuntime.status }

    case 'listener.checkRuntime':
      return listenerRuntime.check().then((value) => ({ ok: true, value }))

    // Загрузка идёт сотнями мегабайт: renderer получает финальный статус, а
    // ход загрузки — событиями, чтобы окно не ждало ответа минутами.
    case 'listener.downloadRuntime':
      return listenerRuntime.download().then(async (value) => {
        if (value.state !== 'ready') return { ok: true, value }

        // Listener выбирает DLL и модель один раз при старте процесса. После
        // установки его нужно поднять заново, иначе он навсегда останется на
        // NullEngine, с которым был запущен до появления каталога tools.
        try {
          const restarted = await restartCore()
          if (!restarted) {
            return {
              ok: true,
              value: { ...value, message: `${value.message} Перезапусти EvoHime, чтобы открыть движок.` }
            }
          }
        } catch (error) {
          log('warn', 'shell.listener_restart_failed', { error })
          return {
            ok: true,
            value: { ...value, message: `${value.message} Перезапусти EvoHime, чтобы открыть движок.` }
          }
        }
        return { ok: true, value }
      })

    // Постоянное слушание (план 04.5). Оболочка только пересылает: ядро
    // заново проверяет capability, политику и подтверждение удаления.
    case 'ambient.setListening': {
      const value = asRecord(payload)
      const deviceId = asOptionalBoundedString(value['deviceId'])
      if (typeof value['enabled'] !== 'boolean' || typeof value['paused'] !== 'boolean' || deviceId === null) {
        return failure('invalid-payload', 'Некорректные параметры слушания.')
      }
      return accepted(
        client.send({
          setAmbientListening: { enabled: value['enabled'], paused: value['paused'], deviceId }
        })
      )
    }

    case 'ambient.getStatus':
      return accepted(client.send({ getAmbientStatus: {} }))

    case 'ambient.listEpisodes': {
      const value = asRecord(payload)
      const cursor = asOptionalBoundedString(value['cursor'])
      const limit = value['limit'] === undefined ? 50 : asBoundedNumber(value['limit'], 200)
      const sinceMs = value['sinceMs'] === undefined ? 0 : value['sinceMs']
      if (cursor === null || limit === null || typeof sinceMs !== 'number' || !Number.isFinite(sinceMs) || sinceMs < 0) {
        return failure('invalid-payload', 'Некорректные параметры списка эпизодов.')
      }
      return accepted(client.send({ listAmbientEpisodes: { sinceMs, limit, cursor } }))
    }

    case 'ambient.getEpisode': {
      const episodeId = asBoundedString(asRecord(payload)['episodeId'])
      if (episodeId === null) return failure('invalid-payload', 'Не указан эпизод.')
      return accepted(client.send({ getAmbientEpisode: { episodeId } }))
    }

    // Подтверждение проверяется и здесь, и в ядре. Оболочка не является
    // границей безопасности: без `confirmed` ядро откажет и при обходе UI.
    case 'ambient.deleteTranscripts': {
      const value = asRecord(payload)
      const all = value['all'] === true
      const episodeIds = all ? [] : asAmbientEpisodeIds(value['episodeIds'])
      if (value['confirmed'] !== true) {
        return failure('invalid-payload', 'Удаление требует подтверждения.')
      }
      if (episodeIds === null) return failure('invalid-payload', 'Некорректный список эпизодов.')
      return accepted(client.send({ deleteAmbientTranscripts: { episodeIds, all, confirmed: true } }))
    }

    case 'ambient.forgetWindow': {
      const value = asRecord(payload)
      const windowMs = asBoundedNumber(value['windowMs'], 24 * 60 * 60 * 1000)
      if (value['confirmed'] !== true) {
        return failure('invalid-payload', 'Удаление требует подтверждения.')
      }
      if (windowMs === null) return failure('invalid-payload', 'Некорректное окно удаления.')
      return accepted(client.send({ forgetAmbientWindow: { windowMs, confirmed: true } }))
    }

    case 'ambient.getPolicy':
      return accepted(client.send({ getAmbientPolicy: {} }))

    case 'ambient.savePolicy': {
      const value = asRecord(payload)
      const quietHours = asQuietHours(value['quietHours'])
      const blocklistPatterns = asAmbientPatterns(value['blocklistPatterns'])
      const windowTitleBlocklist = asAmbientPatterns(value['windowTitleBlocklist'])
      const retentionDays = asBoundedNumber(value['retentionDays'], 90)
      if (quietHours === null || blocklistPatterns === null || windowTitleBlocklist === null || retentionDays === null) {
        return failure('invalid-payload', 'Некорректная политика слушания.')
      }
      // Голосовые поля необязательны: старый вызов без них сохраняет то, что
      // уже стоит в политике, а не выключает команды молчанием.
      const voiceCommands = typeof value['voiceCommands'] === 'boolean' ? value['voiceCommands'] : undefined
      const voiceCommandsAutorun =
        typeof value['voiceCommandsAutorun'] === 'boolean' ? value['voiceCommandsAutorun'] : undefined
      return accepted(
        client.send({
          saveAmbientPolicy: {
            policy: {
              quietHours,
              blocklistPatterns,
              windowTitleBlocklist,
              retentionDays,
              ...(voiceCommands === undefined ? {} : { voiceCommands }),
              ...(voiceCommandsAutorun === undefined ? {} : { voiceCommandsAutorun })
            }
          }
        })
      )
    }

    case 'ambient.resolveProposal': {
      const value = asRecord(payload)
      const proposalId = asBoundedString(value['proposalId'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      // Ключ идемпотентности обязателен и здесь, и в ядре: принятие создаёт
      // задачу, и запрос без ключа не должен доходить до Core вовсе.
      if (proposalId === null || idempotencyKey === null || typeof value['accepted'] !== 'boolean') {
        return failure('invalid-payload', 'Некорректное решение по предложению.')
      }
      return accepted(
        client.send({
          resolveAmbientProposal: {
            proposalId,
            accepted: value['accepted'],
            idempotencyKey,
            mute: value['mute'] === true
          }
        })
      )
    }

    case 'ambient.listProposals': {
      const value = asRecord(payload)
      const limit = value['limit'] === undefined ? 50 : asBoundedNumber(value['limit'], 200)
      if (limit === null) {
        return failure('invalid-payload', 'Некорректный лимит списка предложений.')
      }
      return accepted(client.send({ listAmbientProposals: { limit } }))
    }

    case 'ambient.listVoiceCommands': {
      const value = asRecord(payload)
      const limit = value['limit'] === undefined ? 8 : asBoundedNumber(value['limit'], 8)
      if (limit === null) {
        return failure('invalid-payload', 'Некорректный лимит списка команд.')
      }
      return accepted(client.send({ listVoiceCommands: { limit } }))
    }

    case 'ambient.resolveVoiceCommand': {
      const value = asRecord(payload)
      const commandId = asBoundedString(value['commandId'])
      if (commandId === null || typeof value['accepted'] !== 'boolean') {
        return failure('invalid-payload', 'Некорректное решение по команде.')
      }
      return accepted(
        client.send({ resolveVoiceCommand: { commandId, accepted: value['accepted'] } })
      )
    }

    case 'ambient.hotkeyStatus':
      return { ok: true, value: ambientHotkey() }

    // ------------------------------------------------------------------
    // Workflow orchestration (план 06.3).
    //
    // Main-процесс здесь только курьер: он не строит граф, не считает
    // зависимости и не решает порядок узлов. Всё это принадлежит Core, а
    // сюда возвращается bounded projection.
    // ------------------------------------------------------------------

    case 'workflow.listTemplates':
      return accepted(client.send({ listWorkflowTemplates: {} }))

    case 'workflow.getDefinition': {
      const value = asRecord(payload)
      const templateId = asBoundedString(value['templateId'])
      if (templateId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор шаблона.')
      }
      return accepted(client.send({ getWorkflowDefinition: { templateId } }))
    }

    case 'workflow.start': {
      const value = asRecord(payload)
      const templateId = asBoundedString(value['templateId'])
      const workspacePath = asBoundedString(value['workspacePath'])
      // Ключ идемпотентности обязателен: без него двойной клик по кнопке
      // создал бы два запуска одного и того же шаблона.
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const inputs = asWorkflowInputs(value['inputs'])
      if (templateId === null || workspacePath === null || idempotencyKey === null) {
        return failure('invalid-payload', 'Некорректный запрос запуска workflow.')
      }
      if (inputs === null) {
        return failure('invalid-payload', 'Некорректные входы шаблона.')
      }
      const taskId = value['taskId'] === undefined ? '' : asBoundedString(value['taskId'])
      if (taskId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор задачи.')
      }
      return accepted(
        client.send({
          startWorkflow: { templateId, taskId, workspacePath, inputs, idempotencyKey }
        })
      )
    }

    case 'workflow.getRun': {
      const value = asRecord(payload)
      const runId = asBoundedString(value['runId'])
      if (runId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор запуска.')
      }
      return accepted(client.send({ getWorkflowRun: { runId } }))
    }

    case 'workflow.cancel': {
      const value = asRecord(payload)
      const runId = asBoundedString(value['runId'])
      if (runId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор запуска.')
      }
      return accepted(client.send({ cancelWorkflow: { runId } }))
    }

    case 'workflow.listEvents': {
      const value = asRecord(payload)
      const runId = asBoundedString(value['runId'])
      if (runId === null) {
        return failure('invalid-payload', 'Некорректный идентификатор запуска.')
      }
      const limit = value['limit'] === undefined ? 100 : asBoundedNumber(value['limit'], 500)
      if (limit === null) {
        return failure('invalid-payload', 'Некорректный лимит списка событий.')
      }
      const rawAfter = value['afterSequence']
      const afterSequence =
        rawAfter === undefined
          ? -1
          : typeof rawAfter === 'number' && Number.isSafeInteger(rawAfter) && rawAfter >= -1
            ? rawAfter
            : null
      if (afterSequence === null) {
        return failure('invalid-payload', 'Некорректная позиция replay.')
      }
      return accepted(client.send({ listWorkflowEvents: { runId, afterSequence, limit } }))
    }

    case 'workflowPackage.preview': {
      const value = asRecord(payload)
      const graphJson = asBoundedString(value['graphJson'])
      const name = asBoundedString(value['name'])
      const description = value['description'] === undefined ? '' : asBoundedString(value['description'])
      const createdAt = asBoundedString(value['createdAt'])
      const portableArgumentKeys = asBoundedStringArray(value['portableArgumentKeys'] ?? [])
      const credentialSlotsJson = value['credentialSlotsJson'] === undefined ? '' : asBoundedString(value['credentialSlotsJson'])
      if (graphJson === null || name === null || description === null || createdAt === null || portableArgumentKeys === null || credentialSlotsJson === null) {
        return failure('invalid-payload', 'Некорректный preview Workflow Package.')
      }
      return accepted(client.send({ previewWorkflowPackage: {
        graphJson: Buffer.from(graphJson, 'utf8'), name, description,
        portableArgumentKeys, credentialSlotsJson: Buffer.from(credentialSlotsJson, 'utf8'), createdAt
      } }))
    }

    case 'workflowPackage.export': {
      const value = asRecord(payload)
      const graphJson = asBoundedString(value['graphJson'])
      const name = asBoundedString(value['name'])
      const description = value['description'] === undefined ? '' : asBoundedString(value['description'])
      const createdAt = asBoundedString(value['createdAt'])
      const destinationPath = asBoundedString(value['destinationPath'])
      const portableArgumentKeys = asBoundedStringArray(value['portableArgumentKeys'] ?? [])
      const credentialSlotsJson = value['credentialSlotsJson'] === undefined ? '' : asBoundedString(value['credentialSlotsJson'])
      if (graphJson === null || name === null || description === null || createdAt === null || destinationPath === null || portableArgumentKeys === null || credentialSlotsJson === null) {
        return failure('invalid-payload', 'Некорректный export Workflow Package.')
      }
      return accepted(client.send({ exportWorkflowPackage: {
        graphJson: Buffer.from(graphJson, 'utf8'), name, description,
        portableArgumentKeys, credentialSlotsJson: Buffer.from(credentialSlotsJson, 'utf8'), createdAt, destinationPath
      } }))
    }

    case 'workflowPackage.commit': {
      const value = asRecord(payload)
      const packageJson = asBoundedString(value['packageJson'])
      const sourcePath = asBoundedString(value['sourcePath'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (packageJson === null || sourcePath === null || idempotencyKey === null) {
        return failure('invalid-payload', 'Некорректный commit Workflow Package.')
      }
      return accepted(client.send({ commitWorkflowPackage: {
        packageJson: Buffer.from(packageJson, 'utf8'), sourcePath, idempotencyKey
      } }))
    }

    case 'workflowPackage.rebind': {
      const value = asRecord(payload)
      const packageJson = asBoundedString(value['packageJson'])
      const slotId = asBoundedString(value['slotId'])
      const localCredentialReference = asBoundedString(value['localCredentialReference'])
      if (packageJson === null || slotId === null || localCredentialReference === null) {
        return failure('invalid-payload', 'Некорректный rebind Workflow Package.')
      }
      return accepted(client.send({ rebindWorkflowPackage: {
        packageJson: Buffer.from(packageJson, 'utf8'), slotId, localCredentialReference
      } }))
    }

    case 'workflowBuilder.command': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const draftId = asBoundedString(value['draftId'])
      const operation = asBoundedString(value['operation'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const payloadJson = value['payload'] === undefined ? '' : asBoundedString(value['payload'])
      const expectedRevision = value['expectedRevision'] === undefined ? 0 : asBoundedNumber(value['expectedRevision'], Number.MAX_SAFE_INTEGER)
      if (requestId === null || ownerScope === null || draftId === null || operation === null || idempotencyKey === null || payloadJson === null || expectedRevision === null) {
        return failure('invalid-payload', 'Некорректный запрос Builder.')
      }
      if (payloadJson.length > 512 * 1024) return failure('invalid-payload', 'Payload Builder слишком большой.')
      return accepted(client.send({ visualWorkflowBuilder: { schemaVersion: 1, requestId, ownerScope, draftId, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedRevision, idempotencyKey } }))
    }

    case 'workflowComposer.command': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const draftId = asBoundedString(value['draftId'])
      const operation = asBoundedString(value['operation'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const payloadJson = value['payload'] === undefined ? '' : asBoundedString(value['payload'])
      const expectedRevision = value['expectedRevision'] === undefined ? 0 : asBoundedNumber(value['expectedRevision'], Number.MAX_SAFE_INTEGER)
      if (requestId === null || ownerScope === null || draftId === null || operation === null || idempotencyKey === null || payloadJson === null || expectedRevision === null) {
        return failure('invalid-payload', 'Некорректный запрос Composer.')
      }
      if (payloadJson.length > 512 * 1024) return failure('invalid-payload', 'Payload Composer слишком большой.')
      return accepted(client.send({ conversationalWorkflowComposer: { schemaVersion: 1, requestId, ownerScope, draftId, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedRevision, idempotencyKey } }))
    }

    case 'integrationProvider.listCatalog': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      if (requestId === null || ownerScope === null) return failure('invalid-payload', 'Некорректный запрос каталога интеграций.')
      return accepted(client.send({ integrationProviderSdkCatalog: { schemaVersion: 1, requestId, ownerScope, operation: 'list_catalog', payload: Buffer.alloc(0), expectedVersion: 0, idempotencyKey: requestId } }))
    }

    case 'integrationProvider.command': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const operation = asBoundedString(value['operation'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const payloadJson = value['payload'] === undefined ? '' : asBoundedString(value['payload'])
      const expectedVersion = value['expectedVersion'] === undefined ? 0 : asBoundedNumber(value['expectedVersion'], Number.MAX_SAFE_INTEGER)
      if (requestId === null || ownerScope === null || operation === null || idempotencyKey === null || payloadJson === null || expectedVersion === null) return failure('invalid-payload', 'Некорректный запрос интеграции.')
      if (payloadJson.length > 64 * 1024) return failure('invalid-payload', 'Payload интеграции слишком большой.')
      return accepted(client.send({ integrationProviderSdkAction: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedVersion, idempotencyKey } }))
    }

    case 'eventTriggerRuntime.list': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      if (requestId === null || ownerScope === null) return failure('invalid-payload', 'Некорректный запрос списка триггеров.')
      return accepted(client.send({ eventTriggerRuntimeList: { schemaVersion: 1, requestId, ownerScope, operation: 'list', payload: Buffer.alloc(0), expectedVersion: 0, idempotencyKey: requestId } }))
    }

    case 'eventTriggerRuntime.command': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const operation = asBoundedString(value['operation'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const payloadJson = value['payload'] === undefined ? '' : asBoundedString(value['payload'])
      const expectedVersion = value['expectedVersion'] === undefined ? 0 : asBoundedNumber(value['expectedVersion'], Number.MAX_SAFE_INTEGER)
      if (requestId === null || ownerScope === null || operation === null || idempotencyKey === null || payloadJson === null || expectedVersion === null) return failure('invalid-payload', 'Некорректная команда триггера.')
      if (payloadJson.length > 256 * 1024) return failure('invalid-payload', 'Payload триггера слишком большой.')
      return accepted(client.send({ eventTriggerRuntimeAction: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedVersion, idempotencyKey } }))
    }

    case 'invocationPreset.list': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const limit = value['limit'] === undefined ? 50 : asBoundedNumber(value['limit'], 100)
      if (requestId === null || ownerScope === null || limit === null) return failure('invalid-payload', 'Некорректный запрос списка пресетов.')
      return accepted(client.send({ invocationPresetList: { schemaVersion: 1, requestId, ownerScope, operation: 'list', payload: Buffer.alloc(0), expectedRevision: limit, idempotencyKey: requestId } }))
    }

    case 'invocationPreset.command': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const operation = asBoundedString(value['operation'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const payloadJson = value['payload'] === undefined ? '' : asBoundedString(value['payload'])
      const expectedRevision = value['expectedRevision'] === undefined ? 0 : asBoundedNumber(value['expectedRevision'], Number.MAX_SAFE_INTEGER)
      if (requestId === null || ownerScope === null || operation === null || idempotencyKey === null || payloadJson === null || expectedRevision === null) return failure('invalid-payload', 'Некорректная команда пресета.')
      if (payloadJson.length > 128 * 1024) return failure('invalid-payload', 'Payload пресета слишком большой.')
      return accepted(client.send({ invocationPresetAction: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedRevision, idempotencyKey } }))
    }

    case 'benchmarkMatrix.list':
    case 'benchmarkMatrix.start':
    case 'benchmarkMatrix.cancel':
    case 'benchmarkMatrix.approveBaseline': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const operation = command.slice('benchmarkMatrix.'.length)
      const suiteId = value['suiteId'] === undefined ? '' : asBoundedString(value['suiteId'])
      const runId = value['runId'] === undefined ? '' : asBoundedString(value['runId'])
      const attempts = value['attempts'] === undefined ? 3 : asBoundedNumber(value['attempts'], 32)
      const expectedVersion = value['expectedVersion'] === undefined ? 0 : asBoundedNumber(value['expectedVersion'], Number.MAX_SAFE_INTEGER)
      if (requestId === null || ownerScope === null || idempotencyKey === null || suiteId === null || runId === null || attempts === null || expectedVersion === null) return failure('invalid-payload', 'Некорректная команда benchmark matrix.')
      const payloadJson = JSON.stringify({ suiteId, runId, attempts, mode: value['mode'] === 'real' ? 'real' : 'deterministic' })
      return accepted(client.send({ benchmarkMatrixAction: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedVersion, idempotencyKey } }))
    }

    case 'agentMiddleware.list':
    case 'agentMiddleware.start':
    case 'agentMiddleware.cancel': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const idempotencyKey = value['idempotencyKey'] === undefined ? '' : asBoundedString(value['idempotencyKey'])
      const runId = value['runId'] === undefined ? '' : asBoundedString(value['runId'])
      const operation = command.slice('agentMiddleware.'.length)
      if (requestId === null || ownerScope === null || idempotencyKey === null || runId === null) return failure('invalid-payload', 'Некорректная команда middleware pipeline.')
      const payloadJson = JSON.stringify({ runId })
      return accepted(client.send({ agentMiddlewarePipelineAction: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.from(payloadJson, 'utf8'), expectedVersion: 0, idempotencyKey } }))
    }

    case 'structuredResponse.list':
    case 'structuredResponse.cancel': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (requestId === null || ownerScope === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректная команда structured response.')
      const operation = command.slice('structuredResponse.'.length)
      return accepted(client.send({ structuredResponse: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.alloc(0), expectedVersion: 0, idempotencyKey } }))
    }

    case 'sensitiveDataGuardrails.status':
    case 'sensitiveDataGuardrails.evaluate': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const destination = value['destination'] === undefined ? 'provider' : asBoundedString(value['destination'])
      const input = value['input'] === undefined ? '' : asBoundedString(value['input'])
      if (requestId === null || ownerScope === null || idempotencyKey === null || destination === null || input === null || destination.length > 128 || input.length > 64 * 1024) return failure('invalid-payload', 'Некорректные параметры sensitive-data guardrails.')
      const operation = command.slice('sensitiveDataGuardrails.'.length)
      if (operation === 'evaluate' && input.length === 0) return failure('invalid-payload', 'Для evaluate нужен bounded input.')
      const payloadJson = JSON.stringify({ destination, input })
      return accepted(client.send({ sensitiveDataGuardrails: { schemaVersion: 1, requestId, ownerScope, operation, payload: Buffer.from(payloadJson, 'utf8'), idempotencyKey } }))
    }

    case 'executionPolicyProfiles.status': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const profileId = value['profileId'] === undefined ? '' : asBoundedString(value['profileId'])
      if (requestId === null || ownerScope === null || idempotencyKey === null || profileId === null) return failure('invalid-payload', 'Некорректные параметры execution policy profile.')
      return accepted(client.send({ executionPolicyProfiles: { schemaVersion: 1, requestId, operation: 'status', profileId, expectedVersion: 0, idempotencyKey } }))
    }

    case 'modelResiliencePolicy.status': {
      const value = asRecord(payload)
      const requestId = asBoundedString(value['requestId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      if (requestId === null || ownerScope === null || idempotencyKey === null) return failure('invalid-payload', 'Некорректные параметры model resilience policy.')
      return accepted(client.send({ modelResiliencePolicy: { schemaVersion: 1, requestId, ownerScope, operation: 'status', expectedVersion: 0, idempotencyKey } }))
    }

    case 'automation.listSchedules': {
      const value = asRecord(payload)
      const ownerScope = asBoundedString(value['ownerScope'])
      const limit = value['limit'] === undefined ? 0 : asBoundedNumber(value['limit'], 256)
      if (ownerScope === null || limit === null) {
        return failure('invalid-payload', 'Некорректный запрос списка расписаний.')
      }
      return accepted(client.send({ listAutomationSchedules: { ownerScope, limit } }))
    }

    case 'automation.saveSchedule': {
      const value = asRecord(payload)
      const scheduleId = asBoundedString(value['scheduleId'])
      const definitionId = asBoundedString(value['definitionId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const revision = value['revision']
      const hour = value['hour']
      const minute = value['minute']
      const timezoneMinutes = value['timezoneMinutes']
      const missedGraceMs = value['missedGraceMs']
      const enabled = value['enabled']
      const presetId = value['presetId'] === undefined ? '' : asBoundedString(value['presetId'])
      const presetRevision = value['presetRevision'] === undefined ? 0 : asBoundedNumber(value['presetRevision'], Number.MAX_SAFE_INTEGER)
      const presetContentHash = value['presetContentHash'] === undefined ? '' : asBoundedString(value['presetContentHash'])
      const workspacePath = value['workspacePath'] === undefined ? '' : asBoundedString(value['workspacePath'])
      const validInteger = (input: unknown): input is number =>
        typeof input === 'number' && Number.isSafeInteger(input)
      if (
        scheduleId === null ||
        definitionId === null ||
        ownerScope === null ||
        !validInteger(revision) ||
        revision <= 0 ||
        !validInteger(hour) ||
        hour < 0 ||
        hour > 23 ||
        !validInteger(minute) ||
        minute < 0 ||
        minute > 59 ||
        !validInteger(timezoneMinutes) ||
        timezoneMinutes < -1439 ||
        timezoneMinutes > 1439 ||
        !validInteger(missedGraceMs) ||
        missedGraceMs < 0 ||
        missedGraceMs > 86_400_000 ||
        typeof enabled !== 'boolean'
        || presetId === null || presetRevision === null || presetContentHash === null || workspacePath === null
        || (presetId !== '' && (presetRevision <= 0 || presetContentHash === ''))
      ) {
        return failure('invalid-payload', 'Некорректная политика расписания.')
      }
      return accepted(
        client.send({
          saveAutomationSchedule: {
            scheduleId,
            definitionId,
            revision,
            ownerScope,
            hour,
            minute,
            timezoneMinutes,
            missedGraceMs,
            enabled
            , presetId, presetRevision, presetContentHash, workspacePath
          }
        })
      )
    }

    case 'automation.trigger': {
      const value = asRecord(payload)
      const definitionId = asBoundedString(value['definitionId'])
      const ownerScope = asBoundedString(value['ownerScope'])
      const triggerKey = asBoundedString(value['triggerKey'])
      const correlationId = asBoundedString(value['correlationId'])
      const idempotencyKey = asBoundedString(value['idempotencyKey'])
      const inputJson = value['inputJson']
      const revision = value['revision']
      if (
        definitionId === null || ownerScope === null || triggerKey === null ||
        correlationId === null || idempotencyKey === null ||
        typeof inputJson !== 'string' || inputJson.length > 64 * 1024 ||
        typeof revision !== 'number' || !Number.isSafeInteger(revision) || revision <= 0
      ) {
        return failure('invalid-payload', 'Некорректный запуск automation.')
      }
      return accepted(client.send({ triggerAutomation: {
        definitionId, revision, ownerScope, triggerKey, inputJson, correlationId, idempotencyKey
      } }))
    }

    case 'automation.listRuns': {
      const value = asRecord(payload)
      const ownerScope = asBoundedString(value['ownerScope'])
      const definitionId = value['definitionId'] === undefined ? '' : asBoundedString(value['definitionId'])
      const limit = value['limit'] === undefined ? 0 : asBoundedNumber(value['limit'], 256)
      if (ownerScope === null || definitionId === null || limit === null) {
        return failure('invalid-payload', 'Некорректный запрос списка запусков.')
      }
      return accepted(client.send({ listAutomationRuns: { ownerScope, definitionId, limit } }))
    }

    case 'automation.getRun': {
      const value = asRecord(payload)
      const runId = asBoundedString(value['runId'])
      if (runId === null) return failure('invalid-payload', 'Некорректный идентификатор запуска.')
      return accepted(client.send({ getAutomationRun: { runId } }))
    }

    case 'automation.listEvents': {
      const value = asRecord(payload)
      const runId = asBoundedString(value['runId'])
      const limit = value['limit'] === undefined ? 0 : asBoundedNumber(value['limit'], 256)
      const rawAfter = value['afterSequence']
      const afterSequence = rawAfter === undefined
        ? -1
        : typeof rawAfter === 'number' && Number.isSafeInteger(rawAfter) && rawAfter >= -1
          ? rawAfter
          : null
      if (runId === null || limit === null || afterSequence === null) {
        return failure('invalid-payload', 'Некорректный запрос событий automation.')
      }
      return accepted(client.send({ listAutomationEvents: { runId, afterSequence, limit } }))
    }

    case 'automation.cancel': {
      const value = asRecord(payload)
      const runId = asBoundedString(value['runId'])
      if (runId === null) return failure('invalid-payload', 'Некорректный идентификатор запуска.')
      return accepted(client.send({ cancelAutomationRun: { runId } }))
    }

    case 'automation.setScheduleEnabled': {
      const value = asRecord(payload)
      const scheduleId = asBoundedString(value['scheduleId'])
      if (scheduleId === null || typeof value['enabled'] !== 'boolean') {
        return failure('invalid-payload', 'Некорректное состояние расписания.')
      }
      return accepted(client.send({ setAutomationScheduleEnabled: { scheduleId, enabled: value['enabled'] } }))
    }
  }
}

/**
 * Входы шаблона: ограниченная карта строк.
 *
 * Ядро всё равно проверит имена и длины по контракту шаблона; здесь
 * отсекается только заведомо неподходящая форма, чтобы в очередь не уходил
 * объект произвольной глубины.
 */
function asWorkflowInputs(value: unknown): { name: string; value: string }[] | null {
  if (value === undefined) return []
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null
  const entries = Object.entries(value as Record<string, unknown>)
  if (entries.length > 16) return null
  const inputs: { name: string; value: string }[] = []
  for (const [name, raw] of entries) {
    const boundedName = asBoundedString(name)
    // Пустое значение допустимо: обязательность входа объявляет шаблон, и
    // отказ по нему должен приходить от ядра с typed-кодом, а не отсюда.
    if (boundedName === null || typeof raw !== 'string' || raw.length > MAX_TEXT_FIELD_CHARS) {
      return null
    }
    inputs.push({ name: boundedName, value: raw })
  }
  return inputs
}

/**
 * Ограниченный список эпизодов для удаления. Ядро всё равно проверит каждый
 * идентификатор; здесь отсекается только заведомый мусор.
 */
function asAmbientEpisodeIds(value: unknown): string[] | null {
  if (value === undefined) return []
  if (!Array.isArray(value) || value.length > 200) return null
  const ids = value.map((entry) => asBoundedString(entry))
  return ids.every((id): id is string => id !== null) ? ids : null
}

/**
 * Шаблоны чёрного списка. Полная проверка (глоб без метасимволов регулярных
 * выражений, лимиты) живёт в контракте 04.1 и выполняется ядром.
 */
function asAmbientPatterns(value: unknown): string[] | null {
  if (value === undefined) return []
  if (!Array.isArray(value) || value.length > 64) return null
  const patterns = value.map((entry) => asBoundedString(entry))
  return patterns.every((pattern): pattern is string => pattern !== null) ? patterns : null
}

/** Окна тишины в минутах суток; полуоткрытые и, возможно, через полночь. */
function asQuietHours(value: unknown): { startMinute: number; endMinute: number }[] | null {
  if (value === undefined) return []
  if (!Array.isArray(value) || value.length > 16) return null
  const windows = value.map((entry) => {
    const record = asRecord(entry)
    const startMinute = record['startMinute']
    const endMinute = record['endMinute']
    if (
      typeof startMinute !== 'number' ||
      typeof endMinute !== 'number' ||
      !Number.isInteger(startMinute) ||
      !Number.isInteger(endMinute) ||
      startMinute < 0 ||
      endMinute < 0 ||
      startMinute >= 1440 ||
      endMinute >= 1440
    ) {
      return null
    }
    return { startMinute, endMinute }
  })
  return windows.every((window): window is { startMinute: number; endMinute: number } => window !== null)
    ? windows
    : null
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

function asBoundedStringArray(value: unknown): string[] | null {
  if (!Array.isArray(value) || value.length > 128) return null
  const result: string[] = []
  for (const item of value) {
    const stringValue = asBoundedString(item)
    if (stringValue === null || stringValue.length > 128) return null
    result.push(stringValue)
  }
  return result
}

function isSafeSkillId(value: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(value) && !value.includes('..')
}

function isSafeSkillReference(value: string): boolean {
  return value.length <= 256 && !value.startsWith('/') && !value.startsWith('\\\\') && !value.split(/[\\/]+/u).includes('..')
}

function asGoalToken(value: unknown): string | null {
  const token = asBoundedString(value)
  return token !== null && /^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/u.test(token) ? token : null
}

type GoalCriterionPayload = {
  readonly id: string
  readonly kind: 'manual' | 'gate' | 'workflow_evidence' | 'artifact'
  readonly statement: string
}

function asGoalCriteria(value: unknown): readonly GoalCriterionPayload[] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > 64) return null
  const criteria = value.map((entry): GoalCriterionPayload | null => {
    const record = asRecord(entry)
    const id = asGoalToken(record['id'])
    const statement = asBoundedString(record['statement'])
    const kind = record['kind'] === 'manual' || record['kind'] === 'gate' || record['kind'] === 'workflow_evidence' || record['kind'] === 'artifact'
      ? record['kind']
      : null
    return id !== null && statement !== null && statement.trim().length > 0 && kind !== null
      ? { id, kind, statement }
      : null
  })
  return criteria.every((criterion): criterion is GoalCriterionPayload => criterion !== null) ? criteria : null
}

function asOptionalGoalBudget(value: unknown): number | null {
  if (value === undefined || value === null || value === 0) return 0
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 && value <= 0xffffffff ? value : null
}

function asGoalReferenceKind(value: unknown): 'workflow' | 'child' | 'checkpoint' | null {
  return value === 'workflow' || value === 'child' || value === 'checkpoint' ? value : null
}

function asTraceContent(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 && Buffer.byteLength(value, 'utf8') <= MAX_TRACE_EXPORT_BYTES
    ? value
    : null
}

function asOptionalBoundedString(value: unknown): string | null {
  if (value === undefined || value === '') return ''
  return asBoundedString(value)
}

/** Scope kinds Core accepts for memory commands; anything else is refused here. */
function asMemoryScopeKind(value: unknown): string | null {
  return value === 'project' || value === 'task' || value === 'workspace' || value === 'session'
    ? value
    : null
}

/**
 * Bounded batch of memory ids. The main process only forwards; Core still
 * re-checks the approval token, the idempotency key and every id.
 */
function asMemoryIds(value: unknown): readonly string[] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > 64) return null
  const ids = value.map((entry) => asBoundedString(entry))
  return ids.every((id): id is string => id !== null) ? ids : null
}

function asRetainedStringArray(value: unknown, maxItems: number, maxChars: number): string[] | null {
  if (!Array.isArray(value) || value.length > maxItems) return null
  const items = value.map((entry) => typeof entry === 'string' && entry.length > 0 && entry.length <= maxChars ? entry : null)
  return items.every((item): item is string => item !== null) ? items : null
}

/** The supersession reason is a closed enum, never free text. */
function asSupersessionReason(value: unknown): string | null {
  return value === 'user_choice' || value === 'revalidated' || value === 'expired' || value === 'corrected'
    ? value
    : null
}

function asBoundedNumber(value: unknown, maximum: number): number | null {
  if (value === undefined) return maximum
  return typeof value === 'number' && Number.isInteger(value) && value > 0 && value <= maximum ? value : null
}

function asNonNegativeInteger(value: unknown): number | null {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null
}

/** Unlike `asBoundedNumber`, an omitted limit means "let Core apply its own
 * per-command default" (encoded as 0), not "request the maximum". */
function asOptionalLimit(value: unknown, maximum: number): number | null {
  if (value === undefined) return 0
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

function asReviewMarkdown(value: unknown): string | null {
  return typeof value === 'string' && value.trim().length > 0 && Buffer.byteLength(value, 'utf8') <= MAX_REVIEW_PLAN_BYTES
    ? value
    : null
}

function asReviewFileNames(value: unknown): readonly string[] | null {
  if (!Array.isArray(value) || value.length === 0 || value.length > 64) return null
  const names = value.map((item) => asBoundedString(item))
  return names.every((name): name is string => name !== null) ? names : null
}

/**
 * Пути проверяемых планов. В отличие от списка имён, пустой список допустим:
 * ревью запускают и по документу, у которого нет файла на диске.
 */
function asReviewSourcePaths(value: unknown): readonly string[] | null {
  if (value === undefined) return []
  if (!Array.isArray(value) || value.length > 64) return null
  const paths = value.map((item) => asBoundedString(item))
  return paths.every((path): path is string => path !== null) ? paths : null
}

function asReviewDestinationPath(value: unknown): string | null {
  return typeof value === 'string' && value.length <= MAX_TEXT_FIELD_CHARS ? value : null
}

function asReviewModels(value: unknown): readonly string[] | null {
  if (!Array.isArray(value) || value.length < 2 || value.length > 8) return null
  const models = value.map((item) => asBoundedString(item))
  if (!models.every((model): model is string => model !== null)) return null
  return new Set(models).size === models.length ? models : null
}

/**
 * `directory` — папка прошлого выбора: планы почти всегда лежат рядом, поэтому
 * диалог открывается там же. Каталог мог быть удалён или переименован, так что
 * недоступный путь просто игнорируется.
 *
 * Пока прошлого выбора нет, Electron открыл бы папку загрузок — планов там не
 * бывает. Поэтому первым делом предлагается рабочая папка, а если в ней есть
 * `docs/plans`, то сразу она.
 *
 * Файлов можно выбрать несколько: ядро принимает один документ, поэтому склейку
 * делает панель — здесь важно лишь, чтобы суммарный размер уже прошёл проверку
 * и пользователь узнал о превышении до запуска ревью.
 */
async function pickReviewPlan(directory: unknown, workspace: string | null): Promise<unknown> {
  const window = BrowserWindow.getFocusedWindow()
  const options: Electron.OpenDialogOptions = {
    properties: ['openFile', 'multiSelections'],
    filters: [{ name: 'Markdown', extensions: ['md'] }]
  }
  const startIn = await firstUsableDirectory([
    directory,
    workspace === null ? null : joinWorkspacePath(workspace, 'docs', 'plans'),
    workspace
  ])
  if (startIn !== null) options.defaultPath = startIn
  const selected = window
    ? await dialog.showOpenDialog(window, options)
    : await dialog.showOpenDialog(options)
  if (selected.canceled || selected.filePaths.length === 0) {
    return { ok: true, value: { cancelled: true, files: [], directory: '' } }
  }
  const files: { fileName: string; sourceMarkdown: string; path: string }[] = []
  let total = 0
  for (const path of selected.filePaths) {
    if (extname(path).toLowerCase() !== '.md') return failure('invalid-payload', 'Нужны Markdown-файлы с расширением .md.')
    const content = await readFile(path, 'utf8')
    total += Buffer.byteLength(content, 'utf8')
    if (total > MAX_REVIEW_PLAN_BYTES) {
      return failure('invalid-payload', 'Выбранные планы в сумме превышают 512 КБ.')
    }
    files.push({ fileName: basename(path), sourceMarkdown: content, path })
  }
  const last = selected.filePaths[selected.filePaths.length - 1] as string
  return { ok: true, value: { cancelled: false, files, directory: dirname(last) } }
}

function joinWorkspacePath(workspace: string, ...parts: string[]): string {
  return /^[A-Za-z]:[\\/]/.test(workspace) || /^\\\\/.test(workspace)
    ? win32.join(workspace, ...parts)
    : join(workspace, ...parts)
}

async function firstUsableDirectory(candidates: readonly unknown[]): Promise<string | null> {
  for (const candidate of candidates) {
    const directory = await usableDirectory(candidate)
    if (directory !== null) return directory
  }
  return null
}

async function usableDirectory(value: unknown): Promise<string | null> {
  const path = asBoundedString(value)
  if (path === null || path.length === 0 || !isAbsolute(path) && !/^[A-Za-z]:[\\/]/.test(path) && !/^\\\\/.test(path)) return null
  try {
    return (await stat(path)).isDirectory() ? path : null
  } catch {
    return null
  }
}
