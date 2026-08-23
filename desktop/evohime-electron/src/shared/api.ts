/**
 * Contract shared by preload and renderer.
 *
 * This file must stay free of Electron and Node imports: it is compiled into
 * the sandboxed renderer bundle, where neither is available.
 */

import type { ListenerRuntimeStatus } from './listener-runtime'
import type { UpdateStatus } from './update'

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

/**
 * Bounded typed execution-ledger projection (plan 08-1/08-2/08-3), decoded
 * from `EventEnvelope.executionEvent` when Core emitted a typed `ledger.*`
 * row. `body` is already-parsed JSON of the ExecutionEventV1 variant named
 * by `CoreEvent.eventType` (e.g. `ledger.tool_call`); it carries no raw
 * prompt/secret content — Core never puts that there either.
 */
export interface TypedExecutionEvent {
  readonly schemaVersion: number
  readonly eventId: string
  readonly runScope: string
  readonly runId: string
  readonly sessionId: string
  readonly createdAtMs: number
  readonly stateAfter: string
  readonly actionId: string
  readonly toolCallId: string
  readonly observationId: string
  readonly receiptId: string
  readonly failureId: string
  readonly workflowRunId: string
  readonly nodeId: string
  readonly attemptId: string
  readonly effectId: string
  readonly modelRequestId: string
  readonly body: unknown
  readonly secretsPresent: boolean
  readonly redactionDigest: string
}

export interface CoreEvent {
  readonly sequenceId: number
  readonly taskId: string
  readonly eventType: string
  /** Redacted UTF-8 payload as produced by Core; never a secret value. */
  readonly payload: string
  /** Present only for typed `ledger.*` rows (plan 08-3); null otherwise. */
  readonly executionEvent: TypedExecutionEvent | null
}

export type ShellEvent =
  | { readonly kind: 'state'; readonly state: ShellState }
  | { readonly kind: 'core-event'; readonly event: CoreEvent }
  | { readonly kind: 'update'; readonly status: UpdateStatus }
  | { readonly kind: 'listener-runtime'; readonly status: ListenerRuntimeStatus }

/** Model providers the shell can configure. */
export const PROVIDER_KINDS = ['literouter', 'openai_compatible'] as const

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

