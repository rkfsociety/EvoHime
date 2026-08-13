import { useCallback, useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, ShellState, WorkspaceSelection } from '@shared/api'

import { useShellApi } from './shell-api'
import { WorkspacePicker } from './WorkspacePicker'
import { TaskTimeline } from './TaskTimeline'
import { DeveloperTools } from './DeveloperTools'
import { EditorPanel } from './EditorPanel'
import { SafetyPanel } from './SafetyPanel'
import { ProviderForm } from './ProviderForm'
import { TerminalPanel } from './TerminalPanel'

/**
 * Stage 0 shell surface: it only renders the connection state owned by the main
 * process. No business logic lives here, and the renderer never touches the
 * workspace, the pipe or a shell (plan 0, rule 2 of AGENTS.md).
 */

const MAX_VISIBLE_EVENTS = 50

const STATE_LABELS: Record<ConnectionState, string> = {
  starting: 'Запуск',
  connecting: 'Подключение к Core',
  connected: 'Подключено',
  reconnecting: 'Переподключение',
  replaying: 'Восстановление событий',
  resyncing: 'Синхронизация состояния',
  'state-gap': 'Пробел в состоянии — нужна пересинхронизация',
  'version-mismatch': 'Несовместимая версия IPC',
  degraded: 'Ограниченный режим',
  fatal: 'Критическая ошибка'
}

type ViewId = 'chat' | 'files' | 'editor' | 'terminal' | 'safety' | 'settings'

interface ViewDescriptor {
  readonly id: ViewId
  readonly label: string
  readonly icon: string
  readonly group: 'work' | 'tools'
}

const VIEWS: readonly ViewDescriptor[] = [
  { id: 'chat', label: 'Диалог', icon: '◆', group: 'work' },
  { id: 'files', label: 'Файлы и Git', icon: '▤', group: 'tools' },
  { id: 'editor', label: 'Редактор', icon: '✎', group: 'tools' },
  { id: 'terminal', label: 'Терминал', icon: '❯', group: 'tools' },
  { id: 'safety', label: 'Безопасность', icon: '◈', group: 'tools' },
  { id: 'settings', label: 'Настройки', icon: '⚙', group: 'tools' }
]

export function App(): React.JSX.Element {
  const [state, setState] = useState<ShellState | null>(null)
  const [events, setEvents] = useState<readonly CoreEvent[]>([])
  const [apiMissing, setApiMissing] = useState(false)
  const [view, setView] = useState<ViewId>('chat')
  const [workspace, setWorkspace] = useState<string | null>(null)

  const api = useShellApi()

  useEffect(() => {
    if (!api) {
      setApiMissing(true)
      return
    }

    const unsubscribe = api.subscribe((event) => {
      if (event.kind === 'state') {
        setState(event.state)
        return
      }
      setEvents((current) => [event.event, ...current].slice(0, MAX_VISIBLE_EVENTS))
    })

    void api.invoke('shell.getState', {}).then((outcome) => {
      if (outcome.ok) {
        setState(outcome.value)
      }
    })

    return unsubscribe
  }, [api])

  const trackWorkspace = useCallback((selection: WorkspaceSelection) => {
    setWorkspace(selection.selected)
  }, [])

  // Ожидающее разрешение подсвечивается в навигации: пользователь может стоять
  // в другом разделе, когда Core просит подтверждение.
  const pendingApprovals = useMemo(
    () => events.filter((event) => event.eventType === 'approval.required').length,
    [events]
  )

  if (apiMissing) {
    return (
      <main className="shell shell--recovery">
        <h1>Оболочка недоступна</h1>
        <p>Мост preload не загрузился. Перезапусти приложение.</p>
      </main>
    )
  }

  const connection = state?.connection ?? 'starting'
  const activeView = VIEWS.find((item) => item.id === view)
  const title = activeView?.label ?? 'Диалог'

  return (
    <div className="shell">
      <nav className="sidebar" aria-label="Разделы">
        <div className="sidebar__brand">
          <span className="sidebar__logo" aria-hidden="true">E</span>
          <h1 className="sidebar__title">EvoHime</h1>
        </div>

        <div className="sidebar__nav">
          {VIEWS.filter((item) => item.group === 'work').map((item) => (
            <NavItem
              key={item.id}
              view={item}
              active={item.id === view}
              badge={item.id === 'chat' && pendingApprovals > 0 ? pendingApprovals : 0}
              onSelect={setView}
            />
          ))}
        </div>

        <p className="sidebar__section">Инструменты</p>
        <div className="sidebar__nav">
          {VIEWS.filter((item) => item.group === 'tools').map((item) => (
            <NavItem key={item.id} view={item} active={item.id === view} badge={0} onSelect={setView} />
          ))}
        </div>

        <div className="sidebar__workspace">
          <WorkspacePicker connection={connection} onSelectionChange={trackWorkspace} />
        </div>
      </nav>

      <main className="main">
        <header className="topbar">
          <h2 className="topbar__title">{title}</h2>
          <span className="topbar__path">{workspace ?? 'папка не выбрана'}</span>
          <span className="topbar__spacer" />
          <span className={`status-pill status-pill--${connection}`}>{STATE_LABELS[connection]}</span>
        </header>

        <div className="main__body">
          {view === 'chat' ? (
            <TaskTimeline connection={connection} events={events} />
          ) : (
            <div className="main__scroll">
              {view === 'files' ? <DeveloperTools connection={connection} events={events} /> : null}
              {view === 'editor' ? <EditorPanel connection={connection} events={events} /> : null}
              {view === 'terminal' ? <TerminalPanel connection={connection} events={events} /> : null}
              {view === 'safety' ? <SafetyPanel connection={connection} events={events} /> : null}
              {view === 'settings' ? <ProviderForm /> : null}
            </div>
          )}
        </div>
      </main>

      <footer className="statusbar">
        <span>Протокол {state?.protocol ? `v${state.protocol.major}.${state.protocol.minor}` : '—'}</span>
        <span>Core {state?.coreVersion ?? '—'}</span>
        <span>seq {state?.lastSequence ?? 0}</span>
        {(state?.reconnectAttempts ?? 0) > 0 ? <span>переподключений: {state?.reconnectAttempts}</span> : null}
        <span className="statusbar__spacer" />
        {state?.reason ? <span className="statusbar__reason">{state.reason}</span> : null}
      </footer>
    </div>
  )
}

interface NavItemProps {
  readonly view: ViewDescriptor
  readonly active: boolean
  readonly badge: number
  readonly onSelect: (id: ViewId) => void
}

function NavItem({ view, active, badge, onSelect }: NavItemProps): React.JSX.Element {
  return (
    <button
      type="button"
      className="nav-item"
      aria-current={active ? 'page' : undefined}
      onClick={() => onSelect(view.id)}
    >
      <span className="nav-item__icon" aria-hidden="true">{view.icon}</span>
      {view.label}
      {badge > 0 ? <span className="nav-item__badge">{badge}</span> : null}
    </button>
  )
}
