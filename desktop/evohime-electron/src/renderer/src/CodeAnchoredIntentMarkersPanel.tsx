import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { CodeAnchoredIntentMarkersProjection, ConnectionState, ShellEvent } from '@shared/api'

export function CodeAnchoredIntentMarkersPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<CodeAnchoredIntentMarkersProjection | null>(null)
  const [filePath, setFilePath] = useState('src/')
  const [revision, setRevision] = useState('')
  const [payload, setPayload] = useState('[]')
  const [message, setMessage] = useState('')

  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.codeAnchoredIntentMarkers) setProjection(event.event.codeAnchoredIntentMarkers)
  }), [api])

  const request = async (operation: 'scan' | 'propose') => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.codeAnchoredIntentMarkers', { operation, filePath, revision, payload, idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? operation === 'propose' ? 'Обычная задача запущена.' : 'Markers проверены Core.' : result.message)
  }

  return <section aria-label="Code-Anchored Intent Markers">
    <h3>Code-Anchored Intent Markers</h3>
    <p>Сканирование существующих комментариев инертно; запуск обычной задачи выполняется отдельным явным действием.</p>
    <label>Файл<input value={filePath} onChange={e => setFilePath(e.target.value)} /></label>
    <label>Revision<input value={revision} onChange={e => setRevision(e.target.value)} /></label>
    <label>Comment ranges JSON<textarea value={payload} onChange={e => setPayload(e.target.value)} maxLength={64 * 1024} /></label>
    <button type="button" onClick={() => void request('scan')}>Проверить markers в Core</button>
    <button type="button" onClick={() => void request('propose')}>Запустить обычную задачу</button>
    {projection?.projection ? <pre>{JSON.stringify(projection.projection, null, 2)}</pre> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
