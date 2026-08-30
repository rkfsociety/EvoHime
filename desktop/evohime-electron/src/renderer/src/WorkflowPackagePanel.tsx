import { useState } from 'react'

import { useShellApi } from './shell-api'

/** Metadata-only package actions. Core remains the parser, validator and store owner. */
export function WorkflowPackagePanel(): React.JSX.Element {
  const api = useShellApi()
  const [graphJson, setGraphJson] = useState('{}')
  const [packageJson, setPackageJson] = useState('')
  const [name, setName] = useState('Workflow Package')
  const [destinationPath, setDestinationPath] = useState('workflow.evohime-workflow.json')
  const [sourcePath, setSourcePath] = useState('workflow.evohime-workflow.json')
  const [slotId, setSlotId] = useState('')
  const [credentialReference, setCredentialReference] = useState('')
  const [status, setStatus] = useState('')

  const invoke = async (command: Parameters<NonNullable<typeof api>['invoke']>[0], payload: never): Promise<void> => {
    if (!api) return
    const result = await api.invoke(command as never, payload as never)
    setStatus(result.ok ? 'Команда отправлена в Core.' : 'Core отклонил команду.')
  }

  return (
    <section className="panel" aria-label="Workflow Package">
      <div className="panel__header">
        <div>
          <p className="eyebrow">Переносимый workflow</p>
          <h3>Workflow Package</h3>
        </div>
        <span className="status-chip">Core-owned</span>
      </div>
      <p className="panel__hint">JSON-пакет проверяется и сохраняется Core. Renderer не читает SQLite и не получает credentials.</p>
      <label className="field">
        <span>Граф workflow/v1</span>
        <textarea value={graphJson} onChange={(event) => setGraphJson(event.target.value)} rows={5} />
      </label>
      <label className="field">
        <span>Имя</span>
        <input value={name} onChange={(event) => setName(event.target.value)} />
      </label>
      <div className="panel__actions">
        <button type="button" onClick={() => void invoke('workflowPackage.preview', {
          graphJson, name, description: '', portableArgumentKeys: [], credentialSlotsJson: '[]', createdAt: new Date().toISOString()
        } as never)}>Preview</button>
        <button type="button" onClick={() => void invoke('workflowPackage.export', {
          graphJson, name, description: '', portableArgumentKeys: [], credentialSlotsJson: '[]', createdAt: new Date().toISOString(), destinationPath
        } as never)}>Export</button>
      </div>
      <label className="field">
        <span>Путь export/import</span>
        <input value={destinationPath} onChange={(event) => { setDestinationPath(event.target.value); setSourcePath(event.target.value) }} />
      </label>
      <label className="field">
        <span>Package JSON для commit/rebind</span>
        <textarea value={packageJson} onChange={(event) => setPackageJson(event.target.value)} rows={5} />
      </label>
      <div className="panel__actions">
        <button type="button" disabled={!packageJson} onClick={() => void invoke('workflowPackage.commit', { packageJson, sourcePath, idempotencyKey: crypto.randomUUID() } as never)}>Commit</button>
        <button type="button" disabled={!packageJson || !slotId || !credentialReference} onClick={() => void invoke('workflowPackage.rebind', { packageJson, slotId, localCredentialReference: credentialReference } as never)}>Rebind slot</button>
      </div>
      <div className="panel__grid panel__grid--two">
        <label className="field"><span>Slot id</span><input value={slotId} onChange={(event) => setSlotId(event.target.value)} /></label>
        <label className="field"><span>Opaque local reference</span><input value={credentialReference} onChange={(event) => setCredentialReference(event.target.value)} /></label>
      </div>
      <p role="status" className="panel__hint">{status}</p>
    </section>
  )
}
