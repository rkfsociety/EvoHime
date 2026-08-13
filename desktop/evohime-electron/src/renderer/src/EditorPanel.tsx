import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']
const MAX_TEXT = 128 * 1024

interface EditorPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

/**
 * Core-owned bounded editor flow: create project, prepare a diff, then apply
 * only the exact approved proposal returned by Core. The renderer never writes
 * workspace files directly.
 */
export function EditorPanel({ connection, events }: EditorPanelProps): React.JSX.Element {
  const api = useShellApi()
  const [workspacePath, setWorkspacePath] = useState('')
  const [projectId, setProjectId] = useState('electron-editor')
  const [relativePath, setRelativePath] = useState('')
  const [content, setContent] = useState('')
  const [approvedBuild, setApprovedBuild] = useState<string | null>(null)
  const [message, setMessage] = useState<string | null>(null)
  const connected = CONNECTED_STATES.includes(connection)

  useEffect(() => {
    if (!api) return
    void api.invoke('workspace.list', {}).then((outcome) => {
      if (outcome.ok && outcome.value.selected) setWorkspacePath(outcome.value.selected)
    })
  }, [api])

  const prepared = useMemo(() => latestEvent(events, 'build.prepared'), [events])
  const applied = useMemo(() => latestEvent(events, 'build.applied'), [events])

  useEffect(() => {
    if (prepared) {
      setApprovedBuild(prepared.payload)
      setMessage('Core подготовил bounded diff. Проверь preview и одобри применение.')
    }
    if (applied) {
      setMessage('Изменения применены Core с сохранением snapshot.')
      setApprovedBuild(null)
    }
  }, [applied, prepared])

  async function createProject(): Promise<void> {
    if (!api || !connected || !workspacePath || !projectId.trim()) return
    const outcome = await api.invoke('core.createProject', {
      projectId: projectId.trim(),
      title: 'Electron Editor project',
      workspacePath,
      sourceRef: 'electron-editor'
    })
    setMessage(outcome.ok ? 'Проект зарегистрирован в Core.' : outcome.message)
  }

  async function prepareBuild(): Promise<void> {
    if (!api || !connected || !workspacePath || !projectId.trim() || !relativePath.trim() || content.length === 0 || content.length > MAX_TEXT) return
    const slashPath = relativePath.trim().replaceAll('\\', '/')
    const directory = slashPath.includes('/') ? slashPath.slice(0, slashPath.lastIndexOf('/')) : ''
    const extension = slashPath.includes('.') ? slashPath.split('.').pop() ?? 'txt' : 'txt'
    const proposal = {
      scope: {
        allowed_paths: directory ? [directory] : [],
        allowed_operations: ['write'],
        expected_outputs: ['updated source'],
        forbidden_paths: [],
        allowed_file_types: [extension],
        max_files_changed: 1,
        max_bytes_changed: content.length,
        allow_create: true,
        allow_delete: false,
        allow_rename: false,
        dependency_changes: null,
        acceptance_criteria: 'Editor content matches the approved proposal',
        risk_class: 'medium',
        timeout_ms: 30_000
      },
      changes: [{ relative_path: slashPath, new_content: content, expected_content_hash: null, delete: false }]
    }
    const outcome = await api.invoke('core.prepareBuild', {
      projectId: projectId.trim(),
      proposalJson: JSON.stringify(proposal)
    })
    if (!outcome.ok) setMessage(outcome.message)
  }

  async function applyBuild(): Promise<void> {
    if (!api || !approvedBuild || !connected) return
    const outcome = await api.invoke('core.applyApprovedBuild', {
      projectId: projectId.trim(),
      runId: `electron-editor-${Date.now()}`,
      taskId: `editor-${Date.now()}`,
      approvedBuildJson: approvedBuild
    })
    if (!outcome.ok) setMessage(outcome.message)
  }

  return (
    <section className="shell__panel editor-panel" aria-label="Editor">
      <div className="editor-panel__heading">
        <div>
          <h2>Editor</h2>
          <p className="shell__empty">Подготовка diff и запись только через Core approval.</p>
        </div>
        <button type="button" onClick={() => void createProject()} disabled={!connected || !workspacePath}>Создать проект</button>
      </div>
      <div className="editor-panel__form">
        <label>Project ID<input value={projectId} onChange={(event) => setProjectId(event.target.value)} /></label>
        <label>Относительный путь<input value={relativePath} onChange={(event) => setRelativePath(event.target.value)} placeholder="src/example.txt" /></label>
        <label className="editor-panel__content">Новое содержимое<textarea value={content} onChange={(event) => setContent(event.target.value)} maxLength={MAX_TEXT} /></label>
      </div>
      <div className="editor-panel__actions">
        <button type="button" onClick={() => void prepareBuild()} disabled={!connected || !workspacePath || !relativePath.trim() || content.length === 0}>Подготовить diff</button>
        <button type="button" onClick={() => void applyBuild()} disabled={!connected || !approvedBuild}>Одобрить и применить</button>
      </div>
      {message ? <p role="alert" className="shell__reason">{message}</p> : null}
      {approvedBuild ? <pre className="editor-panel__preview">{formatPreview(approvedBuild)}</pre> : <p className="shell__empty">Diff появится после проверки Core.</p>}
    </section>
  )
}

function latestEvent(events: readonly CoreEvent[], eventType: string): CoreEvent | null {
  return [...events].reverse().find((event) => event.eventType === eventType) ?? null
}

function formatPreview(payload: string): string {
  try {
    return JSON.stringify(JSON.parse(payload), null, 2)
  } catch {
    return payload
  }
}
