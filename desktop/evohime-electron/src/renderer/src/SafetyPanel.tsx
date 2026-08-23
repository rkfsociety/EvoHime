import { useCallback, useEffect, useMemo, useState } from 'react'

import type {
  AmbientEpisodeSummary,
  AmbientStatus,
  ConnectionState,
  CoreEvent,
  ListeningState
} from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Панель безопасности: режимы отдельных capability и что слушание успело
 * сделать за последний час.
 *
 * Панель соседствует с `PermissionModePicker` и не подменяет его: общий режим
 * доступа по-прежнему меняется там. Микрофон отделён намеренно —
 * `set_all_modes` в ядре не трогает `microphone_listen`, поэтому смена общего
 * режима не может втихую открыть микрофон (инвариант 04.1). Здесь это видно
 * глазами, а не только в тесте ядра.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

const HOUR_MS = 60 * 60 * 1000

/**
 * Capability, которыми управляет общий режим доступа.
 *
 * Отдельной команды чтения их режимов в протоколе нет, поэтому панель не
 * выдумывает значения: она называет сами разрешения и говорит, чем они
 * управляются.
 */
const SHARED_CAPABILITIES: readonly { readonly id: string; readonly label: string }[] = [
  { id: 'filesystem_read', label: 'Чтение файлов рабочей области' },
  { id: 'filesystem_write', label: 'Запись файлов рабочей области' },
  { id: 'shell_execute', label: 'Запуск команд' },
  { id: 'git_read', label: 'Чтение Git' },
  { id: 'git_write', label: 'Запись в Git' },
  { id: 'browser_access', label: 'Доступ в сеть' },
  { id: 'mcp_call', label: 'Вызовы MCP' },
  { id: 'memory_search', label: 'Поиск по памяти' }
]

/** Состояния, в которых микрофон открыт или открывается. */
const LIVE_STATES: readonly ListeningState[] = ['listening', 'starting']

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

// `events` holds the newest event first (App.tsx prepends on receipt), so
// the latest match is the FIRST one found here — not the last.
function latestPayload<T>(events: readonly CoreEvent[], eventType: string): T | null {
  const event = events.find((item) => item.eventType === eventType)
  if (!event) return null
  try {
    return JSON.parse(event.payload) as T
  } catch {
    return null
  }
}

export function SafetyPanel({ connection, events }: Props): React.JSX.Element {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [notice, setNotice] = useState<string | null>(null)
  const [nowMs, setNowMs] = useState(() => Date.now())

  const status = latestPayload<AmbientStatus>(events, 'ambient.status')
  const stateEvent = latestPayload<{ state: ListeningState }>(events, 'ambient.state')
  const episodes =
    latestPayload<{ episodes: readonly AmbientEpisodeSummary[] }>(events, 'ambient.episodes')
      ?.episodes ?? []

  const state: ListeningState | null = stateEvent?.state ?? status?.state ?? null

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('ambient.getStatus', {})
    void api.invoke('ambient.listEpisodes', { limit: 100 })
  }, [api, connected])

  // Окно «за последний час» двигается само: замороженное на момент открытия
  // оно тихо разъезжалось бы с тем, что показывает панель.
  useEffect(() => {
    const timer = setInterval(() => setNowMs(Date.now()), 30_000)
    return () => clearInterval(timer)
  }, [])

  const utterancesLastHour = useMemo(
    () =>
      episodes
        .filter((episode) => nowMs - episode.started_at_ms <= HOUR_MS)
        .reduce((total, episode) => total + episode.utterance_count, 0),
    [episodes, nowMs]
  )
  const proposalsLastHour = events.filter((event) => event.eventType === 'ambient.proposal').length

  const setMicrophone = useCallback(
    async (enabled: boolean) => {
      if (!api) return
      const outcome = await api.invoke('ambient.setListening', { enabled, paused: false })
      if (!outcome.ok) setNotice(outcome.message)
    },
    [api]
  )

  const microphoneOn = state !== null && LIVE_STATES.includes(state)

  return (
    <section className="settings-info safety" aria-label="Безопасность">
      <h3>Безопасность</h3>
      <p>
        Секреты провайдера шифруются средствами Windows и не возвращаются в интерфейс после
        сохранения.
      </p>

      <h4>Разрешения по отдельности</h4>
      <ul className="safety__capabilities">
        <li className="safety__capability safety__capability--microphone">
          <span>
            <strong>Постоянное слушание микрофона</strong>
            <small>
              Отдельное разрешение. Смена общего режима доступа его не трогает: включить микрофон
              можно только здесь или в панели «Слух».
            </small>
          </span>
          <button
            type="button"
            aria-pressed={microphoneOn}
            disabled={!api || !connected}
            onClick={() => void setMicrophone(!microphoneOn)}
          >
            {microphoneOn ? 'Выключить' : 'Включить'}
          </button>
        </li>
        {SHARED_CAPABILITIES.map((capability) => (
          <li key={capability.id} className="safety__capability">
            <span>
              <strong>{capability.label}</strong>
              <small>следует общему режиму доступа над полем ввода</small>
            </span>
          </li>
        ))}
      </ul>
      {notice ? (
        <p className="listening__error" role="alert">
          {notice}
        </p>
      ) : null}

      <h4>За последний час</h4>
      <p role="status">
        высказываний: {utterancesLastHour} · кандидатов памяти: не подключено · предложений:{' '}
        {proposalsLastHour}
      </p>
      <p>
        Кандидаты памяти из услышанного появятся вместе с мостом ambient-памяти; пока такого
        источника нет, и показывать здесь ноль значило бы утверждать, что он работает и ничего не
        нашёл.
      </p>
    </section>
  )
}
