import { useCallback, useEffect, useState } from 'react'

import type { ChatSummary, ConnectionState, WorkspaceOption } from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Projects and their chats.
 *
 * A project is a workspace the user picked; its chats live under it and are
 * loaded only for the open project. Picking a folder and checking whether it
 * still exists both happen in the main process — this component renders the
 * result and the failure cases.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export interface ProjectSidebarProps {
  readonly connection: ConnectionState
  readonly workspace: string | null
  readonly chatId: string | null
  readonly onWorkspaceChange: (workspace: string | null) => void
  readonly onChatChange: (chatId: string | null) => void
  /** Bumped by the chat surface when a prompt renames or reorders a chat. */
  readonly revision: number
}

export function ProjectSidebar({
  connection,
  workspace,
  chatId,
  onWorkspaceChange,
  onChatChange,
  revision
}: ProjectSidebarProps): React.JSX.Element {
  const api = useShellApi()
  const [projects, setProjects] = useState<readonly WorkspaceOption[]>([])
  const [chats, setChats] = useState<readonly ChatSummary[]>([])
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const loadProjects = useCallback(async () => {
    if (!api) {
      setError('Мост оболочки недоступен.')
      return
    }
    const outcome = await api.invoke('workspace.list', {})
    if (!outcome.ok) {
      setError(outcome.message)
      return
    }
    setError(null)
    setProjects(outcome.value.options)
    onWorkspaceChange(outcome.value.selected)
  }, [api, onWorkspaceChange])

  useEffect(() => {
    void loadProjects()
  }, [loadProjects])

  const loadChats = useCallback(async () => {
    if (!api || workspace === null) {
      setChats([])
      return
    }
    const outcome = await api.invoke('chat.list', { workspacePath: workspace })
    if (outcome.ok) setChats(outcome.value)
  }, [api, workspace])

  useEffect(() => {
    void loadChats()
  }, [loadChats, revision])

  const pick = useCallback(async () => {
    if (!api) return
    setBusy(true)
    const outcome = await api.invoke('workspace.pick', {})
    setBusy(false)
    if (!outcome.ok) {
      setError(outcome.message)
      return
    }
    if (outcome.value.cancelled) return
    setProjects(outcome.value.selection.options)
    onWorkspaceChange(outcome.value.selection.selected)
    onChatChange(null)
  }, [api, onChatChange, onWorkspaceChange])

  const select = useCallback(
    async (path: string) => {
      if (!api) return
      setBusy(true)
      const outcome = await api.invoke('workspace.select', { path })
      setBusy(false)
      if (!outcome.ok) {
        setError(outcome.message)
        return
      }
      setError(null)
      setProjects(outcome.value.options)
      onWorkspaceChange(outcome.value.selected)
      // Chats belong to a project, so the open one cannot survive a switch.
      onChatChange(null)
    },
    [api, onChatChange, onWorkspaceChange]
  )

  const forget = useCallback(
    async (path: string) => {
      if (!api) return
      setBusy(true)
      const outcome = await api.invoke('workspace.forget', { path })
      setBusy(false)
      if (!outcome.ok) return
      setProjects(outcome.value.options)
      onWorkspaceChange(outcome.value.selected)
      onChatChange(null)
    },
    [api, onChatChange, onWorkspaceChange]
  )

  const createChat = useCallback(async () => {
    if (!api || workspace === null) return
    setBusy(true)
    const outcome = await api.invoke('chat.create', { workspacePath: workspace })
    setBusy(false)
    if (!outcome.ok) {
      setError(outcome.message)
      return
    }
    await loadChats()
    onChatChange(outcome.value.id)
  }, [api, loadChats, onChatChange, workspace])

  const removeChat = useCallback(
    async (id: string) => {
      if (!api) return
      const outcome = await api.invoke('chat.remove', { chatId: id })
      if (!outcome.ok) return
      setChats(outcome.value)
      if (id === chatId) onChatChange(null)
    },
    [api, chatId, onChatChange]
  )

  const connected = CONNECTED_STATES.includes(connection)

  return (
    <div className="projects">
      <div className="projects__heading">
        <h2>Проекты</h2>
        <button type="button" onClick={() => void pick()} disabled={busy} title="Добавить проект">
          Выбрать папку…
        </button>
      </div>

      {error ? <p role="alert" className="shell__reason">{error}</p> : null}
      {!connected ? (
        <p className="shell__reason">Core недоступен, задачи не запускаются.</p>
      ) : null}

      {projects.length === 0 ? (
        <p className="shell__empty">Папка не выбрана — выбери её, чтобы начать работу.</p>
      ) : (
        <ul className="projects__list">
          {projects.map((project) => {
            const active = workspace !== null && samePath(project.path, workspace)
            return (
              <li key={project.path} className={project.available ? '' : 'projects__item--missing'}>
                <div className="projects__row">
                  <button
                    type="button"
                    className="projects__name"
                    onClick={() => void select(project.path)}
                    aria-current={active ? 'true' : undefined}
                    title={project.path}
                  >
                    {basename(project.path)}
                  </button>
                  {!project.available ? <span className="shell__badge">недоступна</span> : null}
                  <button
                    type="button"
                    className="projects__forget"
                    onClick={() => void forget(project.path)}
                    aria-label={`Забыть ${project.path}`}
                  >
                    ✕
                  </button>
                </div>

                {active ? (
                  <div className="chats">
                    <button
                      type="button"
                      className="chats__new"
                      onClick={() => void createChat()}
                      disabled={busy}
                    >
                      + Новый чат
                    </button>
                    {chats.length === 0 ? (
                      <p className="chats__empty">Чатов пока нет.</p>
                    ) : (
                      <ul className="chats__list">
                        {chats.map((chat) => (
                          <li key={chat.id}>
                            <button
                              type="button"
                              className="chats__item"
                              aria-current={chat.id === chatId ? 'true' : undefined}
                              onClick={() => onChatChange(chat.id)}
                              title={chat.title}
                            >
                              {chat.title}
                            </button>
                            <button
                              type="button"
                              className="chats__remove"
                              onClick={() => void removeChat(chat.id)}
                              aria-label={`Удалить чат ${chat.title}`}
                            >
                              ✕
                            </button>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                ) : null}

                {active && project.available === false ? (
                  <p role="alert" className="shell__reason">
                    Папка недоступна: её переименовали, удалили или диск не подключён.
                  </p>
                ) : null}
              </li>
            )
          })}
        </ul>
      )}
    </div>
  )
}

/** Windows paths are case-insensitive, so raw comparison would miss a match. */
function samePath(left: string, right: string): boolean {
  return left.toLowerCase() === right.toLowerCase()
}

function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter((part) => part.length > 0)
  return parts.at(-1) ?? path
}
