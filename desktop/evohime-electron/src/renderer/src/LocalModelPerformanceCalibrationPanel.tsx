import { useEffect, useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState, ShellEvent } from '@shared/api'

export function LocalModelPerformanceCalibrationPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<unknown>(null)
  const [message, setMessage] = useState('Проверка доступности calibration adapter…')

  useEffect(() => {
    const onEvent = (event: ShellEvent): void => {
      if (event.kind === 'core-event' && event.event.localModelRuntimeManager?.operation?.startsWith('calibration_')) {
        setProjection(event.event.localModelRuntimeManager.projection)
      }
    }
    return api?.subscribe(onEvent)
  }, [api])

  useEffect(() => {
    if (!api || connection !== 'connected') {
      setMessage('Нет подключения к Core.')
      return
    }
    void api.invoke('core.localModelRuntimeManager', {
      operation: 'calibration_inspect',
      payload: '{}',
      expectedVersion: 0,
      idempotencyKey: crypto.randomUUID(),
    }).then(result => setMessage(result.ok ? 'Calibration status получен из Core.' : result.message))
  }, [api, connection])

  return <section className="settings-info" aria-label="Local Model Performance Calibration">
    <h3>Производительность локальной модели</h3>
    <p>Измерения запускаются только через verified runtime adapter. При его отсутствии результат остаётся typed unavailable.</p>
    <p role="status">{message}</p>
    {projection ? <pre aria-label="Calibration projection">{JSON.stringify(projection, null, 2)}</pre> : null}
  </section>
}
