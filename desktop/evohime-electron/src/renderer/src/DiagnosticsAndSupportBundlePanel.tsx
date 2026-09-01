import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

interface Props { readonly connection: ConnectionState; readonly events: readonly CoreEvent[] }

export function DiagnosticsAndSupportBundlePanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [notice, setNotice] = useState<string | null>(null)
  const [conversationId, setConversationId] = useState('')
  const [runId, setRunId] = useState('')
  const event = events.find((item) => item.eventType === 'diagnostics.snapshot')
  const snapshot = useMemo(() => {
    if (!event) return null
    try { return JSON.parse(event.payload) as Record<string, unknown> } catch { return null }
  }, [event])
  const connected = connection === 'connected' || connection === 'replaying' || connection === 'resyncing'

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('core.createDiagnosticsSnapshot', { maxEventCount: 200, maxLogBytes: 64 * 1024 })
  }, [api, connected])

  async function refresh(): Promise<void> {
    if (!api) return
    const result = await api.invoke('core.createDiagnosticsSnapshot', { conversationId, runId, maxEventCount: 200, maxLogBytes: 64 * 1024 })
    if (!result.ok) setNotice(result.message)
  }

  async function save(): Promise<void> {
    if (!api) return
    const result = await api.invoke('shell.exportDiagnostics', {})
    setNotice(result.ok ? (result.value.cancelled ? 'Сохранение отменено.' : `Bundle сохранён: ${result.value.path}`) : result.message)
  }

  async function copyDraft(): Promise<void> {
    if (!api) return
    const draft = typeof snapshot?.['issue_draft'] === 'string' ? snapshot['issue_draft'] : 'Сначала запусти snapshot диагностики.'
    setNotice(await api.writeClipboardText(draft) ? 'Issue draft скопирован.' : 'Не удалось скопировать issue draft.')
  }

  const health = Array.isArray(snapshot?.['health']) ? snapshot['health'] as readonly Record<string, unknown>[] : []
  const redaction = snapshot?.['redaction'] as Record<string, unknown> | undefined
  return (
    <section className="settings-info" aria-label="Диагностика и support bundle">
      <h3>Диагностика и support bundle</h3>
      <p>Core собирает bounded health snapshot. Сохранение локального ZIP выполняет main после финального scan.</p>
      <div className="safety__actions">
        <input aria-label="Идентификатор conversation" placeholder="conversation id (необязательно)" value={conversationId} onChange={(event) => setConversationId(event.target.value)} />
        <input aria-label="Идентификатор failed run" placeholder="failed run id (необязательно)" value={runId} onChange={(event) => setRunId(event.target.value)} />
        <button type="button" disabled={!api || !connected} onClick={() => void refresh()}>Обновить preview</button>
        <button type="button" disabled={!api || !connected} onClick={() => void save()}>Сохранить support bundle</button>
        <button type="button" disabled={!api || !snapshot} onClick={() => void copyDraft()}>Скопировать issue draft</button>
      </div>
      {snapshot ? <>
        <h4>Preview</h4>
        <p role="status">schema v{String(snapshot['schema_version'] ?? '?')} · scope {String(snapshot['scope'] ?? 'unknown')} · duration {health[0] ? String(health[0]['duration_ms'] ?? 0) : '0'} ms · run {String((snapshot['selected_run'] as Record<string, unknown> | undefined)?.['run_status'] ?? 'не выбран')}</p>
        <ul>{health.map((item) => <li key={String(item['id'])}>{String(item['id'])}: {String(item['status'])} — {String(item['reason_code'])}</li>)}</ul>
        <p>Redaction: raw payloads {redaction?.['raw_payloads_included'] === false ? 'исключены' : 'не подтверждено'} · blocked sections {Array.isArray(redaction?.['blocked_sections']) ? redaction?.['blocked_sections'].length : 0}</p>
      </> : <p role="status">Snapshot ещё не получен от Core.</p>}
      {notice ? <p role="alert">{notice}</p> : null}
    </section>
  )
}