/** Git state of the open project, shown above the composer. */
export interface RepositorySummary {
  readonly branch: string
  readonly added: number
  readonly removed: number
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
 * Один выбранный Markdown-план. Ревью запускается по одному документу, поэтому
 * несколько файлов оболочка склеивает перед отправкой в ядро — здесь они ещё
 * лежат раздельно, чтобы список в панели можно было править по одному файлу.
 */
export interface PlanFile {
  readonly fileName: string
  readonly sourceMarkdown: string
  /**
   * Абсолютный путь файла. Пустая строка означает «путь неизвестен»: так
   * бывает при перетаскивании из источника без файловой системы. Тогда
   * исправленный план можно только сохранить через диалог, но не записать
   * поверх оригинала.
   */
  readonly path: string
}

/**
 * Лимиты модели, как их сообщил провайдер. `null` означает «провайдер не
 * сказал» — это не «без ограничений», поэтому проверки при неизвестном окне
 * молчат, а не разрешают запуск как заведомо безопасный.
 */
export interface ModelLimits {
  readonly context: number | null
  readonly maxOutput: number | null
}

export interface PlanReviewReviewer {
  readonly model: string
  readonly status: string
  readonly content: string
  readonly error: string | null
}

export interface PlanReviewResult {
  readonly reviewId: string
  readonly fileName: string
  readonly fileNames: readonly string[]
  readonly synthesisModel: string
  readonly reviewers: readonly PlanReviewReviewer[]
  readonly finalMarkdown: string
}

/**
 * Исправленный по ревью план. Живёт в памяти ядра до явного сохранения:
 * показать правку и записать её — намеренно два разных действия.
 */
export interface PlanRevisionResult {
  readonly revisionId: string
  readonly reviewId: string
  readonly fileName: string
  readonly model: string
  readonly revisedMarkdown: string
  /**
   * Соседние планы, с которыми ядро сверяло правку. Пустой список означает, что
   * редактор работал по одному файлу и мог разойтись с соседним этапом, — это
   * видно в карточке до сохранения.
   */
  readonly contextFiles: readonly string[]
}

/**
 * Состояние постоянного слушания, как его называет контракт 04.1.
 *
 * Значения совпадают со снимком, который отдаёт Core: оболочка ничего не
 * переименовывает, чтобы строка в интерфейсе соответствовала строке в
 * журнале.
 */
export type ListeningState =
  | 'stopped'
  | 'starting'
  | 'listening'
  | 'paused_by_user'
  | 'paused_by_policy'
  | 'device_conflict'
  | 'device_disconnected'
  | 'engine_unavailable'
  | 'denied'

export type ListeningReason =
  | 'user_request'
  | 'quiet_hours'
  | 'blocklist'
  | 'stop_word'
  | 'permission_denied'
  | 'device_conflict'
  | 'device_disconnected'
  | 'engine_unavailable'
  | 'engine_degraded'
  | 'system_sleep'
  | 'storage_failed'
  | 'unknown'

export type AmbientExtractionState = 'disabled' | 'pending' | 'done' | 'failed'

/** Закрытый набор кодов ошибок ambient-команд. */
export type AmbientErrorCode =
  | 'LISTENER_UNAVAILABLE'
  | 'DEVICE_CONFLICT'
  | 'DEVICE_DISCONNECTED'
  | 'PERMISSION_DENIED'
  | 'POLICY_INVALID'
  | 'ENGINE_NOT_READY'
  | 'STORAGE_FAILED'
  | 'CONFIRMATION_REQUIRED'
  | 'INVALID_ARGUMENT'

export interface AmbientDevice {
  readonly device_id: string
  readonly display_name: string
  readonly is_default: boolean
  readonly is_active: boolean
}

/** Полезная нагрузка события `ambient.status`. */
export interface AmbientStatus {
  readonly state: ListeningState
  readonly reason: ListeningReason
  readonly active_device_id: string
  readonly engine_version: string
  readonly engine_ready: boolean
  readonly devices: readonly AmbientDevice[]
  /**
   * Живёт ли подписка на смену устройств. `false` означает, что список —
   * снимок, который сам не обновится: панель обязана это сказать, а не
   * показывать устаревший список как живой.
   */
  readonly watching_devices: boolean
}

/** Строка списка эпизодов. Текста здесь нет по построению. */
export interface AmbientEpisodeSummary {
  readonly episode_id: string
  readonly started_at_ms: number
  readonly speech_duration_ms: number
  readonly utterance_count: number
  readonly extraction_state: AmbientExtractionState
}

export interface AmbientUtterance {
  readonly utterance_id: string
  readonly started_at_ms: number
  readonly duration_ms: number
  readonly text: string
  readonly language: string
  readonly redacted: boolean
}

export interface AmbientQuietHours {
  readonly start_minute: number
  readonly end_minute: number
}

export interface AmbientPolicy {
  readonly quiet_hours: readonly AmbientQuietHours[]
  /** Шаблоны имён процессов. */
  readonly blocklist_patterns: readonly string[]
  readonly window_title_blocklist: readonly string[]
  readonly retention_days: number
  /** Распознавать ли обращения «Ева, открой …». */
  readonly voice_commands: boolean
  /** Открывать услышанное без подтверждения. */
  readonly voice_commands_autorun: boolean
}

/** Что услышанная команда просит сделать. */
export type VoiceCommandKind = 'open_app'

export type VoiceCommandState = 'pending' | 'launched' | 'declined' | 'expired' | 'failed'

/**
 * Одна услышанная команда, ждущая решения.
 *
 * `title` приходит только командой `ambient.listVoiceCommands`: durable-событие
 * `ambient.voice_command` несёт лишь `app_id`, потому что журнал не пересказывает
 * сказанное — он перечисляет опознанные ключи каталога.
 */
export interface VoiceCommand {
  readonly command_id: string
  readonly kind: VoiceCommandKind
  readonly app_id: string
  readonly title: string
  readonly created_at_ms: number
  readonly expires_at_ms: number
}

export interface VoiceCommandList {
  readonly commands: readonly VoiceCommand[]
  /** `false` — пользователь разрешил автозапуск, очередь в норме пуста. */
  readonly requires_confirmation: boolean
  readonly error_code: string
}

export interface VoiceCommandResolved {
  readonly launched: boolean
  readonly state: VoiceCommandState
  readonly app_id: string
  readonly error_code: string
}

/** Что предложение сделает, если его принять. */
export type AmbientProposalKind = 'suggestion' | 'reminder'

export type AmbientProposalState = 'proposed' | 'accepted' | 'declined' | 'muted' | 'expired'

/**
 * Одна карточка ограниченного предложения (этап 04.7).
 *
 * Человекочитаемый `title` приходит только командой `ambient.listProposals`:
 * durable-событие `ambient.proposal` его не несёт — по той же причине, по
 * которой `memory.pending` не несёт `statement`.
 */
export interface AmbientProposal {
  readonly proposal_id: string
  readonly kind: AmbientProposalKind
  readonly subject: string
  readonly title: string
  readonly source_episode_id: string
  readonly created_at_ms: number
  readonly expires_at_ms: number
  /** Сколько раз это предложили. Повтор поднимает счётчик, а не плодит карточки. */
  readonly occurrences: number
  readonly state: AmbientProposalState
}

/**
 * Список карточек и потолок проактивности.
 *
 * Потолок неизменяем: оболочка его показывает, но поднять не может — это
 * снимок контракта 04.1, а не настройка.
 */
export interface AmbientProposalList {
  readonly proposals: readonly AmbientProposal[]
  readonly max_per_hour: number
  readonly max_per_day: number
  readonly min_interval_ms: number
  readonly error_code: string
}

/** Ответ на решение по карточке. */
export interface AmbientProposalResolution {
  readonly applied: boolean
  readonly state: AmbientProposalState | ''
  readonly task_id: string
  readonly error_code: string
}

/**
 * Полезная нагрузка durable-события `ambient.proposal`.
 *
 * Ни текста карточки, ни темы человеческими словами здесь нет: тема сведена к
 * bounded-токену `subject_key`.
 */
export interface AmbientProposalEvent {
  readonly proposal_id: string
  readonly episode_id: string | null
  readonly kind: AmbientProposalKind
  readonly subject_key: string
  readonly proposal_state: AmbientProposalState
}

/** Доступен ли глобальный хоткей паузы, и почему нет. */
export interface AmbientHotkeyStatus {
  readonly combination: string
  readonly registered: boolean
}

/**
 * Workflow orchestration (план 06.3).
 *
 * Всё, что renderer знает о запуске, — это проекция Core: идентификаторы,
 * состояния, роли и коды ошибок. Ни prompt, ни цель child-узла, ни сырой
 * вывод инструмента в эти типы не входят по построению, потому что их нет в
 * ответе ядра.
 */
export type WorkflowScheduleEligibility = 'interval_only' | 'unavailable'

export interface WorkflowTemplateInput {
  readonly name: string
  readonly title: string
  readonly required: boolean
  readonly max_chars: number
}

export interface WorkflowTemplateSummary {
  readonly template_id: string
  readonly version: number
  readonly display_name: string
  readonly description: string
  readonly inputs: readonly WorkflowTemplateInput[]
  readonly required_capabilities: readonly string[]
  readonly schedule_eligibility: WorkflowScheduleEligibility
  readonly preview: readonly string[]
  readonly node_count: number
}

export interface WorkflowTemplateList {
  readonly templates: readonly WorkflowTemplateSummary[]
  readonly error_code: string
}

export interface WorkflowDefinitionNode {
  readonly node_id: string
  readonly action_kind: string
  readonly approval_required: boolean
  readonly block_id: string
  readonly block_version: number
}

export interface WorkflowDefinitionEdge {
  readonly from_node: string
  readonly to_node: string
  readonly channel: 'data' | 'failure'
}

export interface WorkflowDefinition {
  readonly template_id: string
  readonly version: number
  readonly display_name: string
  readonly graph_id: string
  readonly graph_version: number
  readonly graph_hash: string
  readonly schedule_eligibility: WorkflowScheduleEligibility
  readonly preview: readonly string[]
  readonly nodes: readonly WorkflowDefinitionNode[]
  readonly edges: readonly WorkflowDefinitionEdge[]
  readonly error_code: string
}

export interface WorkflowStartResult {
  readonly run_id: string
  readonly state: string
  readonly graph_hash: string
  readonly deduplicated: boolean
  readonly error_code: string
}

export interface WorkflowNodeProjection {
  readonly node_id: string
  readonly action_kind: string
  readonly role: string
  readonly state: string
  readonly attempts: number
  readonly error_code: string
  readonly message: string
  readonly approval_id: string
  readonly dependencies: readonly string[]
}

export interface WorkflowRunProjection {
  readonly run_id: string
  readonly task_id: string
  readonly template_id: string
  readonly template_version: number
  readonly graph_id: string
  readonly graph_version: number
  readonly graph_hash: string
  /** `unknown_state`, когда Core не знает такого запуска. */
  readonly state: string
  readonly terminal_reason: string
  readonly created_at_ms: number
  readonly updated_at_ms: number
  readonly nodes: readonly WorkflowNodeProjection[]
  readonly error_code: string
}

export interface WorkflowEventEntry {
  readonly sequence: number
  readonly node_id: string
  readonly event_type: string
  readonly payload: string
  readonly created_at_ms: number
}

export interface WorkflowEventList {
  readonly run_id: string
  readonly events: readonly WorkflowEventEntry[]
  readonly error_code: string
}

export interface WorkflowCancelResult {
  readonly run_id: string
  readonly cancelled: boolean
  readonly error_code: string
}

export type PermissionMode = 'ask' | 'read_only' | 'full'

/**
 * Commands the renderer may ask the main process to forward to Core. The main
 * process only forwards; Core re-validates capability, policy and approval for
 * every one of them.
 */
export const RENDERER_COMMANDS = [
  'shell.getState',
  'shell.requestResync',
  'trace.export',
  'workspace.list',
  'workspace.pick',
  'workspace.select',
  'workspace.forget',
  'core.startTask',
  'core.stopTask',
  'core.resolveApproval',
  'core.resolveRoutingDecision',
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
  'core.cancelDatabaseOperation',
  'core.getModelConfig',
  'core.listModelCatalog',
  'core.selectModel',
  'core.getReceiptKeyStatus',
  'core.trustReceiptGenesis',
  'core.rotateReceiptKey',
  'core.createNewReceiptGenesis',
  'core.listMemoryPending',
  'core.getMemoryConflicts',
  'core.getMemory',
  'core.confirmMemory',
  'core.rejectMemory',
  'core.supersedeMemory',
  'core.reviseMemoryCandidate',
  'core.getContextLedger',
  'core.listTaskScratchpad',
  'core.clearTaskScratchpad',
  'core.summarizeContextNow',
  'core.pinContextItem',
  'core.readContextArtifact',
  'core.indexWorkspace',
  'core.rebuildIndex',
  'core.searchWorkspaceKnowledge',
  'core.getIndexStatus',
  'core.cancelWorkspaceIndex',
  'core.listReceipts',
  'core.verifyReceipts',
  'core.exportReceipts',
  'identity.get',
  'repository.get',
  'chat.list',
  'chat.create',
  'chat.open',
  'chat.appendPrompt',
  'chat.remove',
  'review.pickPlan',
  'review.start',
  'review.stop',
  'review.list',
  'review.get',
  'review.export',
  'review.clearHistory',
  'review.revise',
  'review.stopRevision',
  'review.saveRevision',
  'provider.get',
  'provider.save',
  'provider.clearKey',
  'core.createProject',
  'core.prepareBuild',
  'core.applyApprovedBuild',
  'core.terminalExecute',
  'update.getStatus',
  'update.check',
  'update.prepare',
  'update.restart',
  'update.skip',
  'listener.getRuntimeStatus',
  'listener.checkRuntime',
  'listener.downloadRuntime',
  'ambient.setListening',
  'ambient.getStatus',
  'ambient.listEpisodes',
  'ambient.getEpisode',
  'ambient.deleteTranscripts',
  'ambient.forgetWindow',
  'ambient.getPolicy',
  'ambient.savePolicy',
  'ambient.resolveProposal',
  'ambient.listProposals',
  'ambient.listVoiceCommands',
  'ambient.resolveVoiceCommand',
  // Не команда ядра: доступность глобального хоткея знает только main, и
  // спросить её больше негде. Без этого ответа панель молча изображала бы
  // работающую третью точку входа.
  'ambient.hotkeyStatus',
  // Workflow orchestration (план 06.3): renderer только просит и показывает.
  // Подтверждение узла решается уже существующей 'core.resolveApproval'.
  'workflow.listTemplates',
  'workflow.getDefinition',
  'workflow.start',
  'workflow.getRun',
  'workflow.cancel',
  'workflow.listEvents'
] as const

export type RendererCommand = (typeof RENDERER_COMMANDS)[number]

/** One remembered workspace and whether it is still usable on this machine. */
export interface WorkspaceOption {
  readonly path: string
  /** False when the directory is gone or unreadable; the UI says so. */
  readonly available: boolean
  readonly lastUsedMs: number
  /** Permission mode restored when this workspace is selected. */
  readonly permissionMode?: PermissionMode
}

export interface WorkspaceSelection {
  readonly selected: string | null
  readonly options: readonly WorkspaceOption[]
  /** Mode belonging to the selected workspace, or the safe default. */
  readonly permissionMode?: PermissionMode
}

export interface CommandPayloads {
  'shell.getState': Record<string, never>
  'shell.requestResync': Record<string, never>
  'trace.export': { content: string }
  'workspace.list': Record<string, never>
  'workspace.pick': Record<string, never>
  'workspace.select': { path: string }
  'workspace.forget': { path: string }
  'core.startTask': { taskId: string; prompt: string; workspacePath: string; preferredRouteHint?: 'local' | 'cloud' | null }
  'core.stopTask': { taskId: string }
  'core.resolveApproval': { approvalId: string; granted: boolean; idempotencyKey?: string; rejectionReason?: string; cancel?: boolean }
  'core.resolveRoutingDecision': { traceId: string; approve: boolean }
  'core.listWorkspace': { workspacePath: string; relativePath: string; maxEntries?: number }
  'core.readWorkspaceFile': { workspacePath: string; relativePath: string; maxBytes?: number }
  'core.getContextLedger': { taskId: string; limit?: number }
  'core.listTaskScratchpad': {
    taskId: string
    category?: string
    status?: string
    limit?: number
  }
  'core.clearTaskScratchpad': { taskId: string }
  'core.summarizeContextNow': { taskId: string }
  'core.pinContextItem': { taskId: string; itemId: string; pinned: boolean }
  'core.readContextArtifact': { taskId: string; locator: string }
  'core.indexWorkspace': { workspacePath: string; enableEmbeddings?: boolean }
  'core.rebuildIndex': { workspacePath: string; enableEmbeddings?: boolean }
  'core.searchWorkspaceKnowledge': {
    workspacePath: string
    query: string
    pathFilter?: string
    languageFilter?: string
    hybrid?: boolean
  }
  'core.getIndexStatus': { workspacePath: string }
  'core.cancelWorkspaceIndex': { workspacePath: string }
  /** Stage 01.4: bounded, filtered receipt chain listing. */
  'core.listReceipts': {
    taskId?: string
    runId?: string
    actionId?: string
    fromRfc3339?: string
    toRfc3339?: string
    limit?: number
  }
  /** Stage 01.4: synchronous verify-chain over the filtered closure. */
  'core.verifyReceipts': {
    taskId?: string
    runId?: string
    actionId?: string
    fromRfc3339?: string
    toRfc3339?: string
    limit?: number
    trustKeyId?: string
  }
  /** Stage 01.4: atomic JSONL export bundle to a shell-selected directory. */
  'core.exportReceipts': {
    destinationPath: string
    taskId?: string
    runId?: string
    actionId?: string
    fromRfc3339?: string
    toRfc3339?: string
    limit?: number
  }
  'core.gitStatus': { workspacePath: string; maxBytes?: number }
  'core.gitDiff': { workspacePath: string; relativePath?: string; maxBytes?: number }
  'core.setPermissionMode': { mode: PermissionMode }
  'core.runDoctor': { projectId?: string; detailLevel?: 0 | 1 }
  'core.exportDoctorLogs': { destinationPath: string }
  'core.createDatabaseBackup': { destinationPath: string }
  'core.prepareDatabaseRestore': { backupPath: string }
  'core.restoreDatabase': { backupPath: string; approvalId: string }
  'core.cancelDatabaseOperation': { operationId: string }
  'core.getModelConfig': Record<string, never>
  'core.listModelCatalog': { mode: ModelTier }
  'core.selectModel': { model: string }
  'core.getReceiptKeyStatus': Record<string, never>
  'core.trustReceiptGenesis': { genesisKeyId: string; approvalId?: string; source?: string }
  'core.rotateReceiptKey': { reason: 'manual' | 'compromise'; approvalId?: string }
  'core.createNewReceiptGenesis': { approvalId?: string; source?: string }
  /** Pending-confirmation queue and per-state counters; metadata only. */
  'core.listMemoryPending': { scopeKind: string; projectId: string; secondaryId?: string; limit?: number; workspacePath?: string }
  'core.getMemoryConflicts': { scopeKind: string; projectId: string; secondaryId?: string; limit?: number; workspacePath?: string }
  /** The only path that can return a statement, and only when not sensitive. */
  'core.getMemory': { id: string }
  'core.confirmMemory': { ids: readonly string[]; approvalId: string; idempotencyKey: string }
  'core.rejectMemory': { ids: readonly string[]; approvalId: string; idempotencyKey: string }
  'core.supersedeMemory': {
    oldId: string
    newId: string
    reason: 'user_choice' | 'revalidated' | 'expired' | 'corrected'
    approvalId: string
    idempotencyKey: string
  }
  /**
   * Edits a pending candidate ("изменить") or keeps it only for the current
   * session ("только на эту сессию"). Neither confirms the record.
   */
  'core.reviseMemoryCandidate': {
    id: string
    statement: string
    sessionOnly: boolean
    sessionId?: string
    approvalId: string
    idempotencyKey: string
  }
  'identity.get': Record<string, never>
  'repository.get': { workspacePath: string }
  'chat.list': { workspacePath: string }
  'chat.create': { workspacePath: string }
  'chat.open': { chatId: string }
  'chat.appendPrompt': { chatId: string; taskId: string; prompt: string }
  'chat.remove': { chatId: string }
  /** `directory` — папка, открытая в диалоге; пустая строка = выбор системы. */
  'review.pickPlan': { directory?: string }
  /** `sourcePaths` — пути проверяемых файлов: по ним ядро читает соседние планы, на которые они ссылаются. */
  'review.start': { reviewId: string; fileName: string; fileNames: readonly string[]; sourceMarkdown: string; reviewerModels: readonly string[]; synthesisModel: string; sourcePaths: readonly string[] }
  'review.stop': { reviewId: string }
  'review.list': { limit?: number }
  'review.get': { reviewId: string }
  'review.export': { reviewId: string; destinationPath: string; includeReviewers?: boolean }
  'review.clearHistory': Record<string, never>
  /** `sourcePath` — путь исходного файла: по нему ядро находит соседние планы. Пустой означает «путь неизвестен». */
  'review.revise': { revisionId: string; reviewId: string; fileName: string; sourceMarkdown: string; model: string; sourcePath: string }
  'review.stopRevision': { revisionId: string }
  /** Пустой `destinationPath` означает «спроси путь диалогом сохранения». */
  'review.saveRevision': { revisionId: string; destinationPath: string; fileName?: string }
  'provider.get': Record<string, never>
  'provider.save': { provider: ProviderKind; apiKey: string; model: string; baseUrl: string; tier: ModelTier }
  'provider.clearKey': Record<string, never>
  'core.createProject': { projectId: string; title: string; workspacePath: string; sourceRef?: string }
  'core.prepareBuild': { projectId: string; proposalJson: string }
  'core.applyApprovedBuild': { projectId: string; runId: string; taskId: string; approvedBuildJson: string }
  'core.terminalExecute': { taskId: string; workspacePath: string; program: string; args: readonly string[]; cwd?: string; timeoutMs?: number; approvalId?: string }
  'update.getStatus': Record<string, never>
  'update.check': Record<string, never>
  'update.prepare': Record<string, never>
  'update.restart': Record<string, never>
  'update.skip': Record<string, never>
  'listener.getRuntimeStatus': Record<string, never>
  'listener.checkRuntime': Record<string, never>
  'listener.downloadRuntime': Record<string, never>
  /**
   * `enabled=false` — выключено; `enabled=true, paused=true` — пауза;
   * `enabled=true, paused=false` — запуск или продолжение. Пустой `deviceId`
   * означает «не менять устройство».
   */
  'ambient.setListening': { enabled: boolean; paused: boolean; deviceId?: string }
  'ambient.getStatus': Record<string, never>
  'ambient.listEpisodes': { sinceMs?: number; limit?: number; cursor?: string }
  'ambient.getEpisode': { episodeId: string }
  /** `confirmed` обязателен: ядро отвергает неподтверждённое удаление. */
  'ambient.deleteTranscripts': { episodeIds?: readonly string[]; all?: boolean; confirmed: boolean }
  'ambient.forgetWindow': { windowMs: number; confirmed: boolean }
  'ambient.getPolicy': Record<string, never>
  'ambient.savePolicy': {
    /**
     * Окна тишины в форме команды: поля именуются как в остальном
     * renderer-API, а не как в снимке политики от ядра.
     */
    quietHours: readonly { startMinute: number; endMinute: number }[]
    blocklistPatterns: readonly string[]
    windowTitleBlocklist: readonly string[]
    retentionDays: number
    /**
     * Голосовые поля необязательны: вызов без них сохраняет то, что уже стоит
     * в политике. Явный `false` — это выключение, а отсутствие — «не трогать».
     */
    voiceCommands?: boolean
    voiceCommandsAutorun?: boolean
  }
  /**
   * Решение по карточке. `idempotencyKey` обязателен: принятие создаёт задачу,
   * и без ключа двойной клик породил бы две. `mute` — третий исход помимо
   * «принять» и «отклонить»: больше не предлагать такое.
   */
  'ambient.resolveProposal': {
    proposalId: string
    accepted: boolean
    idempotencyKey: string
    mute?: boolean
  }
  'ambient.listProposals': { limit?: number }
  'ambient.listVoiceCommands': { limit?: number }
  /** Решение по услышанной команде: открыть или отказаться. */
  'ambient.resolveVoiceCommand': { commandId: string; accepted: boolean }
  'ambient.hotkeyStatus': Record<string, never>
  'workflow.listTemplates': Record<string, never>
  'workflow.getDefinition': { templateId: string }
  'workflow.start': {
    templateId: string
    workspacePath: string
    inputs: Record<string, string>
    idempotencyKey: string
    taskId?: string
  }
  'workflow.getRun': { runId: string }
  'workflow.cancel': { runId: string }
  'workflow.listEvents': { runId: string; afterSequence?: number; limit?: number }
}

export interface CommandResults {
  'shell.getState': ShellState
  'shell.requestResync': { accepted: boolean }
  'trace.export': { cancelled: boolean; path: string }
  'workspace.list': WorkspaceSelection
  /** `cancelled` when the user closed the native folder dialog. */
  'workspace.pick': { cancelled: boolean; selection: WorkspaceSelection }
  'workspace.select': WorkspaceSelection
  'workspace.forget': WorkspaceSelection
  'core.startTask': { accepted: boolean }
  'core.stopTask': { accepted: boolean }
  'core.resolveApproval': { accepted: boolean }
  'core.resolveRoutingDecision': { accepted: boolean }
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
  'core.cancelDatabaseOperation': { accepted: boolean }
  'core.getModelConfig': { accepted: boolean }
  'core.listModelCatalog': { accepted: boolean }
  'core.selectModel': { accepted: boolean }
  'core.getReceiptKeyStatus': { accepted: boolean }
  'core.trustReceiptGenesis': { accepted: boolean }
  'core.rotateReceiptKey': { accepted: boolean }
  'core.createNewReceiptGenesis': { accepted: boolean }
  'core.listMemoryPending': { accepted: boolean }
  'core.getMemoryConflicts': { accepted: boolean }
  'core.getMemory': { accepted: boolean }
  'core.confirmMemory': { accepted: boolean }
  'core.rejectMemory': { accepted: boolean }
  'core.supersedeMemory': { accepted: boolean }
  'core.reviseMemoryCandidate': { accepted: boolean }
  /**
   * План 01.5: команды контекста отвечают отдельным Core-событием, поэтому
   * здесь возвращается только факт постановки команды в очередь.
   */
  'core.getContextLedger': { accepted: boolean }
  'core.listTaskScratchpad': { accepted: boolean }
  'core.clearTaskScratchpad': { accepted: boolean }
  'core.summarizeContextNow': { accepted: boolean }
  'core.pinContextItem': { accepted: boolean }
  'core.readContextArtifact': { accepted: boolean }
  'core.indexWorkspace': { accepted: boolean }
  'core.rebuildIndex': { accepted: boolean }
  'core.searchWorkspaceKnowledge': { accepted: boolean }
  'core.getIndexStatus': { accepted: boolean }
  'core.cancelWorkspaceIndex': { accepted: boolean }
  /** Responds via the `receipts.listed`/`receipts.verified`/`receipts.exported` core-event; this is only the queue acknowledgement. */
  'core.listReceipts': { accepted: boolean }
  'core.verifyReceipts': { accepted: boolean }
  'core.exportReceipts': { accepted: boolean }
  'identity.get': UserIdentity
  'repository.get': RepositorySummary | null
  'chat.list': readonly ChatSummary[]
  'chat.create': ChatRecord
  'chat.open': ChatRecord | null
  'chat.appendPrompt': ChatRecord | null
  'chat.remove': readonly ChatSummary[]
  /** `directory` — папка выбранного файла, чтобы следующий диалог открылся в ней. */
  'review.pickPlan': { cancelled: boolean; files: readonly PlanFile[]; directory: string }
  'review.start': { accepted: boolean; reviewId: string }
  'review.stop': { accepted: boolean }
  'review.list': { reviews: readonly PlanReviewResult[] }
  'review.get': { review: PlanReviewResult | null }
  'review.export': { reviewId: string; destinationPath: string }
  'review.clearHistory': { cleared: boolean }
  /** Результат приходит событием `task.completed` с `task_id = revisionId`. */
  'review.revise': { accepted: boolean; revisionId: string }
  'review.stopRevision': { accepted: boolean }
  /** `cancelled` приходит вместо `accepted`, когда закрыт диалог сохранения. */
  'review.saveRevision': { accepted?: boolean; cancelled?: boolean }
  'provider.get': ProviderSummary
  /** `restarted` is false when Core could not be relaunched with the new key. */
  'provider.save': { summary: ProviderSummary; restarted: boolean }
  'provider.clearKey': { summary: ProviderSummary; restarted: boolean }
  'core.createProject': { accepted: boolean }
  'core.prepareBuild': { accepted: boolean }
  'core.applyApprovedBuild': { accepted: boolean }
  'core.terminalExecute': { accepted: boolean }
  'update.getStatus': UpdateStatus
  'update.check': UpdateStatus
  'update.prepare': UpdateStatus
  /** `false` when nothing is staged, so the shell keeps running. */
  'update.restart': { accepted: boolean }
  /** Releases the launch gate without applying anything. */
  'update.skip': UpdateStatus
  'listener.getRuntimeStatus': ListenerRuntimeStatus
  'listener.checkRuntime': ListenerRuntimeStatus
  /** Долгая загрузка: статус приходит и событиями по мере прогресса. */
  'listener.downloadRuntime': ListenerRuntimeStatus
  /**
   * Ambient-команды отвечают отдельным Core-событием (`ambient.listening`,
   * `ambient.status`, `ambient.episodes`, `ambient.episode`,
   * `ambient.deleted`, `ambient.forgotten`, `ambient.policy`,
   * `ambient.policy_saved`, `ambient.proposal_resolved`, `ambient.proposals`);
   * здесь — только факт
   * постановки команды в очередь.
   */
  'ambient.setListening': { accepted: boolean }
  'ambient.getStatus': { accepted: boolean }
  'ambient.listEpisodes': { accepted: boolean }
  'ambient.getEpisode': { accepted: boolean }
  'ambient.deleteTranscripts': { accepted: boolean }
  'ambient.forgetWindow': { accepted: boolean }
  'ambient.getPolicy': { accepted: boolean }
  'ambient.savePolicy': { accepted: boolean }
  'ambient.resolveProposal': { accepted: boolean }
  'ambient.listProposals': { accepted: boolean }
  'ambient.listVoiceCommands': { accepted: boolean }
  'ambient.resolveVoiceCommand': { accepted: boolean }
  'ambient.hotkeyStatus': AmbientHotkeyStatus
  'workflow.listTemplates': { accepted: boolean }
  'workflow.getDefinition': { accepted: boolean }
  'workflow.start': { accepted: boolean }
  'workflow.getRun': { accepted: boolean }
  'workflow.cancel': { accepted: boolean }
  'workflow.listEvents': { accepted: boolean }
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
  /**
   * Путь файла, брошенного в окно. Нужен ровно затем, чтобы следующий диалог
   * открылся в той же папке: у объекта `File` в renderer пути нет. Возвращает
   * пустую строку, если файл пришёл не из файловой системы.
   */
  pathForFile(file: File): string
}

declare global {
  interface Window {
    readonly evohime?: { readonly v1: EvoHimeApiV1 }
  }
}
