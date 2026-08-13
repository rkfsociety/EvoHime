import { useCallback, useEffect, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

interface SafetyPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

type PermissionMode = 'ask' | 'read_only' | 'full'

export function SafetyPanel({ connection, events }: SafetyPanelProps): React.JSX.Element {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [mode, setMode] = useState<PermissionMode>('ask')
  const [doctor, setDoctor] = useState<Record<string, unknown> | null>(null)
  const [progress, setProgress] = useState<Record<string, unknown> | null>(null)
  const [backupPath, setBackupPath] = useState('')
  const [restorePath, setRestorePath] = useState('')
  const [restoreApproval, setRestoreApproval] = useState('')
  const [message, setMessage] = useState<string | null>(null)
  const [cancelling, setCancelling] = useState(false)

  useEffect(() => {
    const report = latestEvent(events, 'doctor.report')
    if (report) setDoctor(parseJson(report.payload))
    const progressEvent = latestEvent(events, 'storage.progress')
    if (progressEvent) {
      const nextProgress = parseJson(progressEvent.payload)
      setProgress(nextProgress)
      const progressMessage = stringField(nextProgress, 'message')
      if (progressMessage === 'backup completed') {
        setCancelling(false)
        setMessage('Backup создан и проверен Core.')
      } else if (progressMessage === 'restore completed') {
        setCancelling(false)
        setMessage('Восстановление завершено. Перезапусти shell, если Core запросит resync.')
      }
    }
    const preview = latestEvent(events, 'storage.restore.preview')
    if (preview) {
      const payload = parseJson(preview.payload)
      const approvalId = stringField(payload, 'approval_id') ?? stringField(payload, 'approvalId')
      if (approvalId) setRestoreApproval(approvalId)
      setMessage('Backup проверен Core. Для восстановления требуется approval token.')
    }
    const completed = latestEvent(events, 'storage.backup.created')
    if (completed) {
      setCancelling(false)
      setMessage('Backup создан и проверен Core.')
    }
    const restored = latestEvent(events, 'storage.restore.completed')
    if (restored) {
      setCancelling(false)
      setMessage('Восстановление завершено. Перезапусти shell, если Core запросит resync.')
    }
    const exported = latestEvent(events, 'doctor.export.completed')
    if (exported) setMessage('Диагностика экспортирована.')
  }, [events])

  const send = useCallback(
    async (action: () => Promise<{ ok: true; value: { accepted: boolean } } | { ok: false; message: string }>) => {
      if (!connected) return
      setMessage(null)
      const outcome = await action()
      if (!outcome.ok) setMessage(outcome.message)
    },
    [connected]
  )

  const changeMode = useCallback(
    async (next: PermissionMode) => {
      if (!api) return
      await send(() => api.invoke('core.setPermissionMode', { mode: next }))
      setMode(next)
    },
    [api, send]
  )

  const runDoctor = useCallback(async () => {
    if (!api) return
    await send(() => api.invoke('core.runDoctor', { detailLevel: 1 }))
  }, [api, send])

  const createBackup = useCallback(async () => {
    if (!api || backupPath.trim().length === 0) return
    await send(() => api.invoke('core.createDatabaseBackup', {
      destinationPath: backupPath.trim()
    }))
  }, [api, backupPath, send])

  const prepareRestore = useCallback(async () => {
    if (!api || restorePath.trim().length === 0) return
    setRestoreApproval('')
    await send(() => api.invoke('core.prepareDatabaseRestore', {
      backupPath: restorePath.trim()
    }))
  }, [api, restorePath, send])

  const restore = useCallback(async () => {
    if (!api || restorePath.trim().length === 0 || restoreApproval.trim().length === 0) return
    await send(() => api.invoke('core.restoreDatabase', {
      backupPath: restorePath.trim(),
      approvalId: restoreApproval.trim()
    }))
  }, [api, restoreApproval, restorePath, send])

  const cancelOperation = useCallback(async () => {
    if (!api || !latestProgressOperation(events)) return
    setCancelling(true)
    const outcome = await api.invoke('core.cancelDatabaseOperation', {
      operationId: latestProgressOperation(events)!
    })
    if (!outcome.ok) {
      setCancelling(false)
      setMessage(outcome.message)
    } else if (!outcome.value.accepted) {
      setCancelling(false)
      setMessage('Core уже завершил или не нашёл эту операцию.')
    } else {
      setMessage('Core получил запрос на отмену. Изменения не будут применены.')
    }
  }, [api, events])

  const exportLogs = useCallback(async () => {
    if (!api) return
    const destinationPath = window.prompt('Путь JSONL для redacted diagnostics')
    if (!destinationPath) return
    await send(() => api.invoke('core.exportDoctorLogs', { destinationPath }))
  }, [api, send])

  return (
    <section className="shell__panel safety-panel" aria-label="Политика и диагностика">
      <div className="safety-panel__heading">
        <div>
          <h2>Политика и диагностика</h2>
          <p className="shell__empty">Решения и операции выполняет Core; shell показывает их состояние.</p>
        </div>
        <button type="button" onClick={() => void runDoctor()} disabled={!connected}>Запустить Doctor</button>
      </div>

      <div className="safety-panel__modes">
        <strong>Режим разрешений: {modeLabel(mode)}</strong>
        {(['ask', 'read_only', 'full'] as const).map((value) => (
          <button key={value} type="button" onClick={() => void changeMode(value)} disabled={!connected || mode === value}>
            {modeLabel(value)}
          </button>
        ))}
      </div>

      {!connected ? <p className="shell__reason">Core недоступен: операции приостановлены.</p> : null}
      {message ? <p role="alert" className="shell__reason">{message}</p> : null}

      <div className="safety-panel__doctor">
        <h3>Doctor</h3>
        {doctor ? <pre>{formatDoctor(doctor)}</pre> : <p className="shell__empty">Диагностика ещё не запускалась.</p>}
        <button type="button" onClick={() => void exportLogs()} disabled={!connected}>Экспортировать redacted logs</button>
      </div>

      <div className="safety-panel__backup">
        <h3>Backup и restore</h3>
        <label>Новый backup (.evohime-backup)<input value={backupPath} onChange={(event) => setBackupPath(event.target.value)} /></label>
        <button type="button" onClick={() => void createBackup()} disabled={!connected || backupPath.trim().length === 0}>Создать backup</button>
        <label>Файл backup для проверки и restore<input value={restorePath} onChange={(event) => setRestorePath(event.target.value)} /></label>
        <div className="safety-panel__backup-actions">
          <button type="button" onClick={() => void prepareRestore()} disabled={!connected || restorePath.trim().length === 0}>Проверить backup</button>
          <input aria-label="Approval token restore" placeholder="Approval token" value={restoreApproval} onChange={(event) => setRestoreApproval(event.target.value)} />
          <button type="button" onClick={() => void restore()} disabled={!connected || restoreApproval.trim().length === 0}>Восстановить</button>
        </div>
        {progress ? <p className="safety-panel__progress">{String(progress.phase ?? 'операция')} · {String(progress.completed ?? 0)} / {String(progress.total ?? '?')} · {String(progress.message ?? '')}</p> : null}
        {progress && isCancellablePhase(progress.phase) ? <button type="button" onClick={() => void cancelOperation()} disabled={!connected || cancelling}>{cancelling ? 'Отмена запрошена…' : 'Отменить операцию'}</button> : null}
      </div>
    </section>
  )
}

function latestEvent(events: readonly CoreEvent[], eventType: string): CoreEvent | null {
  return [...events].reverse().find((event) => event.eventType === eventType) ?? null
}

function parseJson(payload: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
  } catch {
    return {}
  }
}

function stringField(payload: Record<string, unknown>, key: string): string | null {
  const value = payload[key]
  return typeof value === 'string' && value.length > 0 ? value : null
}

function latestProgressOperation(events: readonly CoreEvent[]): string | null {
  return latestEvent(events, 'storage.progress')?.taskId ?? null
}

function isCancellablePhase(phase: unknown): boolean {
  return phase === 'prepare' || phase === 'backup' || phase === 'validate' || phase === 'restore'
}

function formatDoctor(doctor: Record<string, unknown>): string {
  const checks = Array.isArray(doctor.checks) ? doctor.checks : []
  return checks.map((check) => {
    if (typeof check !== 'object' || check === null) return String(check)
    const value = check as Record<string, unknown>
    return `${String(value.id ?? 'check')}: ${String(value.status ?? 'unknown')} — ${String(value.summary ?? '')}`
  }).join('\n') || 'Core не вернул проверки.'
}

function modeLabel(mode: PermissionMode): string {
  return mode === 'ask' ? 'Спрашивать' : mode === 'read_only' ? 'Только чтение' : 'Полный доступ'
}
