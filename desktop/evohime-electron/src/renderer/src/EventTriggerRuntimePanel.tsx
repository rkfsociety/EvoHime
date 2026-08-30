import { useEffect, useState } from 'react'

import { useShellApi } from './shell-api'

export function EventTriggerRuntimePanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка состояния…')

  useEffect(() => {
    if (!api) { setStatus('Core недоступен'); return }
    void api.invoke('eventTriggerRuntime.list', {
      requestId: crypto.randomUUID(), ownerScope: 'settings'
    }).then((outcome) => {
      setStatus(!outcome.ok || !outcome.value.accepted
        ? 'Состояние триггеров недоступно'
        : 'Локальные и системные источники доступны; webhook-провайдеры недоступны')
    })
  }, [api])

  return <section className="settings-info" aria-label="Триггеры событий">
    <h3>Триггеры событий</h3>
    <p>{status}</p>
    <span className="settings-info__badge">Core-owned · без автоматического запуска</span>
  </section>
}
