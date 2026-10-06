import { useCallback, useEffect, useMemo, useState } from 'react'

import type {
  AmbientEpisodeSummary,
  AmbientHotkeyStatus,
  AmbientPolicy,
  AmbientProposalList,
  AmbientQuietHours,
  AmbientStatus,
  AmbientUtterance,
  ConnectionState,
  CoreEvent,
  ListeningReason,
  ListeningState,
  VoiceCommandList,
  VoiceCommandResolved
} from '@shared/api'
import {
  initialListenerRuntimeStatus,
  optionalFileLabel,
  type ListenerRuntimeStatus
} from '@shared/listener-runtime'

import { useShellApi } from './shell-api'
import { formatDateTime, translate} from './i18n'
import { useT } from './i18n'

/**
 * Панель «Слух» — полная пользовательская поверхность постоянного слушания
 * (план 04.5).
 *
 * Панель ничего не решает сама: она отправляет команды и рисует состояние,
 * которое прислало ядро. Собственной копии состояния у неё нет — иначе трей,
 * хоткей и панель разошлись бы, и одна из трёх точек входа врала бы.
 *
 * Подписи локализуются renderer-слоем; runtime-состояние остаётся владельцем
 * Core и не зависит от выбранного языка интерфейса.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

/** Окно «забыть последние N минут». */
const FORGET_WINDOW_MINUTES = 5

export const STATE_TITLES: Record<ListeningState, string> = {
  stopped: 'Слушание выключено',
  starting: 'Слушание запускается',
  listening: 'Ева слушает',
  paused_by_user: 'Микрофон на паузе',
  paused_by_policy: 'Микрофон закрыт политикой',
  device_conflict: 'Микрофон занят',
  device_disconnected: 'Микрофон отключён',
  engine_unavailable: 'Слушание: проверка состояния…',
  denied: 'Слушание запрещено'
}

export const REASON_TEXTS: Record<ListeningReason, string> = {
  user_request: 'по решению пользователя',
  quiet_hours: 'идут тихие часы',
  blocklist: 'активное окно в чёрном списке',
  stop_word: 'прозвучало стоп-слово',
  permission_denied: 'доступ к микрофону не разрешён',
  device_conflict: 'устройство занято другим приложением',
  device_disconnected: 'выбранное устройство пропало',
  engine_unavailable: 'нет связи с процессом слушателя или движком распознавания',
  engine_degraded: 'распознавание не укладывается в бюджет даже на лёгкой модели',
  system_sleep: 'система уходила в сон',
  storage_failed: 'сбой локального хранилища',
  unknown: 'причина не сообщена'
}

/**
 * Строки кодов ошибок. Неизвестный код показывается общей фразой и никогда
 * не трактуется как успешная смена состояния.
 */
export const ERROR_TEXTS: Record<string, string> = {
  LISTENER_UNAVAILABLE: 'Процесс слушателя недоступен. Настройка сохранена и применится, когда он поднимется.',
  DEVICE_CONFLICT: 'Микрофон занят другим приложением.',
  DEVICE_DISCONNECTED: 'Выбранное устройство недоступно. Выбери другой микрофон.',
  PERMISSION_DENIED: 'Доступ к микрофону не разрешён.',
  POLICY_INVALID: 'Политика не прошла проверку и не была применена.',
  ENGINE_NOT_READY: 'Движок распознавания не готов: установи его ниже на этой странице.',
  STORAGE_FAILED: 'Локальное хранилище недоступно.',
  CONFIRMATION_REQUIRED: 'Действие требует подтверждения.',
  INVALID_ARGUMENT: 'Ядро отвергло параметры команды.'
}

export function errorText(code: string): string {
  return ERROR_TEXTS[code] ?? 'Ошибка слушателя'
}

const EXTRACTION_LABELS: Record<string, string> = {
  disabled: 'извлечение выключено',
  pending: 'ожидает извлечения',
  done: 'извлечено',
  failed: 'извлечение не удалось'
}

const RUNTIME_LABELS: Record<ListenerRuntimeStatus['state'], string> = {
  unknown: 'не проверялось',
  missing: 'не установлено',
  ready: 'готово',
  'update-available': 'доступно обновление',
  downloading: 'загрузка',
  failed: 'ошибка'
}

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

