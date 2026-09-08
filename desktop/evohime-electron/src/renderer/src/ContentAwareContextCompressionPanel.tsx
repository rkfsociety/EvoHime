import { useState } from 'react'

export function ContentAwareContextCompressionPanel(): React.JSX.Element {
  const [sourceRef, setSourceRef] = useState('')
  return <section className="settings-info" aria-label="Content-aware context compression">
    <h3>Сжатие контекста</h3>
    <p>Core показывает compact lineage, savings и RecoveryUnavailable; renderer не сжимает и не восстанавливает source.</p>
    <label>Source reference <input value={sourceRef} maxLength={256} onChange={event => setSourceRef(event.target.value)} /></label>
    <p role="status">Ожидается Core projection для {sourceRef || 'выбранного контекста'}.</p>
  </section>
}
