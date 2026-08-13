import { useEffect, useState } from 'react'

import type { ConnectionState, CoreEvent, ShellState } from '@shared/api'

import { useShellApi } from './shell-api'
import { WorkspacePicker } from './WorkspacePicker'
import { TaskTimeline } from './TaskTimeline'
import { DeveloperTools } from './DeveloperTools'
import { SafetyPanel } from './SafetyPanel'

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

export function App(): React.JSX.Element {
  const [state, setState] = useState<ShellState | null>(null)
  const [events, setEvents] = useState<readonly CoreEvent[]>([])
  const [apiMissing, setApiMissing] = useState(false)

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

  if (apiMissing) {
    return (
      <main className="shell shell--recovery">
        <h1>Оболочка недоступна</h1>
        <p>Мост preload не загрузился. Перезапусти приложение.</p>
      </main>
    )
  }

  const connection = state?.connection ?? 'starting'

  return (
    <main className="shell">
      <header className="shell__header">
        <h1>EvoHime</h1>
        <p className={`shell__status shell__status--${connection}`}>{STATE_LABELS[connection]}</p>
      </header>

      <section className="shell__panel">
        <dl>
          <dt>Протокол</dt>
          <dd>{state?.protocol ? `v${state.protocol.major}.${state.protocol.minor}` : '—'}</dd>
          <dt>Версия Core</dt>
          <dd>{state?.coreVersion ?? '—'}</dd>
          <dt>Последняя последовательность</dt>
          <dd>{state?.lastSequence ?? 0}</dd>
          <dt>Попыток переподключения</dt>
          <dd>{state?.reconnectAttempts ?? 0}</dd>
        </dl>
        {state?.reason ? <p className="shell__reason">Причина: {state.reason}</p> : null}
      </section>

      <WorkspacePicker connection={connection} />

      <TaskTimeline connection={connection} events={events} />
      <DeveloperTools connection={connection} events={events} />
      <SafetyPanel connection={connection} events={events} />
    </main>
  )
}