// `events` holds the newest event first (App.tsx prepends on receipt), so
// the latest match is the FIRST one found here — not the last. Over a long
// session `events` is bounded (`MAX_VISIBLE_EVENTS`), and once an older
// occurrence of `eventType` gets evicted, `.filter().at(-1)` would silently
// start returning the oldest *surviving* match instead of the newest one.
function latestPayload<T>(events: readonly CoreEvent[], eventType: string): T | null {
  const event = events.find((item) => item.eventType === eventType)
  if (!event) return null
  try {
    return JSON.parse(event.payload) as T
  } catch {
    return null
  }
}

/** Минуты суток в «ЧЧ:ММ» для редактора тихих часов. */
export function minutesToClock(minutes: number): string {
  const bounded = Math.max(0, Math.min(1439, Math.trunc(minutes)))
  return `${String(Math.floor(bounded / 60)).padStart(2, '0')}:${String(bounded % 60).padStart(2, '0')}`
}

/** «ЧЧ:ММ» обратно в минуты суток; `null` для непригодной строки. */
export function clockToMinutes(value: string): number | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(value.trim())
  if (!match) return null
  const hours = Number(match[1])
  const minutes = Number(match[2])
  if (hours > 23 || minutes > 59) return null
  return hours * 60 + minutes
}

function formatTime(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return 'время неизвестно'
  return formatDateTime(ms)
}

function formatDuration(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000))
  if (seconds < 60) return `${seconds} с`
  return `${Math.floor(seconds / 60)} мин ${seconds % 60} с`
}

