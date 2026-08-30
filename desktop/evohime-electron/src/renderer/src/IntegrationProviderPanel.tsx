import { useEffect, useState } from 'react'

import { useShellApi } from './shell-api'

export function IntegrationProviderPanel(): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState('Загрузка каталога…')

  useEffect(() => {
    if (!api) { setStatus('Core недоступен'); return }
    void api.invoke('integrationProvider.listCatalog', { requestId: crypto.randomUUID(), ownerScope: 'settings' }).then((outcome) => {
      if (!outcome.ok || !outcome.value.accepted) { setStatus('Каталог недоступен'); return }
      setStatus('Каталог загружен из Core')
    })
  }, [api])

  return <section className="settings-info" aria-label="Интеграции">
    <h3>Интеграции</h3>
    <p>{status}. Секреты и исполняемые данные не передаются в renderer.</p>
    <span className="settings-info__badge">fixture.echo · offline</span>
  </section>
}
