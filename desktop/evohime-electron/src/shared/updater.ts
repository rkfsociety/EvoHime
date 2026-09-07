import type { UpdateStatus } from './update'

export type UpdaterUiPhase = 'checking' | 'ready' | 'available' | 'applying' | 'failed'

export interface UpdaterUiModule {
  readonly id: string
  readonly label: string
  readonly installed: string
  readonly available: string | null
  readonly summary: string
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
}

export interface EvoHimeUpdaterApi {
  getStatus(): Promise<UpdaterUiStatus>
  subscribe(listener: (status: UpdaterUiStatus) => void): () => void
  apply(): Promise<void>
  launch(): Promise<void>
  close(): Promise<void>
  minimize(): Promise<void>
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
  const phase = status.error ? 'failed' : toUiPhase(status.phase)
  const available = status.availableModules ?? []
  const installed = status.installedModules ?? {}
  const versions = status.availableModuleVersions ?? {}
  const summaries = status.availableModuleSummaries ?? {}
  const ids = available.length > 0 ? available : DEFAULT_MODULES
  const modules = ids.map((id) => ({
    id,
    label: MODULE_LABELS[id] ?? id,
    installed: installed[id] ?? 'установлено',
    available: versions[id] ?? null,
    summary: summaries[id] ?? (available.length > 0 ? 'Подготовлено к безопасному обновлению.' : 'Работает в установленной версии.')
  }))

  return {
    phase,
    heading: phaseHeading(phase),
    badge: phaseBadge(phase),
    message: status.error ?? status.message,
    detail: status.detail,
    percent: phase === 'applying' ? parsePercent(status.message) : null,
    modules,
    canApply: phase === 'available' && available.length > 0
  }
}

function toUiPhase(phase: UpdateStatus['phase']): UpdaterUiPhase {
  if (phase === 'available') return 'available'
  if (phase === 'applying') return 'applying'
  if (phase === 'failed') return 'failed'
  if (phase === 'checking' || phase === 'idle') return 'checking'
  return 'ready'
}

function phaseHeading(phase: UpdaterUiPhase): string {
  if (phase === 'checking') return 'Проверяю модули'
  if (phase === 'available') return 'Доступно обновление'
  if (phase === 'applying') return 'Устанавливаю обновление'
  if (phase === 'failed') return 'Проверка требует внимания'
  return 'Модули проверены'
}

function phaseBadge(phase: UpdaterUiPhase): string {
  if (phase === 'checking') return 'Проверка'
  if (phase === 'available') return 'Доступно'
  if (phase === 'applying') return 'Установка'
  if (phase === 'failed') return 'Ошибка'
  return 'Готово к запуску'
}

function parsePercent(message: string): number | null {
  const match = message.match(/(?:—|-)\s*(\d{1,3})%\s*$/)
  if (!match?.[1]) return null
  return Math.min(100, Number(match[1]))
}
