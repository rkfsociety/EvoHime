import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'

import type { ChatSummary, ConnectionState, WorkspaceOption } from '@shared/api'

import { useShellApi } from './shell-api'
import { useT } from './i18n'
import { UiIcon } from './UiIcon'

/**
 * Chat workspace and its chats.
 *
 * A project is a workspace the user picked; its chats live under it and are
 * loaded only for the open workspace. The context rail owns workspace selection
 * and the chat list; global destinations stay in the adjacent icon rail.
 */

interface FloatingMenuState<T extends string> {
  readonly key: T
  readonly left: number
  readonly top: number
  readonly anchorTop: number
}

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
  const [openProjectMenu, setOpenProjectMenu] = useState<FloatingMenuState<string> | null>(null)
  const [openChatMenu, setOpenChatMenu] = useState<FloatingMenuState<string> | null>(null)
  const [renamingChatId, setRenamingChatId] = useState<string | null>(null)
  const [renameDraft, setRenameDraft] = useState('')
  /** Lowercase project paths (or '' for standalone chats) with visible chat lists. */
  const [expandedGroups, setExpandedGroups] = useState<ReadonlySet<string>>(new Set())
  const projectsRef = useRef<HTMLElement | null>(null)
  const treeRef = useRef<HTMLUListElement | null>(null)
  const projectMenuRef = useRef<HTMLDivElement | null>(null)
  const chatMenuRef = useRef<HTMLDivElement | null>(null)

  // Текущий проект (или группу чатов без проекта) всегда раскрываем: что бы
  // ни происходило в оболочке, пользователь видит чаты активного места.
  useEffect(() => {
    const key = workspace === null ? '' : workspace.toLowerCase()
    setExpandedGroups((current) => (current.has(key) ? current : new Set(current).add(key)))
  }, [workspace])

  useEffect(() => {
    if (!openProjectMenu && !openChatMenu) return
    const onPointerDown = (event: MouseEvent): void => {
      const target = event.target as Node
      if (openProjectMenu && !projectsRef.current?.contains(target) && !projectMenuRef.current?.contains(target)) {
        setOpenProjectMenu(null)
      }
      if (openChatMenu && !treeRef.current?.contains(target) && !chatMenuRef.current?.contains(target)) {
        setOpenChatMenu(null)
      }
    }
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        setOpenProjectMenu(null)
        setOpenChatMenu(null)
        setRenamingChatId(null)
      }
    }
    const onResize = (): void => {
      setOpenProjectMenu(null)
      setOpenChatMenu(null)
    }
    document.addEventListener('mousedown', onPointerDown)
    document.addEventListener('keydown', onKeyDown)
    window.addEventListener('resize', onResize)
    return () => {
      document.removeEventListener('mousedown', onPointerDown)
      document.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('resize', onResize)
    }
  }, [openChatMenu, openProjectMenu])

  useLayoutEffect(() => {
    const adjustMenuPosition = (menuEl: HTMLDivElement | null, anchorTop: number): void => {
      if (!menuEl) return
      const rect = menuEl.getBoundingClientRect()
      if (rect.height === 0 && rect.width === 0) return
      const margin = 8
      if (rect.bottom > window.innerHeight - margin) {
        const flippedTop = Math.max(margin, anchorTop - rect.height - 4)
        menuEl.style.top = `${flippedTop}px`
      }
      if (rect.right > window.innerWidth - margin) {
        const shiftedLeft = Math.max(margin, window.innerWidth - rect.width - margin)
        menuEl.style.left = `${shiftedLeft}px`
      }
    }
    if (openProjectMenu) adjustMenuPosition(projectMenuRef.current, openProjectMenu.anchorTop)
    if (openChatMenu) adjustMenuPosition(chatMenuRef.current, openChatMenu.anchorTop)
  }, [openChatMenu, openProjectMenu])

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
    const outcome = await api.invoke('chat.listAll', {})
    if (outcome.ok) setChats(outcome.value)
  }, [api])

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
    setOpenProjectMenu(null)
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
    setOpenProjectMenu(null)
    const outcome = await api.invoke('workspace.openFolder', { path })
    if (!outcome.ok) {
      setError(outcome.message)
    }
  }, [api])

  const toggleGroup = useCallback((key: string) => {
    setExpandedGroups((current) => {
      const next = new Set(current)
      if (next.has(key)) next.delete(key)
      else next.add(key)
      return next
    })
  }, [])

  const startNewChatInGroup = useCallback(
    async (path: string | null) => {
      setOpenProjectMenu(null)
      setExpandedGroups((current) => new Set(current).add(path === null ? '' : path.toLowerCase()))
      if (path === null) {
        if (workspace !== null) onWorkspaceChange(null)
      } else if (workspace === null || !samePath(workspace, path)) {
        await selectWorkspace(path)
      }
      onChatChange(null)
    },
    [onChatChange, onWorkspaceChange, selectWorkspace, workspace]
  )

  const openChat = useCallback(
    async (chat: ChatSummary) => {
      // Чат живёт в своём проекте: открытие попутно переключает оболочку
      // на этот проект, чтобы новая задача пошла в правильной папке.
      if (chat.workspacePath === null) {
        if (workspace !== null) onWorkspaceChange(null)
      } else if (workspace === null || !samePath(workspace, chat.workspacePath)) {
        await selectWorkspace(chat.workspacePath)
      }
      onChatChange(chat.id)
    },
    [onChatChange, onWorkspaceChange, selectWorkspace, workspace]
  )

  const removeChat = useCallback(
    async (id: string) => {
      if (!api) return
      setOpenChatMenu(null)
      const outcome = await api.invoke('chat.remove', { chatId: id })
      if (!outcome.ok) return
      // Дерево держит чаты всех проектов, а main отвечает списком одного —
      // поэтому правим общий список локально.
      setChats((current) => current.filter((chat) => chat.id !== id))
      if (id === chatId) onChatChange(null)
      onChatsChanged?.()
    },
    [api, chatId, onChatChange, onChatsChanged]
  )

  const archiveChat = useCallback(
    async (id: string) => {
      if (!api) return
      setOpenChatMenu(null)
      const outcome = await api.invoke('chat.archive', { chatId: id, archived: true })
      if (!outcome.ok) return
      setChats((current) => current.filter((chat) => chat.id !== id))
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
  const chatGroups = useMemo(() => buildChatGroups(projects, chats), [projects, chats])
  const openProjectTarget = openProjectMenu
    ? chatGroups.find((group) => samePath(openProjectMenu.key, group.path ?? '')) ?? null
    : null
  const openChatTarget = openChatMenu
    ? chats.find((chat) => chat.id === openChatMenu.key) ?? null
    : null

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

        {activeProject?.available === false ? (
          <p role="alert" className="chat-rail__warning">
            {t('Папка недоступна: её переименовали, удалили или диск не подключён.')}
          </p>
        ) : null}

        {chatGroups.length === 0 ? (
          <p className="chat-rail__projects-empty">{t('Добавь проект — и чаты появятся внутри него.')}</p>
        ) : (
          <ul
            className="chat-rail__projects chat-rail__tree"
            aria-label={t('Список проектов')}
            onScroll={() => {
              if (openProjectMenu) setOpenProjectMenu(null)
              if (openChatMenu) setOpenChatMenu(null)
            }}
          >
            {chatGroups.map((group) => {
              const active = group.path !== null && workspace !== null && samePath(group.path, workspace)
              const expanded = expandedGroups.has(group.key)
              const name = group.path === null ? t('Без проекта') : basename(group.path)
              const menuOpen = openProjectMenu !== null && group.path !== null && samePath(openProjectMenu.key, group.path)
              return (
                <li key={group.key} className="chat-rail__tree-group">
                  <div
                    className={`chat-rail__project-row${active ? ' chat-rail__project-row--active' : ''}${!group.available ? ' chat-rail__project-row--missing' : ''}`}
                  >
                    <button
                      type="button"
                      className={`chat-rail__chevron${expanded ? ' chat-rail__chevron--open' : ''}`}
                      aria-expanded={expanded}
                      aria-label={(expanded ? t('Свернуть чаты проекта {name}') : t('Развернуть чаты проекта {name}')).replace('{name}', name)}
                      title={expanded ? t('Свернуть') : t('Развернуть')}
                      onClick={() => toggleGroup(group.key)}
                    >
                      <UiIcon name="chevron-right" size={12} />
                    </button>
                    <button
                      type="button"
                      className="chat-rail__project-btn"
                      aria-current={active ? 'true' : undefined}
                      title={group.path ?? undefined}
                      onClick={() => {
                        toggleGroup(group.key)
                        void selectWorkspace(group.path ?? '')
                      }}
                    >
                      <span className="chat-rail__project-icon" aria-hidden="true">
                        <UiIcon name="folder" size={14} />
                      </span>
                      <span className="chat-rail__project-name">{name}</span>
                      {group.chats.length > 0 ? (
                        <span className="chat-rail__project-count" aria-hidden="true">{group.chats.length}</span>
                      ) : null}
                    </button>
                    <div className="chat-rail__project-actions">
                      {group.path !== null ? (
                        <button
                          type="button"
                          className="chat-rail__project-action"
                          aria-label={t('Действия с проектом {name}').replace('{name}', name)}
                          aria-expanded={menuOpen}
                          title={t('Действия с проектом')}
                          onClick={(event) => {
                            event.stopPropagation()
                            if (menuOpen) {
                              setOpenProjectMenu(null)
                              return
                            }
                            const rect = event.currentTarget.getBoundingClientRect()
                            setOpenChatMenu(null)
                            setOpenProjectMenu({
                              key: group.path!,
                              left: rect.left,
                              top: rect.bottom + 4,
                              anchorTop: rect.top
                            })
                          }}
                        >
                          <UiIcon name="more-horizontal" size={13} />
                        </button>
                      ) : null}
                      <button
                        type="button"
                        className="chat-rail__project-action"
                        aria-label={t('Создать чат в проекте {name}').replace('{name}', name)}
                        title={t('Новый чат в этом проекте')}
                        onClick={(event) => {
                          event.stopPropagation()
                          void startNewChatInGroup(group.path)
                        }}
                      >
                        <UiIcon name="plus" size={13} />
                      </button>
                    </div>
                  </div>
                  {expanded ? (
                    group.chats.length === 0 ? (
                      <p className="chat-rail__tree-empty">{t('Чатов пока нет. Начни с нового чата.')}</p>
                    ) : (
                      <ul className="chat-rail__project-chats">
                        {group.chats.map((chat) => {
                          const chatMenuOpen = openChatMenu?.key === chat.id
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
                                    onClick={() => void openChat(chat)}
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
                                        if (chatMenuOpen) {
                                          setOpenChatMenu(null)
                                          return
                                        }
                                        const rect = event.currentTarget.getBoundingClientRect()
                                        setOpenProjectMenu(null)
                                        setOpenChatMenu({
                                          key: chat.id,
                                          left: rect.left,
                                          top: rect.bottom + 4,
                                          anchorTop: rect.top
                                        })
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
                            </li>
                          )
                        })}
                      </ul>
                    )
                  ) : null}
                </li>
              )
            })}
          </ul>
        )}
      </section>
      {openProjectMenu && openProjectTarget !== null && openProjectTarget.path !== null
        ? createPortal(
            <div
              ref={projectMenuRef}
              className="chat-rail__project-menu chat-rail__project-menu--floating"
              role="menu"
              aria-label={t('Действия с проектом {name}').replace('{name}', basename(openProjectTarget.path))}
              style={{ top: openProjectMenu.top, left: openProjectMenu.left }}
            >
              <button
                type="button"
                role="menuitem"
                onClick={() => void openWorkspaceInExplorer(openProjectTarget.path!)}
              >
                <UiIcon name="folder" size={13} />
                <span>{t('Открыть в проводнике')}</span>
              </button>
              <button
                type="button"
                role="menuitem"
                className="chat-rail__project-menu-danger"
                onClick={() => void forgetWorkspace(openProjectTarget.path!)}
              >
                <UiIcon name="x" size={13} />
                <span>{t('Удалить из списка')}</span>
              </button>
            </div>,
            document.body
          )
        : null}
      {openChatMenu && openChatTarget
        ? createPortal(
            <div
              ref={chatMenuRef}
              className="chat-rail__project-menu chat-rail__project-menu--floating"
              role="menu"
              aria-label={t('Действия с чатом {title}').replace('{title}', openChatTarget.title)}
              style={{ top: openChatMenu.top, left: openChatMenu.left }}
            >
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setOpenChatMenu(null)
                  setRenamingChatId(openChatTarget.id)
                  setRenameDraft(openChatTarget.title)
                }}
              >
                <UiIcon name="chat" size={13} />
                <span>{t('Переименовать')}</span>
              </button>
              <button
                type="button"
                role="menuitem"
                onClick={() => void archiveChat(openChatTarget.id)}
              >
                <UiIcon name="archive" size={13} />
                <span>{t('Архивировать')}</span>
              </button>
              <button
                type="button"
                role="menuitem"
                className="chat-rail__project-menu-danger"
                onClick={() => void removeChat(openChatTarget.id)}
              >
                <UiIcon name="x" size={13} />
                <span>{t('Удалить чат')}</span>
              </button>
            </div>,
            document.body
          )
        : null}
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

