import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function IncrementalChangeProtocolPanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly { readonly eventType: string; readonly payload: string }[] }): React.JSX.Element {
  const api = useShellApi()
  const [runId, setRunId] = useState('')
  const [operation, setOperation] = useState<'create' | 'apply' | 'cancel' | 'unknown'>('create')
  const [payload, setPayload] = useState('')
  const [fingerprint, setFingerprint] = useState('')
  const [projection, setProjection] = useState<Record<string, unknown> | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => { const event = events.find((item) => item.eventType === 'incremental_change_protocol.result'); if (!event) return; try { setProjection(JSON.parse(event.payload) as Record<string, unknown>) } catch { setMessage('Core вернул некорректную bounded projection.') } }, [events])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected' || !runId.trim()) { setMessage('Нужны подключение к Core и идентификатор run.'); return }
    const result = await api.invoke('core.incrementalChangeProtocol', { operation, runId: runId.trim(), payload, observedFingerprint: fingerprint, expectedVersion: Number(projection?.['version'] ?? 0), idempotencyKey: crypto.randomUUID() })
    if (!result.ok) setMessage(result.message)
  }
  return <section aria-label="Incremental Change Protocol" className="incremental-change-panel"><h3>Incremental Change Protocol</h3><p>Core связывает requirement delta, baseline, plan и checkpoint. Renderer показывает только metadata projection.</p><label>Run ID <input value={runId} onChange={(event) => setRunId(event.target.value)} maxLength={128} /></label><label>Операция <select value={operation} onChange={(event) => setOperation(event.target.value as typeof operation)}><option value="create">Создать</option><option value="apply">Применить</option><option value="cancel">Отменить</option><option value="unknown">Пометить unknown</option></select></label>{operation === 'create' ? <label>Bounded JSON (delta/impact/plan)<textarea value={payload} onChange={(event) => setPayload(event.target.value)} maxLength={65536} /></label> : null}<label>Observed fingerprint <input value={fingerprint} onChange={(event) => setFingerprint(event.target.value)} maxLength={512} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button>{projection ? <pre>{JSON.stringify(projection, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
