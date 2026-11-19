import { useCallback, useEffect, useMemo, useState } from 'react'

import { useShellApi } from './shell-api'
import { translate } from './i18n'
import type { AnalysisKernelProjection, ConnectionState, CoreEvent } from '@shared/api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly taskId?: string | null
}

function parseListPayload(raw: string | undefined): {
  readonly taskId: string
  readonly kernels: readonly AnalysisKernelProjection[]
  readonly errorCode: string
} | null {
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw) as {
      taskId?: unknown
      kernels?: unknown
      errorCode?: unknown
    }
    if (!Array.isArray(parsed.kernels)) return null
    const kernels: AnalysisKernelProjection[] = []
    for (const item of parsed.kernels) {
      if (!item || typeof item !== 'object') continue
      const record = item as Record<string, unknown>
      if (typeof record.kernelId !== 'string' || record.kernelId.length === 0) continue
      kernels.push({
        schemaVersion: typeof record.schemaVersion === 'number' ? record.schemaVersion : 1,
        kernelId: record.kernelId,
        taskId: typeof record.taskId === 'string' ? record.taskId : '',
        workspaceId: typeof record.workspaceId === 'string' ? record.workspaceId : '',
        runtimeVersion: typeof record.runtimeVersion === 'string' ? record.runtimeVersion : 'trusted-local-1',
        packageManifestHash: typeof record.packageManifestHash === 'string' ? record.packageManifestHash : '',
        policyHash: typeof record.policyHash === 'string' ? record.policyHash : '',
        status: typeof record.status === 'string' ? record.status : 'unknown',
        revision: typeof record.revision === 'number' ? record.revision : 0,
        limitsJson: typeof record.limitsJson === 'string' ? record.limitsJson : '',
        objectCount: typeof record.objectCount === 'number' ? record.objectCount : 0,
        truncated: record.truncated === true,
        errorCode: typeof record.errorCode === 'string' ? record.errorCode : ''
      })
    }
    return {
      taskId: typeof parsed.taskId === 'string' ? parsed.taskId : '',
      kernels,
      errorCode: typeof parsed.errorCode === 'string' ? parsed.errorCode : ''
    }
  } catch {
    return null
  }
}

/** Metadata-only diagnostics for Core-owned analysis kernels created by agent tasks. */
export function AnalysisKernelPanel({ connection, events, taskId }: Props): React.JSX.Element {
  const api = useShellApi()
  const [refreshing, setRefreshing] = useState(false)

  const effectiveTaskId = useMemo(() => {
    if (taskId && taskId.trim().length > 0) return taskId.trim()
    for (let i = events.length - 1; i >= 0; i -= 1) {
      const event = events[i]
      if (event?.taskId && event.taskId.trim().length > 0) return event.taskId.trim()
    }
    return ''
  }, [events, taskId])

  const refresh = useCallback(async () => {
    if (!api || !effectiveTaskId || connection !== 'connected') return
    setRefreshing(true)
    try {
      await api.invoke('core.listAnalysisKernels', { taskId: effectiveTaskId, maxItems: 32 })
    } finally {
      setRefreshing(false)
    }
  }, [api, connection, effectiveTaskId])

  useEffect(() => {
    void refresh()
  }, [refresh])

  const { projections, errorCode } = useMemo(() => {
    const byId = new Map<string, AnalysisKernelProjection>()
    let lastError = ''

    for (const event of events) {
      if (event.eventType === 'analysis_kernel.list') {
        const parsed = parseListPayload(event.payload)
        if (parsed && (!effectiveTaskId || parsed.taskId === effectiveTaskId)) {
          lastError = parsed.errorCode
          for (const kernel of parsed.kernels) {
            byId.set(kernel.kernelId, kernel)
          }
        }
      }
      if (event.analysisKernel) {
        const kernel = event.analysisKernel
        if (!effectiveTaskId || kernel.taskId === effectiveTaskId) {
          byId.set(kernel.kernelId, kernel)
        }
      }
    }

    const sorted = Array.from(byId.values()).sort((left, right) => {
      if (left.taskId !== right.taskId) return left.taskId.localeCompare(right.taskId)
      return left.kernelId.localeCompare(right.kernelId)
    })

    return { projections: sorted.slice(0, 32), errorCode: lastError }
  }, [effectiveTaskId, events])

  return (
    <section className="panel analysis-kernel-panel">
      <div className="panel__header">
        <div>
          <span className="panel__eyebrow">Persistent Analysis Kernel v1</span>
          <h3>{translate("Аналитические ядра")}</h3>
        </div>
        <div className="panel__actions">
          {effectiveTaskId ? (
            <button type="button" onClick={() => void refresh()} disabled={refreshing || connection !== 'connected'}>
              {translate("Обновить")}
            </button>
          ) : null}
          <span className={`status-pill status-pill--${connection}`}>{translate(connection)}</span>
        </div>
      </div>
      <p className="panel__muted">
        {translate("Диагностика аналитических ядер, созданных агентом во время задачи. Значения объектов и пути в UI не передаются.")}
      </p>
      {errorCode ? <div className="error-text">{translate(errorCode)}</div> : null}
      {projections.length === 0 ? (
        <div className="empty-state">
          {translate("Для задачи пока нет аналитических ядер. Ядро создаётся автоматически при первом аналитическом вызове агента.")}
        </div>
      ) : (
        <div className="stack-list">
          {projections.map((kernel) => (
            <article className="stack-list__item" key={kernel.kernelId}>
              <div>
                <strong>{kernel.kernelId}</strong>
                <span className="panel__muted"> · {translate(kernel.status)}</span>
              </div>
              <div className="panel__muted">
                {translate("Задача:")} {kernel.taskId} · {translate("объектов:")} {kernel.objectCount} · revision: {kernel.revision}
              </div>
              <div className="panel__muted">Runtime: {kernel.runtimeVersion}</div>
              {kernel.errorCode ? <div className="error-text">{translate(kernel.errorCode)}</div> : null}
            </article>
          ))}
        </div>
      )}
    </section>
  )
}