/** One expandable node of the sidebar tree: a project (or standalone chats) with its chats. */
interface ChatGroup {
  /** Lowercase path key; '' marks the standalone group. */
  readonly key: string
  /** null — чаты без проекта; для таких чатов папка не выбирается. */
  readonly path: string | null
  readonly chats: readonly ChatSummary[]
  /** false — папка числится в списке, но недоступна на диске. */
  readonly available: boolean
}

/**
 * Groups chats by their project in list order: known projects first, then
 * chats of forgotten folders (kept reachable instead of disappearing), then
 * standalone chats.
 */
function buildChatGroups(projects: readonly WorkspaceOption[], chats: readonly ChatSummary[]): ChatGroup[] {
  const byKey = new Map<string, ChatSummary[]>()
  const standalone: ChatSummary[] = []
  for (const chat of chats) {
    if (chat.workspacePath === null) {
      standalone.push(chat)
      continue
    }
    const key = chat.workspacePath.toLowerCase()
    const bucket = byKey.get(key)
    if (bucket) bucket.push(chat)
    else byKey.set(key, [chat])
  }
  const groups: ChatGroup[] = projects.map((project) => ({
    key: project.path.toLowerCase(),
    path: project.path,
    available: project.available,
    chats: byKey.get(project.path.toLowerCase()) ?? []
  }))
  for (const [key, groupChats] of byKey) {
    if (projects.some((project) => project.path.toLowerCase() === key)) continue
    const path = groupChats[0]?.workspacePath
    if (path === null || path === undefined) continue
    groups.push({ key, path, available: true, chats: groupChats })
  }
  if (standalone.length > 0) {
    groups.push({ key: '', path: null, available: true, chats: standalone })
  }
  return groups
}
