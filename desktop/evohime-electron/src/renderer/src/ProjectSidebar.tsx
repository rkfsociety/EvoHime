import { useCallback, useEffect, useState } from 'react'

import type { ChatSummary, ConnectionState, WorkspaceOption } from '@shared/api'

import { useShellApi } from './shell-api'
import { translate, useT } from './i18n'

/**
 * Chat workspace and its chats.
 *
 * A project is a workspace the user picked; its chats live under it and are
 * loaded only for the open workspace. The context rail owns workspace selection
 * and the chat list; global destinations stay in the adjacent icon rail.
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
  const t = useT()
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

  const selectWorkspace = useCallback(async (path: string) => {
    if (!api) return
    if (path.length === 0) {
      onWorkspaceChange(null)
      return
    }
    const outcome = await api.invoke('workspace.select', { path })
    if (outcome.ok && !Array.isArray(outcome.value)) {
      setProjects(outcome.value.options)
      onWorkspaceChange(outcome.value.selected)
      setError(null)
    } else if (!outcome.ok) {
      setError(outcome.message)
    }
  }, [api, onWorkspaceChange])

  const pickWorkspace = useCallback(async () => {
    if (!api) return
    const outcome = await api.invoke('workspace.pick', {})
    if (outcome.ok && !outcome.value.cancelled) {
      setProjects(outcome.value.selection.options)
      onWorkspaceChange(outcome.value.selection.selected)
      setError(null)
    } else if (!outcome.ok) {
      setError(outcome.message)
    }
  }, [api, onWorkspaceChange])

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
      <div className="chat-rail__actions" aria-label={t('Быстрые действия')}>
        <button
          type="button"
          className="chat-rail__action chat-rail__action--primary"
          onClick={() => onChatChange(null)}
        >
          <span className="chat-rail__action-icon" aria-hidden="true">＋</span>
          <span>{t('Новый чат')}</span>
          <kbd>Ctrl K</kbd>
        </button>
      </div>

      {error ? <p role="alert" className="shell__reason">{t(error)}</p> : null}
      {!connected ? (
        <p className="chat-rail__connection" role="status">{t('Core недоступен, задачи не запускаются.')}</p>
      ) : null}

      <section className="chat-rail__workspace-switcher" aria-label={t('Рабочая папка')}>
        <label htmlFor="workspace-select">{t('Рабочая папка')}</label>
        <div className="chat-rail__workspace-row">
          <select
            id="workspace-select"
            value={workspace ?? ''}
            onChange={(event) => void selectWorkspace(event.target.value)}
            aria-label={t('Рабочая папка')}
            title={workspace ?? t('Без проекта')}
          >
            <option value="">{t('Без проекта')}</option>
            {projects.map((project) => (
              <option key={project.path} value={project.path}>
                {basename(project.path)}{project.available ? '' : t(' · недоступен')}
              </option>
            ))}
          </select>
          <button
            type="button"
            className="chat-rail__workspace-pick"
            aria-label={t('Выбрать папку')}
            title={t('Выбрать папку')}
            onClick={() => void pickWorkspace()}
          >
            <span aria-hidden="true">＋</span>
          </button>
        </div>
      </section>

      <div className="chat-rail__heading">
        <div>
          <h2>{t('Чаты')}</h2>
          <small>{translate(workspace ? basename(workspace) : t('Без проекта'))}</small>
        </div>
      </div>

      {activeProject?.available === false ? (
        <p role="alert" className="chat-rail__warning">
          {t('Папка недоступна: её переименовали, удалили или диск не подключён.')}
        </p>
      ) : null}

      {chats.length === 0 ? (
        <p className="chat-rail__empty">{t('Чатов пока нет. Начни с нового чата.')}</p>
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
                aria-label={t('Удалить чат {title}').replace('{title}', chat.title)}
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
