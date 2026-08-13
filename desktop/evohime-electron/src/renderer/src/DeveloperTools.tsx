import { useCallback, useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

const MAX_ENTRIES = 200
const MAX_BYTES = 512 * 1024
const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

interface DeveloperToolsProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

interface WorkspaceEntry {
  readonly name: string
  readonly relative_path: string
  readonly directory: boolean
  readonly bytes?: number
}

interface WorkspaceListing {
  readonly path: string
  readonly entries: readonly WorkspaceEntry[]
  readonly truncated: boolean
}

export function DeveloperTools({ connection, events }: DeveloperToolsProps): React.JSX.Element {
  const api = useShellApi()
  const [workspacePath, setWorkspacePath] = useState<string | null>(null)
  const [currentPath, setCurrentPath] = useState('.')
  const [listing, setListing] = useState<WorkspaceListing | null>(null)
  const [selectedFile, setSelectedFile] = useState<string | null>(null)
  const [fileContent, setFileContent] = useState<string | null>(null)
  const [gitOutput, setGitOutput] = useState<string | null>(null)
  const [gitMode, setGitMode] = useState<'status' | 'diff'>('status')
  const [message, setMessage] = useState<string | null>(null)

  const connected = CONNECTED_STATES.includes(connection)

  useEffect(() => {
    if (!api) return
    void api.invoke('workspace.list', {}).then((outcome) => {
      if (outcome.ok) setWorkspacePath(outcome.value.selected)
    })
  }, [api])

  useEffect(() => {
    const response = latestEvent(events, 'workspace.list')
    if (response) {
      const parsed = parseJson(response.payload) as Partial<WorkspaceListing>
      if (Array.isArray(parsed.entries)) setListing(parsed as WorkspaceListing)
    }
    const file = latestEvent(events, 'workspace.file')
    if (file) {
      const parsed = parseJson(file.payload)
      if (typeof parsed.content === 'string') {
        setSelectedFile(typeof parsed.path === 'string' ? parsed.path : selectedFile)
        setFileContent(parsed.content)
      }
    }
    const gitStatus = latestEvent(events, 'git.status')
    const gitDiff = latestEvent(events, 'git.diff')
    const git = gitMode === 'status' ? gitStatus : gitDiff
    if (git) {
      const parsed = parseJson(git.payload)
      if (typeof parsed.output === 'string') setGitOutput(parsed.output)
    }
  }, [events, gitMode, selectedFile])

  const sendList = useCallback(
    async (relativePath: string) => {
      if (!api || !workspacePath || !connected) return
      setMessage(null)
      const outcome = await api.invoke('core.listWorkspace', {
        workspacePath,
        relativePath,
        maxEntries: MAX_ENTRIES
      })
      if (!outcome.ok) setMessage(outcome.message)
      else setCurrentPath(relativePath)
    },
    [api, connected, workspacePath]
  )

  const readFile = useCallback(
    async (relativePath: string) => {
      if (!api || !workspacePath || !connected) return
      setMessage(null)
      const outcome = await api.invoke('core.readWorkspaceFile', {
        workspacePath,
        relativePath,
        maxBytes: MAX_BYTES
      })
      if (!outcome.ok) setMessage(outcome.message)
      else setSelectedFile(relativePath)
    },
    [api, connected, workspacePath]
  )

  const runGit = useCallback(
    async (mode: 'status' | 'diff') => {
      if (!api || !workspacePath || !connected) return
      setGitMode(mode)
      setGitOutput(null)
      const outcome = mode === 'status'
        ? await api.invoke('core.gitStatus', { workspacePath, maxBytes: MAX_BYTES })
        : await api.invoke('core.gitDiff', { workspacePath, relativePath: selectedFile ?? '', maxBytes: MAX_BYTES })
      if (!outcome.ok) setMessage(outcome.message)
    },
    [api, connected, selectedFile, workspacePath]
  )

  const entries = useMemo(() => listing?.entries ?? [], [listing])

  return (
    <section className="shell__panel developer-tools" aria-label="Файлы и Git">
      <div className="developer-tools__heading">
        <div>
          <h2>Файлы и Git</h2>
          <p className="shell__empty">Только чтение через bounded Core IPC.</p>
        </div>
        <div className="developer-tools__actions">
          <button type="button" onClick={() => void sendList('.')} disabled={!connected || !workspacePath}>Обновить файлы</button>
          <button type="button" onClick={() => void runGit('status')} disabled={!connected || !workspacePath}>Git status</button>
          <button type="button" onClick={() => void runGit('diff')} disabled={!connected || !workspacePath || !selectedFile}>Git diff</button>
        </div>
      </div>

      {!workspacePath ? <p className="shell__reason">Сначала выбери рабочую папку.</p> : null}
      {!connected ? <p className="shell__reason">Core недоступен: чтение файлов и Git приостановлены.</p> : null}
      {message ? <p role="alert" className="shell__reason">{message}</p> : null}

      <div className="developer-tools__grid">
        <div>
          <h3>Дерево: {currentPath}</h3>
          {listing ? (
            <ul className="developer-tools__files">
              {currentPath !== '.' ? <li><button type="button" onClick={() => void sendList(parentPath(currentPath))}>..</button></li> : null}
              {entries.map((entry) => (
                <li key={entry.relative_path}>
                  <button type="button" onClick={() => void (entry.directory ? sendList(entry.relative_path) : readFile(entry.relative_path))}>
                    {entry.directory ? '▸ ' : '· '}{entry.name}
                  </button>
                  {!entry.directory && entry.bytes !== undefined ? <span>{entry.bytes} Б</span> : null}
                </li>
              ))}
            </ul>
          ) : <p className="shell__empty">Нажми «Обновить файлы».</p>}
          {listing?.truncated ? <p className="shell__reason">Список ограничен Core до {MAX_ENTRIES} элементов.</p> : null}
        </div>

        <div className="developer-tools__preview">
          <h3>{selectedFile ?? 'Предпросмотр файла'}</h3>
          <pre>{fileContent ?? 'Выбери текстовый файл в дереве.'}</pre>
        </div>
      </div>

      <div className="developer-tools__git">
        <h3>{gitMode === 'status' ? 'Git status' : `Git diff${selectedFile ? `: ${selectedFile}` : ''}`}</h3>
        <pre>{gitOutput ?? 'Нажми Git status или выбери файл и запроси Git diff.'}</pre>
      </div>
    </section>
  )
}

function latestEvent(events: readonly CoreEvent[], eventType: string): CoreEvent | null {
  return [...events].reverse().find((event) => event.eventType === eventType) ?? null
}

function parseJson(payload: string): Record<string, any> {
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? value as Record<string, any> : {}
  } catch {
    return {}
  }
}

function parentPath(path: string): string {
  const slash = path.lastIndexOf('/')
  return slash <= 0 ? '.' : path.slice(0, slash)
}
