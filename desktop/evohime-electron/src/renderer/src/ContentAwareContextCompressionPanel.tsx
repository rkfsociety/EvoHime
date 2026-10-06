import { translate } from './i18n'
import { useState } from 'react'

export function ContentAwareContextCompressionPanel(): React.JSX.Element {
  const [sourceRef, setSourceRef] = useState('')
  return <section className="settings-info" aria-label="Content-aware context compression">
    <h3>{translate("Сжатие контекста")}</h3>
    <p>{translate("Core показывает compact lineage, savings и RecoveryUnavailable; renderer не сжимает и не восстанавливает source.")}</p>
    <label>Source reference <input value={sourceRef} maxLength={256} onChange={event => setSourceRef(event.target.value)} /></label>
    <p role="status">{translate("Ожидается Core projection для")}{translate(sourceRef || translate("выбранного контекста"))}.</p>
  </section>
}
