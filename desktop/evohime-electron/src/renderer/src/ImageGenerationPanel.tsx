import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'
import { useShellApi } from './shell-api'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

interface Capability {
  readonly state?: string
  readonly reason_code?: string
  readonly operations?: readonly string[]
  readonly mime_types?: readonly string[]
  readonly max_dimension?: number
  readonly decoder_available?: boolean
}

function latestImageProjection(events: readonly CoreEvent[]): Record<string, unknown> | null {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index]
    if (event?.eventType !== 'image_generation.result') continue
    try {
      const parsed: unknown = JSON.parse(event.imageGeneration ?? event.payload)
      if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) return parsed as Record<string, unknown>
    } catch {
      return { status: 'failed', error_code: 'invalid_projection' }
    }
  }
  return null
}

function latestCapabilityProjection(events: readonly CoreEvent[]): Capability | null {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index]
    if (event?.eventType !== 'image_generation.result') continue
    try {
      const parsed: unknown = JSON.parse(event.imageGeneration ?? event.payload)
      if (typeof parsed === 'object' && parsed !== null && 'contract_version' in parsed) return parsed as Capability
    } catch { /* Invalid event projections are ignored; Core owns the current state. */ }
  }
  return null
}

export function ImageGenerationPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [prompt, setPrompt] = useState('')
  const [operation, setOperation] = useState<'generate' | 'edit'>('generate')
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)
  const [jobId, setJobId] = useState('')
  const connected = connection === 'connected' || connection === 'replaying' || connection === 'resyncing'
  const projection = useMemo(() => latestImageProjection(events), [events])
  const capability = useMemo(() => latestCapabilityProjection(events), [events])
  const available = capability?.state === 'supported' && capability.decoder_available !== false && capability.operations?.includes('generate') === true

  useEffect(() => {
    if (connected && api) void api.invoke('imageGeneration.capability', {})
  }, [api, connected])

  useEffect(() => {
    if (!api || !connected || !jobId || ['completed', 'failed', 'cancelled', 'unknown_outcome', 'not_found', 'rejected', 'unavailable'].includes(String(projection?.['state'] ?? projection?.['status'] ?? ''))) return
    const poll = (): void => { void api.invoke('imageGeneration.get', { jobId }) }
    poll()
    const timer = window.setInterval(poll, 1500)
    return () => window.clearInterval(timer)
  }, [api, connected, jobId, projection?.['state']])

  const generate = async (): Promise<void> => {
    if (!api || !available || prompt.trim().length === 0) return
    setBusy(true)
    setMessage('Запрос передан Core…')
    try {
      const requestedJobId = crypto.randomUUID()
      const inputMime = editableArtifact?.['mime_type']
      const outcome = await api.invoke('imageGeneration.start', {
        operation, prompt, width: 1024, height: 1024, count: 1,
        mimeType: 'image/png',
        ...(operation === 'edit' && editableArtifact && (inputMime === 'image/png' || inputMime === 'image/jpeg') ? {
          inputImages: [{ locator: String(editableArtifact['locator']), mimeType: inputMime, artifactKind: 'generated_image' }]
        } : {}),
        idempotencyKey: requestedJobId, jobId: requestedJobId
      })
      if (outcome.ok) { setJobId(requestedJobId); setMessage('Core принял запрос.') }
      else setMessage(outcome.message)
    } finally {
      setBusy(false)
    }
  }

  const result = typeof projection?.['result_json'] === 'object' && projection['result_json'] !== null ? projection['result_json'] as Record<string, unknown> : null
  const artifacts = Array.isArray(result?.['artifacts']) ? result['artifacts'] as readonly Record<string, unknown>[] : []
  const editableArtifact = [...artifacts].reverse().find((artifact) => typeof artifact['locator'] === 'string' && (artifact['mime_type'] === 'image/png' || artifact['mime_type'] === 'image/jpeg'))
  const jobState = typeof projection?.['state'] === 'string' ? projection['state'] : typeof projection?.['status'] === 'string' ? projection['status'] : ''
  const cancellableJob = Boolean(jobId) && ['preflight', 'queued'].includes(jobState)
  return (
    <article className="operations-card">
      <div className="operations-card__topline"><span className="operations-card__eyebrow">Core image capability</span><span className="operations-card__phase">{capability?.state ?? 'проверка'}</span></div>
      <h3>Генерация изображений</h3>
      <small>{available ? `Операции: ${capability?.operations?.join(', ')} · ${capability.mime_types?.join(', ')}` : capability?.reason_code ?? 'Состояние capability ещё не получено.'}</small>
      <label>
        <span>Описание изображения</span>
        <textarea value={prompt} maxLength={8192} rows={3} onChange={(event) => setPrompt(event.target.value)} disabled={!available || busy} />
      </label>
      <div className="operations-card__actions">
        <button type="button" aria-pressed={operation === 'generate'} disabled={!available || busy} onClick={() => setOperation('generate')}>Создать</button>
        <button type="button" aria-pressed={operation === 'edit'} disabled={!available || !capability?.operations?.includes('edit') || !editableArtifact || busy} onClick={() => setOperation('edit')}>Изменить последний результат</button>
      </div>
      {operation === 'edit' && editableArtifact ? <small>Вход: {String(editableArtifact['mime_type'])}, {String(editableArtifact['width'])}×{String(editableArtifact['height'])}</small> : null}
      <button type="button" disabled={!connected || !available || busy || prompt.trim().length === 0 || (operation === 'edit' && !editableArtifact)} onClick={() => void generate()}>{busy ? 'Передача…' : operation === 'edit' ? 'Изменить изображение' : 'Создать изображение'}</button>
      {cancellableJob ? <button type="button" onClick={() => void api?.invoke('imageGeneration.cancel', { jobId })}>Отменить до dispatch</button> : null}
      {message ? <small role="status">{message}</small> : null}
      {artifacts.map((artifact, index) => (
        <small key={`${String(artifact['content_hash'])}-${index}`}>Готово: {String(artifact['mime_type'])}, {String(artifact['width'])}×{String(artifact['height'])}, SHA-256 {String(artifact['sha256']).slice(0, 16)}…</small>
      ))}
      {typeof projection?.['error_code'] === 'string' && projection['error_code'] ? <small className="operations-card__error">{projection['error_code']}</small> : null}
    </article>
  )
}
