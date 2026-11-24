import type { UpdateStatus } from './update'

export type UpdaterUiPhase = 'checking' | 'ready' | 'available' | 'applying' | 'recovering' | 'manual-recovery' | 'failed'

export type UpdaterModuleState = 'pending' | 'active' | 'done' | 'error'

export interface UpdaterUiModule {
  readonly id: string
  readonly label: string
  readonly installed: string
  readonly available: string | null
  readonly summary: string
  readonly changes?: readonly string[]
  readonly state?: UpdaterModuleState
}

export interface UpdaterUiStatus {
  readonly phase: UpdaterUiPhase
  readonly heading: string
  readonly badge: string
  readonly message: string
  readonly detail: string
  readonly percent: number | null
  readonly modules: readonly UpdaterUiModule[]
  readonly canApply: boolean
  readonly recovery?: UpdateStatus['recovery']
}

export interface UpdaterWindowSize {
  readonly width: number
  readonly height: number
}

export interface EvoHimeUpdaterApi {
  getStatus(): Promise<UpdaterUiStatus>
  subscribe(listener: (status: UpdaterUiStatus) => void): () => void
  close(): Promise<void>
  minimize(): Promise<void>
  hideToTray?(): Promise<void>
  retry?(): Promise<void>
  resize?(size: UpdaterWindowSize): Promise<void>
}

/** True when a worker status message describes an artifact download, not a file swap. */
export function isDownloadPhaseMessage(message: string): boolean {
  return /скачив|загруз/i.test(message)
}

/** True when a worker status message describes installing or unpacking files. */
export function isInstallPhaseMessage(message: string): boolean {
  return /установ|примен|распаков|обновля|перезапуск/i.test(message)
}

const MODULE_LABELS: Readonly<Record<string, string>> = {
  'shell-host': 'Оболочка Electron',
  'ui-bundle': 'Интерфейс EvoHime',
  core: 'EvoHime Core',
  supervisor: 'Supervisor',
  cli: 'Командный клиент',
  'analysis-worker': 'Аналитический модуль',
  listener: 'Голосовой модуль',
  'listener-runtime': 'Распознавание речи',
  transaction: 'Транзакционный worker',
  updater: 'Updater worker',
  verifier: 'Проверка пакета'
}

const DEFAULT_MODULES = ['core', 'shell-host', 'supervisor', 'listener']

export function updaterUiStatus(status: UpdateStatus): UpdaterUiStatus {
  const phase = status.error ? 'failed' : toUiPhase(status.phase, status.recovery)
  const available = status.availableModules ?? []
  const installed = status.installedModules ?? {}
  const versions = status.availableModuleVersions ?? {}
  const summaries = status.availableModuleSummaries ?? {}
  const changesMap = status.availableModuleChanges ?? {}
  const ids = available.length > 0 ? available : DEFAULT_MODULES
  const modules = ids.map((id) => ({
    id,
    label: MODULE_LABELS[id] ?? id,
    installed: installed[id] ?? 'установлено',
    available: versions[id] ?? null,
    summary: summaries[id] ?? (available.length > 0 ? 'Подготовлено к безопасному обновлению.' : 'Работает в установленной версии.'),
    changes: changesMap[id] ?? []
  }))

  const percent = phase === 'applying'
    ? (parsePercent(status.message) ?? (status.downloadProgress !== null ? Math.min(100, Math.round(status.downloadProgress * 100)) : null))
    : null
  const rawMessage = status.error ?? status.message
  const strippedMessage = status.error || phase !== 'applying' ? rawMessage : stripPercentSuffix(rawMessage)

  // The `applying` phase covers both downloading module artifacts and swapping
  // them on disk. Headings and badges must follow the actual sub-step, same way
  // the step list already does, otherwise the panel reads "Установка" while
  // the message says "Скачивание shell-host".
  const downloading = phase === 'applying' && !isInstallPhaseMessage(rawMessage) && isDownloadPhaseMessage(rawMessage)

  const activeModuleId = status.activeModule ?? (downloading ? extractDownloadModule(strippedMessage) : null)
  const activeModuleInfo = findActiveModuleInfo(modules, activeModuleId)

  const rawFailure = phase === 'failed'
    ? formatFailureCopy(rawMessage, activeModuleId, status.detail)
    : null

  const message = downloading
    ? formatDownloadSubtitle({
        moduleLabel: activeModuleInfo?.label ?? resolveModuleLabel(activeModuleId) ?? strippedMessage,
        downloadedBytes: status.downloadedBytes ?? null,
        totalBytes: status.totalBytes ?? null,
        speedBytesPerSec: status.downloadSpeedBytesPerSec ?? null
      })
    : rawFailure ? rawFailure.message : strippedMessage

  const detail = rawFailure ? rawFailure.detail : status.detail

  const badge = downloading
    ? formatDownloadBadge({
        module: activeModuleInfo,
        downloadedBytes: status.downloadedBytes ?? null,
        totalBytes: status.totalBytes ?? null,
        speedBytesPerSec: status.downloadSpeedBytesPerSec ?? null
      })
    : phaseBadge(phase)

  const completedSet = new Set(status.completedModules ?? [])
  const modulesWithState: readonly UpdaterUiModule[] = modules.map((module) => ({
    ...module,
    state: resolveModuleState({
      id: module.id,
      available: module.available !== null,
      phase,
      downloading,
      activeModuleId,
      completedSet
    })
  }))

  return {
    phase,
    heading: downloading ? 'Скачиваю обновление' : phaseHeading(phase),
    badge,
    message,
    detail,
    percent,
    modules: modulesWithState,
    canApply: phase === 'available' && available.length > 0,
    ...(status.recovery ? { recovery: status.recovery } : {})
  }
}

