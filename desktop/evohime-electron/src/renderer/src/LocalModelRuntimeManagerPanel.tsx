import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState, LocalModelRuntimeManagerProjection, ShellEvent } from '@shared/api'

type ManagerOperation = 'inspect' | 'hardware' | 'fit' | 'download_artifact' | 'save_policy' | 'get_policy' | 'start' | 'stop' | 'probe' | 'verify_artifact' | 'promote_artifact' | 'transition' | 'profile' | 'register_model' | 'register_runtime' | 'register_artifact' | 'register_session' | 'recover'

export function LocalModelRuntimeManagerPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<LocalModelRuntimeManagerProjection | null>(null)
  const [payload, setPayload] = useState('{}')
  const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.localModelRuntimeManager) setProjection(event.event.localModelRuntimeManager)
  }), [api])
  const request = async (operation: ManagerOperation) => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.localModelRuntimeManager', { operation, payload, expectedVersion: projection?.version ?? 0, idempotencyKey: crypto.randomUUID() })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }
  return <section aria-label="Local Model Runtime Manager">
    <h3>Local Model Runtime Manager</h3>
    <p>Hardware, catalog, artifact и health остаются Core-owned; запуск процесса возможен только через supervisor boundary.</p>
    <textarea aria-label="Manager payload JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={256 * 1024} />
    <div>
      <button type="button" onClick={() => void request('inspect')}>Показать состояние</button>
      <button type="button" onClick={() => void request('hardware')}>Снять hardware snapshot</button>
      <button type="button" onClick={() => void request('fit')}>Рассчитать fit</button>
      <button type="button" onClick={() => void request('download_artifact')}>Скачать artifact</button>
      <button type="button" onClick={() => void request('save_policy')}>Сохранить policy</button>
      <button type="button" onClick={() => void request('get_policy')}>Загрузить policy</button>
      <button type="button" onClick={() => void request('start')}>Запустить runtime</button>
      <button type="button" onClick={() => void request('probe')}>Проверить health</button>
      <button type="button" onClick={() => void request('stop')}>Остановить runtime</button>
      <button type="button" onClick={() => void request('verify_artifact')}>Проверить artifact</button>
      <button type="button" onClick={() => void request('promote_artifact')}>Установить artifact</button>
      <button type="button" onClick={() => void request('transition')}>Проверить переход</button>
      <button type="button" onClick={() => void request('profile')}>Создать profile</button>
      <button type="button" onClick={() => void request('recover')}>Восстановить состояние</button>
    </div>
    {projection?.projection ? <pre>{JSON.stringify(projection.projection, null, 2)}</pre> : null}
    {message ? <p role="status">{message}</p> : null}
  </section>
}