export function ListeningPanel({ connection, events }: Props): React.JSX.Element {
  const t = useT()

  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)

  const [hotkey, setHotkey] = useState<AmbientHotkeyStatus | null>(null)
  const [runtime, setRuntime] = useState<ListenerRuntimeStatus>(() =>
    initialListenerRuntimeStatus('')
  )
  const [runtimeBusy, setRuntimeBusy] = useState(false)
  const [expanded, setExpanded] = useState<string | null>(null)
  const [confirming, setConfirming] = useState<'forget' | 'all' | null>(null)
  const [notice, setNotice] = useState<string | null>(null)
  const [policyDraft, setPolicyDraft] = useState<AmbientPolicy | null>(null)

  const status = latestPayload<AmbientStatus>(events, 'ambient.status')
  const stateEvent = latestPayload<{ state: ListeningState; reason: ListeningReason }>(
    events,
    'ambient.state'
  )
  const stateEventCount = events.filter((item) => item.eventType === 'ambient.state').length
  const listeningResult = latestPayload<{ state: ListeningState; error_code: string }>(
    events,
    'ambient.listening'
  )
  const episodeList = latestPayload<{ episodes: readonly AmbientEpisodeSummary[] }>(
    events,
    'ambient.episodes'
  )
  const episodeDetail = latestPayload<{
    episode_id: string
    utterances: readonly AmbientUtterance[]
  }>(events, 'ambient.episode')
  const proposalList = latestPayload<AmbientProposalList>(events, 'ambient.proposals')
  const voiceCommandList = latestPayload<VoiceCommandList>(events, 'ambient.voice_commands')
  const voiceResolved = latestPayload<VoiceCommandResolved>(events, 'ambient.voice_command_resolved')
  // Событие журнала несёт только ключ приложения: заголовок читается списком,
  // поэтому панель перечитывает очередь на каждое такое событие.
  const voiceCommandEvent = events.filter((item) => item.eventType === 'ambient.voice_command').length
  const storedPolicy = latestPayload<AmbientPolicy>(events, 'ambient.policy')
  const policySaved = latestPayload<{ applied: boolean; error_code: string }>(
    events,
    'ambient.policy_saved'
  )
  const deleted = latestPayload<{ deleted_count: number; error_code: string }>(
    events,
    'ambient.deleted'
  )
  const forgotten = latestPayload<{ deleted_count: number; error_code: string }>(
    events,
    'ambient.forgotten'
  )
  const transcriptEvent = events.filter((item) => item.eventType === 'ambient.transcript').length
  const retentionEvent = events.filter((item) => item.eventType === 'ambient.retention').length

  // Состояние берётся из последнего события, а снимок статуса — из ответа на
  // явный запрос. Если ни того, ни другого нет, состояние неизвестно: панель
  // говорит именно это, а не «выключено».
  const state: ListeningState | null = stateEvent?.state ?? status?.state ?? null
  const reason: ListeningReason | null = stateEvent?.reason ?? status?.reason ?? null
  const devices = status?.devices ?? []

  const refresh = useCallback(() => {
    if (!api || !connected) return
    void api.invoke('ambient.getStatus', {})
    void api.invoke('ambient.listEpisodes', { limit: 50 })
    void api.invoke('ambient.getPolicy', {})
    void api.invoke('ambient.listProposals', { limit: 50 })
    void api.invoke('ambient.listVoiceCommands', {})
  }, [api, connected])

  // При открытии панель сперва спрашивает состояние: полагаться на то, что
  // событие застало окно открытым, нельзя.
  useEffect(() => {
    refresh()
  }, [refresh])

  // Listener sends the device snapshot immediately before its state event.
  // The first status request can race that handshake, so reread the full
  // status when a fresh listener state arrives; otherwise the panel remains
  // stuck on an empty device list until the user clicks refresh.
  useEffect(() => {
    if (!api || !connected || stateEventCount === 0) return
    void api.invoke('ambient.getStatus', {})
  }, [api, connected, stateEventCount])

  // Новый транскрипт или удаление меняют список эпизодов — он перечитывается,
  // а не досочиняется на месте.
  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('ambient.listEpisodes', { limit: 50 })
  }, [api, connected, transcriptEvent, retentionEvent])

  useEffect(() => {
    if (!api || !connected) return
    void api.invoke('ambient.listVoiceCommands', {})
  }, [api, connected, voiceCommandEvent])

  useEffect(() => {
    if (!api) return
    void api.invoke('ambient.hotkeyStatus', {}).then((outcome) => {
      if (outcome.ok) setHotkey(outcome.value)
    })
    void api.invoke('listener.getRuntimeStatus', {}).then((outcome) => {
      if (outcome.ok) setRuntime(outcome.value)
    })
    return api.subscribe((event) => {
      if (event.kind === 'listener-runtime') setRuntime(event.status)
    })
  }, [api])

  useEffect(() => {
    if (storedPolicy && policyDraft === null) setPolicyDraft(storedPolicy)
  }, [storedPolicy, policyDraft])

  const setListening = useCallback(
    async (enabled: boolean, paused: boolean, deviceId = '') => {
      if (!api) return
      const outcome = await api.invoke('ambient.setListening', { enabled, paused, deviceId })
      if (!outcome.ok) setNotice(outcome.message)
    },
    [api]
  )

  const runRuntime = useCallback(
    async (command: 'listener.checkRuntime' | 'listener.downloadRuntime') => {
      if (!api || runtimeBusy) return
      setRuntimeBusy(true)
      const outcome = await api.invoke(command, {})
      if (outcome.ok) setRuntime(outcome.value)
      setRuntimeBusy(false)
    },
    [api, runtimeBusy]
  )

  const confirmDestructive = useCallback(async () => {
    if (!api || confirming === null) return
    const outcome =
      confirming === 'forget'
        ? await api.invoke('ambient.forgetWindow', {
            windowMs: FORGET_WINDOW_MINUTES * 60 * 1000,
            confirmed: true
          })
        : await api.invoke('ambient.deleteTranscripts', { all: true, confirmed: true })
    if (!outcome.ok) setNotice(outcome.message)
    setConfirming(null)
    setExpanded(null)
  }, [api, confirming])

  const resolveVoiceCommand = useCallback(
    async (commandId: string, accepted: boolean) => {
      if (!api) return
      const outcome = await api.invoke('ambient.resolveVoiceCommand', { commandId, accepted })
      if (!outcome.ok) setNotice(outcome.message)
    },
    [api]
  )

  const savePolicy = useCallback(async () => {
    if (!api || policyDraft === null) return
    const outcome = await api.invoke('ambient.savePolicy', {
      quietHours: policyDraft.quiet_hours.map((window) => ({
        startMinute: window.start_minute,
        endMinute: window.end_minute
      })),
      blocklistPatterns: policyDraft.blocklist_patterns,
      windowTitleBlocklist: policyDraft.window_title_blocklist,
      retentionDays: policyDraft.retention_days,
      voiceCommands: policyDraft.voice_commands,
      voiceCommandsAutorun: policyDraft.voice_commands_autorun
    })
    if (!outcome.ok) setNotice(outcome.message)
  }, [api, policyDraft])

  const episodes = episodeList?.episodes ?? []
  const voiceCommands = voiceCommandList?.commands ?? []
  const failure = useMemo(() => {
    const codes = [
      listeningResult?.error_code,
      policySaved?.applied === false ? policySaved.error_code : undefined,
      deleted?.error_code,
      forgotten?.error_code
    ].filter((code): code is string => typeof code === 'string' && code.length > 0)
    return codes.at(-1) ?? null
  }, [listeningResult, policySaved, deleted, forgotten])

  const paused = state === 'paused_by_user'
  const active = state === 'listening' || state === 'starting'

  return (
    <section className="listening" aria-label={t("Слух")}>
      <header className="listening__header">
        <h3 className={`listening__state listening__state--${state ?? 'unknown'}`}>
          <span aria-hidden="true">{active ? '🎙' : state === null || state === 'engine_unavailable' ? '⚠️' : '⏸'}</span>{' '}
          {state === null ? t('Слушание: проверка состояния…') : t(STATE_TITLES[state])}
        </h3>
        <p className="listening__reason">
          {reason === null ? t('Ядро ещё не сообщило состояние.') : t(REASON_TEXTS[reason])}
        </p>
      </header>

      <div className="listening__actions">
        <button type="button" disabled={!api || !connected} onClick={() => void setListening(true, false)}>
          {t("Включить слушание")}
        </button>
        <button type="button" disabled={!api || !connected || !active} onClick={() => void setListening(true, true)}>
          {t("Пауза")}
        </button>
        <button type="button" disabled={!api || !connected || !paused} onClick={() => void setListening(true, false)}>
          {t("Продолжить")}
        </button>
        <button type="button" disabled={!api || !connected} onClick={() => void setListening(false, false)}>
          {t("Выключить")}
        </button>
        <button type="button" disabled={!api || !connected} onClick={refresh}>
          {t("Обновить состояние")}
        </button>
      </div>

      <p className="listening__hotkey" role="status">
        {hotkey === null
          ? t('Доступность глобального хоткея выясняется.')
          : hotkey.registered
            ? t(`Глобальный хоткей паузы: ${hotkey.combination}.`)
            : t(`Глобальный хоткей ${hotkey.combination} занят другим приложением и недоступен. Пользуйся треем или этой панелью.`)}
      </p>

      {failure ? (
        <p className="listening__error" role="alert">
          {t(errorText(failure))}
        </p>
      ) : null}
      {notice ? (
        <p className="listening__error" role="alert">
          {notice}
        </p>
      ) : null}

      <section className="listening__block" aria-label={t("Микрофон")}>
        <h4>{t("Микрофон")}</h4>
        {devices.length === 0 ? (
          <p>{t("Устройства захвата не найдены. Проверь, подключён ли микрофон.")}</p>
        ) : (
          <ul className="listening__devices">
            {devices.map((device) => (
              <li key={device.device_id}>
                <button
                  type="button"
                  aria-pressed={device.is_active}
                  disabled={!api || !connected}
                  onClick={() => void setListening(true, paused, device.device_id)}
                >
                  <span aria-hidden="true">{device.is_active ? '●' : '○'}</span> {device.display_name}
                  {device.is_default ? translate(" · по умолчанию") : ''}
                </button>
              </li>
            ))}
          </ul>
        )}
        {status && !status.watching_devices ? (
          <p className="listening__warning">
            {t("Подписка на смену устройств не поднялась: список не обновится сам, нажми «Обновить состояние» после подключения микрофона.")}
          </p>
        ) : null}
      </section>

      <section className="listening__block" aria-label={t("Распознавание речи")}>
        <h4>{t("Движок распознавания")}</h4>
        <p>
          {t("Состояние набора:")} {t(RUNTIME_LABELS[runtime.state])}
          {runtime.installedVersion ? ` (${runtime.installedVersion})` : ''}{t(". Пока набор не установлен, слушание не включится.")}
        </p>
        <p role="status">{t(runtime.message)}</p>
        {runtime.state === 'downloading' ? (
          <progress value={runtime.progressPct} max={100} aria-label={t("Ход загрузки распознавания речи")} />
        ) : null}
        {runtime.missingOptional.length > 0 ? (
          <p className="listening__warning">
            {t("Не установлено:")} {runtime.missingOptional.map(optionalFileLabel).join(', ')}{t(". Обнаружение речи остаётся энергетическим.")}
          </p>
        ) : null}
        <div className="listening__actions">
          <button type="button" disabled={!api || runtimeBusy} onClick={() => void runRuntime('listener.checkRuntime')}>
            {t("Проверить")}
          </button>
          <button
            type="button"
            disabled={!api || runtimeBusy || runtime.state === 'downloading' || runtime.state === 'ready'}
            onClick={() => void runRuntime('listener.downloadRuntime')}
          >
            {runtime.state === 'update-available' ? translate("Обновить") : translate("Установить")}
          </button>
        </div>
        {status ? (
          <p>
            {t("Движок в процессе слушателя:")} {status.engine_ready ? translate("открыт") : translate("не открыт")}
            {status.engine_version ? ` · ${status.engine_version}` : ''}.
          </p>
        ) : null}
      </section>

      <section className="listening__block" aria-label={t("Проактивность")}>
        <h4>{t("Предложения")}</h4>
        <p>
          {t("По услышанному Ева может предложить задачу или напоминание — и больше ничего. Ни запуска задач, ни вызова инструментов, ни записи файлов, ни сети без твоего клика: это инвариант ядра, а не настройка на этой странице.")}
        </p>
        {proposalList ? (
          <p role="status">
            {t("Ждут решения:")} {proposalList.proposals.length}{t(". Потолок — не больше")}{' '}
            {proposalList.max_per_hour} {t("в час и")} {proposalList.max_per_day} {t("в сутки, не чаще одного раз в")} {Math.round(proposalList.min_interval_ms / 60000)} {t("минут. Сверх потолка предложение отбрасывается, а не копится в очередь. Решать карточки — во вкладке «Память и автоматизация».")}
          </p>
        ) : (
          <p>{t("Состояние предложений ещё не получено.")}</p>
        )}
      </section>

      <section className="listening__block" aria-label={t("Услышанные команды")}>
        <h4>{t("Голосовые команды")}</h4>
        <p>
          {t("Обращение по имени и глагол: «Ева, открой хром». Без имени команды нет — разговор рядом с микрофоном ничего не запускает. Открывается только приложение из каталога, и по умолчанию — после клика: услышанное само по себе не является подтверждением.")}
        </p>
        {voiceCommands.length === 0 ? (
          <p>{t("Команд, ждущих решения, нет.")}</p>
        ) : (
          <ul className="listening__voice-commands">
            {voiceCommands.map((command) => (
              <li key={command.command_id}>
                <span>{t("Открыть")} {command.title}?</span>
                <div className="listening__actions">
                  <button
                    type="button"
                    disabled={!api || !connected}
                    onClick={() => void resolveVoiceCommand(command.command_id, true)}
                  >
                    {t("Открыть")}
                  </button>
                  <button
                    type="button"
                    disabled={!api || !connected}
                    onClick={() => void resolveVoiceCommand(command.command_id, false)}
                  >
                    {t("Не надо")}
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}
        {voiceCommandList && !voiceCommandList.requires_confirmation ? (
          <p role="status">
            {t("Автозапуск включён в политике: услышанная команда открывает приложение сразу, без карточки.")}
          </p>
        ) : null}
        {voiceResolved && voiceResolved.error_code === 'launch_failed' ? (
          <p role="status">{t("Не удалось открыть приложение. Подробности — в журнале ядра.")}</p>
        ) : null}
      </section>

      <section className="listening__block" aria-label={t("Удаление записанного")}>
        <h4>{t("Удаление")}</h4>
        <p>
          {t("Удаление необратимо. Текст исчезает из базы вместе со следами эпизода в журнале, но остаётся внутри резервных копий, снятых до удаления, пока они не состарятся.")}
        </p>
        <div className="listening__actions">
          <button type="button" disabled={!api || !connected} onClick={() => setConfirming('forget')}>
            {t("Забыть последние")} {FORGET_WINDOW_MINUTES} {t("минут")}
          </button>
          <button
            type="button"
            className="listening__danger"
            disabled={!api || !connected}
            onClick={() => setConfirming('all')}
          >
            {t("Удалить все транскрипты")}
          </button>
        </div>
        {deleted && !deleted.error_code ? <p role="status">{t("Удалено высказываний:")} {deleted.deleted_count}.</p> : null}
        {forgotten && !forgotten.error_code ? (
          <p role="status">{t("Забыто высказываний:")} {forgotten.deleted_count}.</p>
        ) : null}
      </section>

      <section className="listening__block" aria-label={t("Эпизоды")}>
        <h4>{t("Эпизоды")}</h4>
        {episodes.length === 0 ? (
          <p>{t("Записанных эпизодов нет.")}</p>
        ) : (
          <ul className="listening__episodes">
            {episodes.map((episode) => (
              <li key={episode.episode_id}>
                <div className="listening__episode-row">
                  <span>{formatTime(episode.started_at_ms)}</span>
                  <span>{t("речь")} {formatDuration(episode.speech_duration_ms)}</span>
                  <span>{t("высказываний:")} {episode.utterance_count}</span>
                  <span>{t(EXTRACTION_LABELS[episode.extraction_state] ?? episode.extraction_state)}</span>
                  <button
                    type="button"
                    disabled={!api || !connected}
                    aria-expanded={expanded === episode.episode_id}
                    onClick={() => {
                      if (expanded === episode.episode_id) {
                        setExpanded(null)
                        return
                      }
                      setExpanded(episode.episode_id)
                      void api?.invoke('ambient.getEpisode', { episodeId: episode.episode_id })
                    }}
                  >
                    {expanded === episode.episode_id ? translate("Скрыть текст") : translate("Показать текст")}
                  </button>
                </div>
                {expanded === episode.episode_id && episodeDetail?.episode_id === episode.episode_id ? (
                  <ol className="listening__utterances">
                    {episodeDetail.utterances.map((utterance) => (
                      <li key={utterance.utterance_id}>{utterance.text}</li>
                    ))}
                  </ol>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </section>

      {policyDraft ? (
        <PolicyEditor
          policy={policyDraft}
          disabled={!api || !connected}
          onChange={setPolicyDraft}
          onSave={() => void savePolicy()}
          onReset={() => setPolicyDraft(storedPolicy)}
          applied={policySaved?.applied === true}
        />
      ) : (
        <section className="listening__block" aria-label={t("Политика слушания")}>
          <h4>{t("Политика")}</h4>
          <p>{t("Политика ещё не загружена.")}</p>
        </section>
      )}

      {confirming ? (
        <div className="listening__modal" role="presentation">
          <div className="listening__modal-window" role="dialog" aria-modal="true" aria-labelledby="listening-confirm">
            <h4 id="listening-confirm">
              {confirming === 'forget'
                ? `Забыть последние ${FORGET_WINDOW_MINUTES} минут?`
                : translate("Удалить все транскрипты?")}
            </h4>
            <p>
              {t("Действие необратимо.")} {confirming === 'all' ? translate("Будут удалены все сохранённые эпизоды.") : ''}
            </p>
            <div className="listening__actions">
              <button type="button" onClick={() => setConfirming(null)}>
                {t("Отмена")}
              </button>
              <button type="button" className="listening__danger" onClick={() => void confirmDestructive()}>
                {t("Удалить")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </section>
  )
}

interface PolicyEditorProps {
  readonly policy: AmbientPolicy
  readonly disabled: boolean
  readonly applied: boolean
  readonly onChange: (policy: AmbientPolicy) => void
  readonly onSave: () => void
  readonly onReset: () => void
}

/**
 * Редактор политики. Проверку выполняет ядро: здесь нет второй, «своей»
 * валидации, которая могла бы разойтись с контрактом 04.1.
 */
function PolicyEditor({
  policy,
  disabled,
  applied,
  onChange,
  onSave,
  onReset
}: PolicyEditorProps): React.JSX.Element {
  const t = useT()
  const setQuiet = (index: number, next: Partial<AmbientQuietHours>): void => {
    onChange({
      ...policy,
      quiet_hours: policy.quiet_hours.map((window, position) =>
        position === index ? { ...window, ...next } : window
      )
    })
  }

  return (
    <section className="listening__block" aria-label={t("Политика слушания")}>
      <h4>{t("Политика")}</h4>

      <p>{t("Тихие часы: в эти окна поток микрофона закрывается целиком.")}</p>
      <ul className="listening__quiet">
        {policy.quiet_hours.map((window, index) => (
          <li key={`${window.start_minute}-${window.end_minute}-${index}`}>
            <label>
              {t("с")}
              <input
                type="time"
                value={minutesToClock(window.start_minute)}
                disabled={disabled}
                onChange={(event) => {
                  const minutes = clockToMinutes(event.target.value)
                  if (minutes !== null) setQuiet(index, { start_minute: minutes })
                }}
              />
            </label>
            <label>
              {t("до")}
              <input
                type="time"
                value={minutesToClock(window.end_minute)}
                disabled={disabled}
                onChange={(event) => {
                  const minutes = clockToMinutes(event.target.value)
                  if (minutes !== null) setQuiet(index, { end_minute: minutes })
                }}
              />
            </label>
            <button
              type="button"
              disabled={disabled}
              onClick={() =>
                onChange({
                  ...policy,
                  quiet_hours: policy.quiet_hours.filter((_, position) => position !== index)
                })
              }
            >
              {t("Убрать")}
            </button>
          </li>
        ))}
      </ul>
      <button
        type="button"
        disabled={disabled}
        onClick={() =>
          onChange({
            ...policy,
            quiet_hours: [...policy.quiet_hours, { start_minute: 23 * 60, end_minute: 7 * 60 }]
          })
        }
      >
        {t("Добавить окно тишины")}
      </button>

      <label className="listening__field">
        {t("Чёрный список процессов (по одному в строке)")}
        <textarea
          rows={4}
          disabled={disabled}
          value={policy.blocklist_patterns.join('\n')}
          onChange={(event) =>
            onChange({
              ...policy,
              blocklist_patterns: event.target.value
                .split('\n')
                .map((line) => line.trim())
                .filter((line) => line.length > 0)
            })
          }
        />
      </label>

      <label className="listening__field">
        {t("Чёрный список заголовков окон (по одному в строке)")}
        <textarea
          rows={4}
          disabled={disabled}
          value={policy.window_title_blocklist.join('\n')}
          onChange={(event) =>
            onChange({
              ...policy,
              window_title_blocklist: event.target.value
                .split('\n')
                .map((line) => line.trim())
                .filter((line) => line.length > 0)
            })
          }
        />
      </label>

      <label className="listening__toggle">
        <input
          type="checkbox"
          disabled={disabled}
          checked={policy.voice_commands}
          onChange={(event) => onChange({ ...policy, voice_commands: event.target.checked })}
        />
        {t("Распознавать обращения «Ева, открой …»")}
      </label>

      <label className="listening__toggle">
        <input
          type="checkbox"
          disabled={disabled || !policy.voice_commands}
          checked={policy.voice_commands_autorun}
          onChange={(event) =>
            onChange({ ...policy, voice_commands_autorun: event.target.checked })
          }
        />
        {t("Открывать сразу, без подтверждения")}
      </label>

      <label className="listening__field">
        {t("Срок хранения текста, суток")}
        <input
          type="number"
          min={1}
          max={90}
          disabled={disabled}
          value={policy.retention_days}
          onChange={(event) =>
            onChange({ ...policy, retention_days: Number(event.target.value) || 0 })
          }
        />
      </label>

      <div className="listening__actions">
        <button type="button" disabled={disabled} onClick={onSave}>
          {t("Сохранить политику")}
        </button>
        <button type="button" disabled={disabled} onClick={onReset}>
          {t("Отменить правки")}
        </button>
      </div>
      {applied ? <p role="status">{t("Политика сохранена и передана слушателю.")}</p> : null}
    </section>
  )
}
