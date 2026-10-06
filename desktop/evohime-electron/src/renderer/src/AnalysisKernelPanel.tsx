import { translate } from './i18n'
import { useMemo } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

/** Metadata-only diagnostics for Core-owned analysis kernels. */
export function AnalysisKernelPanel({ connection, events }: Props): React.JSX.Element {
  const projections = useMemo(() => events
    .filter((event) => event.analysisKernel)
    .map((event) => event.analysisKernel)
    .filter((value): value is NonNullable<typeof value> => value !== null && value !== undefined)
    .filter((value, index, all) => all.findIndex((candidate) => candidate.kernelId === value.kernelId) === index)
    .slice(0, 32), [events])

  return <section className="panel analysis-kernel-panel">
    <div className="panel__header">
      <div><span className="panel__eyebrow">Persistent Analysis Kernel v1</span><h3>{translate("Аналитические ядра")}</h3></div>
      <span className={`status-pill status-pill--${connection}`}>{translate(connection)}</span>
    </div>
    <p className="panel__muted">{translate("Core показывает только состояние, лимиты и метаданные. Память процесса и значения объектов в UI не передаются.")}</p>
    {projections.length === 0 ? <div className="empty-state">{translate("Нет созданных аналитических ядер.")}</div> : <div className="stack-list">
      {projections.map((kernel) => <article className="stack-list__item" key={kernel.kernelId}>
        <div><strong>{translate(kernel.kernelId)}</strong><span className="panel__muted"> · {translate(kernel.status)}</span></div>
        <div className="panel__muted">{translate("Задача:")}{translate(kernel.taskId)} {translate("· объектов:")}{translate(kernel.objectCount)} · revision: {translate(kernel.revision)}</div>
        <div className="panel__muted">Runtime: {translate(kernel.runtimeVersion)}</div>
        {kernel.errorCode ? <div className="error-text">{translate(kernel.errorCode)}</div> : null}
      </article>)}
    </div>}
  </section>
}
