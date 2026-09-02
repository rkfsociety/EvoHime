import React, { useState } from 'react'
import { useShellApi } from './shell-api'
import type { ConnectionState } from '@shared/api'

export function CustomizationInventoryPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi(); const [result, setResult] = useState<unknown>(null); const [message, setMessage] = useState('')
  const list = async () => { if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }; const r = await api.invoke('core.customizationInventory', { operation: 'list' }); if (r.ok) { setResult(r.value); setMessage('Каталог обновлён.') } else setMessage(r.message) }
  return <section aria-label="Customization Inventory"><h3>Customization Inventory</h3><p>Единый metadata-каталог; authority и runtime semantics остаются у владельцев.</p><button type="button" onClick={() => void list()}>Обновить каталог</button>{result ? <pre>{JSON.stringify(result, null, 2)}</pre> : null}{message ? <p role="status">{message}</p> : null}</section>
}
