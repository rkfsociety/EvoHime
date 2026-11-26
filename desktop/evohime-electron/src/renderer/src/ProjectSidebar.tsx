import { useCallback, useEffect, useRef, useState } from 'react'

import type { ChatSummary, ConnectionState, WorkspaceOption } from '@shared/api'

import { useShellApi } from './shell-api'
import { translate, useT } from './i18n'
import { UiIcon } from './UiIcon'

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
  /** True once Core has been connected at least once in this shell session. */
  readonly everConnected?: boolean
  readonly workspace: string | null
  readonly chatId: string | null
  readonly onWorkspaceChange: (workspace: string | null) => void
  readonly onChatChange: (chatId: string | null) => void
  /** Notifies the shell that the chat list changed (e.g. a chat was removed). */
  readonly onChatsChanged?: () => void
  /** Bumped by the chat surface when a prompt renames or reorders a chat. */
  readonly revision: number
}

export function ProjectSidebar({
  connection,
  everConnected = true,
  workspace,
  chatId,
  onWorkspaceChange,
  onChatChange,
  onChatsChanged,
  revision
}: ProjectSidebarProps): React.JSX.Element {
  const t = useT()
  const api = useShellApi()
  const [projects, setProjects] = useState<readonly WorkspaceOption[]>([])
  const [chats, setChats] = useState<readonly ChatSummary[]>([])
  const [error, setError] = useState<string | null>(null)
  const [openProjectMenuPath, setOpenProjectMenuPath] = useState<string | null>(null)
  const [openChatMenuId, setOpenChatMenuId] = useState<string | null>(null)
  const [renamingChatId, setRenamingChatId] = useState<string | null>(null)
  const [renameDraft, setRenameDraft] = useState('')
  const projectsRef = useRef<HTMLElement | null>(null)
  const chatsRef = useRef<HTMLUListElement | null>(null)

  useEffect(() => {
    if (!openProjectMenuPath && !openChatMenuId) return
    const onPointerDown = (event: MouseEvent): void => {
      const target = event.target as Node
      if (openProjectMenuPath && !projectsRef.current?.contains(target)) setOpenProjectMenuPath(null)
      if (openChatMenuId && !chatsRef.current?.contains(target)) setOpenChatMenuId(null)
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        setOpenProjectMenuPath(null)
        setOpenChatMenuId(null)
        setRenamingChatId(null)
      }
    }
    document.addEventListener('mousedown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    return () => {
      document.removeEventListener('mousedown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
    }
  }, [openChatMenuId, openProjectMenuPath])

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

  const forgetWorkspace = useCallback(async (path: string) => {
    if (!api) return
    const outcome = await api.invoke('workspace.forget', { path })
    setOpenProjectMenuPath(null)
    if (outcome.ok) {
      setProjects(outcome.value.options)
      onWorkspaceChange(outcome.value.selected)
      setError(null)
    } else {
      setError(outcome.message)
    }
  }, [api, onWorkspaceChange])

  const openWorkspaceInExplorer = useCallback(async (path: string) => {
    if (!api) return
    setOpenProjectMenuPath(null)
    const outcome = await api.invoke('workspace.openFolder', { path })
    if (!outcome.ok) {
      setError(outcome.message)
    }
  }, [api])

  const startNewChatInProject = useCallback(async (path: string) => {
    setOpenProjectMenuPath(null)
    if (workspace === null || !samePath(workspace, path)) {
      await selectWorkspace(path)
    }
    onChatChange(null)
  }, [onChatChange, selectWorkspace, workspace])

  const removeChat = useCallback(
    async (id: string) => {
      if (!api) return
      setOpenChatMenuId(null)
      const outcome = await api.invoke('chat.remove', { chatId: id })
      if (!outcome.ok) return
      setChats(outcome.value)
      if (id === chatId) onChatChange(null)
      onChatsChanged?.()
    },
    [api, chatId, onChatChange, onChatsChanged]
  )

  const archiveChat = useCallback(
    async (id: string) => {
      if (!api) return
      setOpenChatMenuId(null)
      const outcome = await api.invoke('chat.archive', { chatId: id, archived: true })
      if (!outcome.ok) return
      setChats(outcome.value)
      if (id === chatId) onChatChange(null)
      onChatsChanged?.()
    },
    [api, chatId, onChatChange, onChatsChanged]
  )

  const commitRenameChat = useCallback(
    async (id: string) => {
      if (!api) return
      const trimmed = renameDraft.trim()
      setRenamingChatId(null)
      if (trimmed.length === 0) return
      const outcome = await api.invoke('chat.rename', { chatId: id, title: trimmed })
      if (!outcome.ok || !outcome.value || typeof outcome.value.title !== 'string') return
      const nextTitle = outcome.value.title
      setChats((current) => current.map((item) => (item.id === id ? { ...item, title: nextTitle } : item)))
      onChatsChanged?.()
    },
    [api, onChatsChanged, renameDraft]
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
          <span className="chat-rail__action-icon" aria-hidden="true"><UiIcon name="plus" size={15} /></span>
          <span className="chat-rail__action-label">{t('Новый чат')}</span>
          <kbd>Ctrl K</kbd>
        </button>
      </div>

      {error ? <p role="alert" className="shell__reason">{t(error)}</p> : null}
      {!connected ? (
        everConnected ? (
          <p className="chat-rail__connection" role="status">{t('Core недоступен, задачи не запускаются.')}</p>
        ) : (
          // Холодный старт: Core ещё ни разу не подключился в этой сессии — это
          // не потеря связи, поэтому плашка спокойная, а не предупреждающая.
          <p className="chat-rail__connection chat-rail__connection--connecting" role="status">{t('Подключаемся к Core…')}</p>
        )
      ) : null}

      <section className="chat-rail__workspace-switcher" aria-label={t('Проекты')} ref={projectsRef}>
        <div className="chat-rail__projects-header">
          <span className="chat-rail__projects-title">{t('Проекты')}</span>
          <button
            type="button"
            className="chat-rail__workspace-pick"
            aria-label={t('Выбрать папку')}
            title={t('Выбрать папку')}
            onClick={() => void pickWorkspace()}
          >
            <UiIcon name="folder-plus" size={14} />
          </button>
        </div>
        {projects.length === 0 ? (
          <p className="chat-rail__projects-empty">{t('Без проекта')}</p>
        ) : (
          <ul className="chat-rail__projects" aria-label={t('Список проектов')}>
            {projects.map((project) => {
              const name = basename(project.path)
              const active = workspace !== null && samePath(project.path, workspace)
              const menuOpen = openProjectMenuPath !== null && samePath(openProjectMenuPath, project.path)
              return (
                <li
                  key={project.path}
                  className={`chat-rail__project-row${active ? ' chat-rail__project-row--active' : ''}${!project.available ? ' chat-rail__project-row--missing' : ''}`}
                >
                  <button
                    type="button"
                    className="chat-rail__project-btn"
                    aria-current={active ? 'true' : undefined}
                    title={project.path}
                    onClick={() => void selectWorkspace(project.path)}
                  >
                    <span className="chat-rail__project-icon" aria-hidden="true">
                      <UiIcon name="folder" size={14} />
                    </span>
                    <span className="chat-rail__project-name">{name}</span>
                  </button>
                  <div className="chat-rail__project-actions">
                    <button
                      type="button"
                      className="chat-rail__project-action"
                      aria-label={t('Действия с проектом {name}').replace('{name}', name)}
                      aria-expanded={menuOpen}
                      title={t('Действия с проектом')}
                      onClick={(event) => {
                        event.stopPropagation()
                        setOpenProjectMenuPath(menuOpen ? null : project.path)
                      }}
                    >
                      <UiIcon name="more-horizontal" size={13} />
                    </button>
                    <button
                      type="button"
                      className="chat-rail__project-action"
                      aria-label={t('Создать чат в проекте {name}').replace('{name}', name)}
                      title={t('Новый чат в этом проекте')}
                      onClick={(event) => {
                        event.stopPropagation()
                        void startNewChatInProject(project.path)
                      }}
                    >
                      <UiIcon name="plus" size={13} />
                    </button>
                  </div>
                  {menuOpen ? (
                    <div className="chat-rail__project-menu" role="menu" aria-label={t('Действия с проектом {name}').replace('{name}', name)}>
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => void openWorkspaceInExplorer(project.path)}
                      >
                        <UiIcon name="folder" size={13} />
                        <span>{t('Открыть в проводнике')}</span>
                      </button>
                      <button
                        type="button"
                        role="menuitem"
                        className="chat-rail__project-menu-danger"
                        onClick={() => void forgetWorkspace(project.path)}
                      >
                        <UiIcon name="x" size={13} />
                        <span>{t('Удалить из списка')}</span>
                      </button>
                    </div>
                  ) : null}
                </li>
              )
            })}
          </ul>
        )}
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
        <ul className="chats__list" ref={chatsRef}>
          {chats.map((chat) => {
            const chatMenuOpen = openChatMenuId === chat.id
            const isRenaming = renamingChatId === chat.id
            return (
              <li key={chat.id} className="chats__row">
                {isRenaming ? (
                  <input
                    type="text"
                    className="chats__rename-input"
                    aria-label={t('Новое название чата')}
                    value={renameDraft}
                    autoFocus
                    maxLength={80}
                    onChange={(event) => setRenameDraft(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') {
                        event.preventDefault()
                        void commitRenameChat(chat.id)
                      } else if (event.key === 'Escape') {
                        setRenamingChatId(null)
                      }
                    }}
                    onBlur={() => void commitRenameChat(chat.id)}
                  />
                ) : (
                  <>
                    <button
                      type="button"
                      className="chats__item"
                      aria-current={chat.id === chatId ? 'true' : undefined}
                      onClick={() => onChatChange(chat.id)}
                      title={chat.title}
                    >
                      <span className="chats__item-icon" aria-hidden="true"><UiIcon name="message" size={13} /></span>
                      <span className="chats__item-title">{chat.title}</span>
                    </button>
                    <div className="chat-rail__project-actions chat-rail__chat-actions">
                      <button
                        type="button"
                        className="chat-rail__project-action"
                        aria-label={t('Действия с чатом {title}').replace('{title}', chat.title)}
                        aria-expanded={chatMenuOpen}
                        title={t('Действия с чатом')}
                        onClick={(event) => {
                          event.stopPropagation()
                          setOpenChatMenuId(chatMenuOpen ? null : chat.id)
                        }}
                      >
                        <UiIcon name="more-horizontal" size={13} />
                      </button>
                      <button
                        type="button"
                        className="chat-rail__project-action"
                        aria-label={t('Архивировать чат {title}').replace('{title}', chat.title)}
                        title={t('Архивировать чат')}
                        onClick={(event) => {
                          event.stopPropagation()
                          void archiveChat(chat.id)
                        }}
                      >
                        <UiIcon name="archive" size={13} />
                      </button>
                    </div>
                  </>
                )}
                {chatMenuOpen ? (
                  <div className="chat-rail__project-menu" role="menu" aria-label={t('Действия с чатом {title}').replace('{title}', chat.title)}>
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setOpenChatMenuId(null)
                        setRenamingChatId(chat.id)
                        setRenameDraft(chat.title)
                      }}
                    >
                      <UiIcon name="chat" size={13} />
                      <span>{t('Переименовать')}</span>
                    </button>
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => void archiveChat(chat.id)}
                    >
                      <UiIcon name="archive" size={13} />
                      <span>{t('Архивировать')}</span>
                    </button>
                    <button
                      type="button"
                      role="menuitem"
                      className="chat-rail__project-menu-danger"
                      onClick={() => void removeChat(chat.id)}
                    >
                      <UiIcon name="x" size={13} />
                      <span>{t('Удалить чат')}</span>
                    </button>
                  </div>
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
