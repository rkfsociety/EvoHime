import { translate } from './i18n'
import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

interface Props { readonly connection: ConnectionState; readonly events: readonly CoreEvent[] }

export function DiagnosticsAndSupportBundlePanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const [notice, setNotice] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
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

  async function submit(): Promise<void> {
    if (!api || sending) return
    if (!window.confirm(translate('Отправить redacted support bundle в публичный GitHub issue для анализа?'))) return
    setSending(true)
    setNotice('Отправляю redacted support bundle…')
    const result = await api.invoke('shell.submitDiagnostics', {})
    setSending(false)
    if (!result.ok) {
      setNotice(result.message)
      return
    }
    const opened = await api.openExternal(result.value.url)
    setNotice(opened ? `Issue создан и открыт: ${result.value.url}` : `Issue создан: ${result.value.url}`)
  }

  const health = Array.isArray(snapshot?.['health']) ? snapshot['health'] as readonly Record<string, unknown>[] : []
  const redaction = snapshot?.['redaction'] as Record<string, unknown> | undefined
  return (
    <section className="settings-info" aria-label={translate("Диагностика и support bundle")}>
      <h3>{translate("Диагностика и support bundle")}</h3>
      <p>{translate("Core собирает bounded health snapshot. Main делает финальный redaction scan. После живого падения задачи Ева автоматически отправляет redacted report при наличии авторизации GitHub; кнопку можно использовать для повторной ручной отправки.")}</p>
      <div className="safety__actions">
        <input aria-label={translate("Идентификатор conversation")} placeholder={translate("conversation id (необязательно)")} value={conversationId} onChange={(event) => setConversationId(event.target.value)} />
        <input aria-label={translate("Идентификатор failed run")} placeholder={translate("failed run id (необязательно)")} value={runId} onChange={(event) => setRunId(event.target.value)} />
        <button type="button" disabled={!api || !connected} onClick={() => void refresh()}>{translate("Обновить preview")}</button>
        <button type="button" disabled={!api || !connected || !snapshot} onClick={() => void save()}>{translate("Сохранить support bundle")}</button>
        <button type="button" disabled={!api || !connected || !snapshot || sending} onClick={() => void submit()}>{translate(sending ? translate("Отправка…") : translate("Отправить в GitHub issue"))}</button>
        <button type="button" disabled={!api || !snapshot} onClick={() => void copyDraft()}>{translate("Скопировать issue draft")}</button>
      </div>
      {snapshot ? <>
        <h4>Preview</h4>
        <p role="status">schema v{translate(String(snapshot['schema_version'] ?? '?'))} · scope {translate(String(snapshot['scope'] ?? 'unknown'))} · duration {translate(health[0] ? String(health[0]['duration_ms'] ?? 0) : '0')} ms · run {translate(String((snapshot['selected_run'] as Record<string, unknown> | undefined)?.['run_status'] ?? translate("не выбран")))}</p>
        <ul>{health.map((item) => <li key={String(item['id'])}>{translate(String(item['id']))}: {translate(String(item['status']))} — {translate(String(item['reason_code']))}</li>)}</ul>
        <p>Redaction: raw payloads {translate(redaction?.['raw_payloads_included'] === false ? translate("исключены") : translate("не подтверждено"))} · blocked sections {translate(Array.isArray(redaction?.['blocked_sections']) ? redaction?.['blocked_sections'].length : 0)}</p>
      </> : <p role="status">{translate("Snapshot ещё не получен от Core.")}</p>}
      {notice ? <p role="alert">{translate(notice)}</p> : null}
    </section>
  )
}
