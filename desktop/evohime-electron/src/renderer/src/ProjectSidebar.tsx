import { useCallback, useEffect, useState } from 'react'

import type { ChatSummary, ConnectionState, WorkspaceOption } from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Chat workspace and its chats.
 *
 * A project is a workspace the user picked; its chats live under it and are
 * loaded only for the open workspace. Project selection happens in the chat
 * composer; this component keeps the rail focused on navigation and chats.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export interface ProjectSidebarProps {
  readonly connection: ConnectionState
  readonly workspace: string | null
  readonly chatId: string | null
  readonly onWorkspaceChange: (workspace: string | null) => void
  readonly onChatChange: (chatId: string | null) => void
  readonly onScheduled: () => void
  readonly onPlugins: () => void
  /** Bumped by the chat surface when a prompt renames or reorders a chat. */
  readonly revision: number
}

export function ProjectSidebar({
  connection,
  workspace,
  chatId,
  onWorkspaceChange,
  onChatChange,
  onScheduled,
  onPlugins,
  revision
}: ProjectSidebarProps): React.JSX.Element {
  const api = useShellApi()
  const [projects, setProjects] = useState<readonly WorkspaceOption[]>([])
  const [chats, setChats] = useState<readonly ChatSummary[]>([])
  const [error, setError] = useState<string | null>(null)

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
    if (!api) return
    const outcome = await api.invoke('chat.list', { workspacePath: workspace })
    if (outcome.ok) setChats(outcome.value)
  }, [api, workspace])

  useEffect(() => {
    void loadChats()
  }, [loadChats, revision])

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
  const activeProject = projects.find((project) => workspace !== null && samePath(project.path, workspace))

  return (
    <div className="chat-rail">
      <div className="chat-rail__actions" aria-label="Быстрые действия">
        <button
          type="button"
          className="chat-rail__action chat-rail__action--primary"
          onClick={() => onChatChange(null)}
        >
          <span className="chat-rail__action-icon" aria-hidden="true">＋</span>
          <span>Новый чат</span>
          <kbd>Ctrl K</kbd>
        </button>
        <button type="button" className="chat-rail__action" onClick={onScheduled}>
          <span className="chat-rail__action-icon" aria-hidden="true">◷</span>
          <span>Запланировано</span>
        </button>
        <button type="button" className="chat-rail__action" onClick={onPlugins}>
          <span className="chat-rail__action-icon" aria-hidden="true">✦</span>
          <span>Плагины</span>
        </button>
      </div>

      {error ? <p role="alert" className="shell__reason">{error}</p> : null}
      {!connected ? (
        <p className="shell__reason">Core недоступен, задачи не запускаются.</p>
      ) : null}

      <div className="chat-rail__heading">
        <div>
          <h2>Чаты</h2>
          <small>{workspace ? basename(workspace) : 'Без проекта'}</small>
        </div>
      </div>

      {activeProject?.available === false ? (
        <p role="alert" className="chat-rail__warning">
          Папка недоступна: её переименовали, удалили или диск не подключён.
        </p>
      ) : null}

      {chats.length === 0 ? (
        <p className="chat-rail__empty">Чатов пока нет. Начни с нового чата.</p>
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
                <span className="chats__item-icon" aria-hidden="true">✦</span>
                <span className="chats__item-title">{chat.title}</span>
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