function toUiPhase(phase: UpdateStatus['phase'], recovery: UpdateStatus['recovery']): UpdaterUiPhase {
  if (phase === 'available') return 'available'
  if (phase === 'applying') return 'applying'
  if (phase === 'failed') return 'failed'
  if (recovery?.phase === 'manual-recovery') return 'manual-recovery'
  if (recovery && recovery.phase !== 'committed' && recovery.phase !== 'rolled-back') return 'recovering'
  if (phase === 'checking' || phase === 'idle') return 'checking'
  return 'ready'
}

function phaseHeading(phase: UpdaterUiPhase): string {
  if (phase === 'checking') return 'Проверяю обновления'
  if (phase === 'available') return 'Готовлю обновление'
  if (phase === 'applying') return 'Устанавливаю обновление'
  if (phase === 'failed') return 'Обновление не завершено'
  if (phase === 'recovering') return 'Восстанавливаю обновление'
  if (phase === 'manual-recovery') return 'Требуется восстановление'
  return 'Обновление завершено'
}

function phaseBadge(phase: UpdaterUiPhase): string {
  if (phase === 'checking') return 'Проверка'
  if (phase === 'available') return 'Доступно'
  if (phase === 'applying') return 'Установка'
  if (phase === 'failed') return 'Ошибка'
  if (phase === 'recovering') return 'Восстановление'
  if (phase === 'manual-recovery') return 'Ручное действие'
  return 'Готово к запуску'
}

const PERCENT_SUFFIX = /\s*(?:—|-)\s*(\d{1,3})%\s*$/

export function parsePercent(message: string): number | null {
  const match = message.match(PERCENT_SUFFIX)
  if (!match?.[1]) return null
  return Math.min(100, Number(match[1]))
}

function stripPercentSuffix(message: string): string {
  return message.replace(PERCENT_SUFFIX, '').trimEnd()
}

function extractDownloadModule(message: string): string | null {
  const match = message.match(/^(?:скачивание|загрузка)\s+(.+)$/i)
  const moduleName = match?.[1]?.replace(/\.{3}|…$/g, '').trim()
  return moduleName && moduleName.length > 0 ? moduleName : null
}

function resolveModuleLabel(moduleId: string | null): string | null {
  if (!moduleId) return null
  if (MODULE_LABELS[moduleId]) return MODULE_LABELS[moduleId]
  const baseId = moduleId.split(':', 1)[0]
  return baseId ? (MODULE_LABELS[baseId] ?? moduleId) : moduleId
}

function findActiveModuleInfo(
  modules: readonly UpdaterUiModule[],
  activeModuleId: string | null
): UpdaterUiModule | null {
  if (activeModuleId) {
    const exact = modules.find((item) => item.id === activeModuleId)
    if (exact && exact.available !== null) return exact
    const prefix = modules.find((item) => activeModuleId.startsWith(`${item.id}:`) && item.available !== null)
    if (prefix) return prefix
  }
  const updatable = modules.filter((item) => item.available !== null)
  return updatable.length === 1 ? (updatable[0] ?? null) : null
}

interface DownloadTelemetryInput {
  readonly moduleLabel: string
  readonly downloadedBytes: number | null
  readonly totalBytes: number | null
  readonly speedBytesPerSec: number | null
}

function formatDownloadSubtitle(input: DownloadTelemetryInput): string {
  const parts: string[] = []
  if (input.moduleLabel.length > 0) {
    parts.push(input.moduleLabel)
  }
  const hasSpeed = input.speedBytesPerSec !== null && input.speedBytesPerSec > 0
  const hasBytes = input.downloadedBytes !== null && input.totalBytes !== null && input.totalBytes > 0
  if (hasSpeed && input.speedBytesPerSec !== null) {
    parts.push(formatSpeed(input.speedBytesPerSec))
    if (hasBytes && input.totalBytes !== null && input.downloadedBytes !== null && input.totalBytes > input.downloadedBytes) {
      parts.push(formatEta((input.totalBytes - input.downloadedBytes) / input.speedBytesPerSec))
    }
  } else if (hasBytes && input.downloadedBytes !== null && input.totalBytes !== null) {
    parts.push(formatBytePair(input.downloadedBytes, input.totalBytes))
  }
  return parts.join(' · ')
}

function formatDownloadBadge(input: {
  readonly module: UpdaterUiModule | null
  readonly downloadedBytes: number | null
  readonly totalBytes: number | null
  readonly speedBytesPerSec: number | null
}): string {
  const versionPair = input.module && input.module.available !== null && input.module.installed !== 'установлено'
    ? `${formatVersionLabel(input.module.installed)} → ${formatVersionLabel(input.module.available)}`
    : null
  const hasBytes = input.downloadedBytes !== null && input.totalBytes !== null && input.totalBytes > 0
  const hasSpeed = input.speedBytesPerSec !== null && input.speedBytesPerSec > 0
  const bytePair = hasBytes && input.downloadedBytes !== null && input.totalBytes !== null
    ? formatBytePair(input.downloadedBytes, input.totalBytes)
    : null

  if (versionPair && bytePair && hasSpeed) {
    return `${versionPair} · ${bytePair}`
  }
  if (versionPair) {
    return versionPair
  }
  if (bytePair) {
    return bytePair
  }
  return 'Обновление'
}

function formatVersionLabel(version: string): string {
  return /^\d/.test(version) ? `v${version}` : version
}

function formatMegabytes(bytes: number): string {
  return (bytes / (1024 * 1024)).toFixed(1)
}

function formatBytePair(downloadedBytes: number, totalBytes: number): string {
  if (totalBytes < 1024 * 1024) {
    const doneKb = Math.round(downloadedBytes / 1024)
    const totalKb = Math.max(1, Math.round(totalBytes / 1024))
    return `${doneKb} / ${totalKb} КБ`
  }
  return `${formatMegabytes(downloadedBytes)} / ${formatMegabytes(totalBytes)} МБ`
}

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec < 1024 * 1024) {
    const kbPerSec = Math.max(1, Math.round(bytesPerSec / 1024))
    return `${kbPerSec} КБ/с`
  }
  return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} МБ/с`
}

function formatEta(seconds: number): string {
  const rounded = Math.max(1, Math.round(seconds))
  if (rounded < 60) {
    return `осталось ~${rounded} с`
  }
  const minutes = Math.round(rounded / 60)
  return `осталось ~${minutes} мин`
}

function resolveModuleState(input: {
  readonly id: string
  readonly available: boolean
  readonly phase: UpdaterUiPhase
  readonly downloading: boolean
  readonly activeModuleId: string | null
  readonly completedSet: ReadonlySet<string>
}): UpdaterModuleState {
  if (!input.available || input.phase === 'ready') {
    return 'done'
  }
  if (input.phase === 'failed' || input.phase === 'manual-recovery') {
    if (input.activeModuleId && (input.activeModuleId === input.id || input.activeModuleId.startsWith(`${input.id}:`))) {
      return 'error'
    }
    return input.completedSet.has(input.id) ? 'done' : 'pending'
  }
  if (input.phase === 'recovering') {
    return 'done'
  }
  if (input.phase === 'applying') {
    if (!input.downloading) {
      return 'done'
    }
    if (input.completedSet.has(input.id)) {
      return 'done'
    }
    if (input.activeModuleId && (input.activeModuleId === input.id || input.activeModuleId.startsWith(`${input.id}:`))) {
      return 'active'
    }
    return 'pending'
  }
  return 'pending'
}

function formatFailureCopy(
  rawMessage: string,
  activeModuleId: string | null,
  existingDetail: string
): { readonly message: string; readonly detail: string } {
  const isNetworkOrDownload = /timeout|timed out|connection|network|dns|econn|enotfound|http|5\d\d|404|скач|загруз|github|сбой сети|ошибка сети/i.test(rawMessage)
  if (isNetworkOrDownload && activeModuleId) {
    const friendly = `Не удалось скачать ${activeModuleId}`
    const detail = existingDetail.length > 0
      ? existingDetail
      : rawMessage === friendly ? '' : rawMessage
    return { message: friendly, detail }
  }
  return { message: rawMessage, detail: existingDetail }
}
