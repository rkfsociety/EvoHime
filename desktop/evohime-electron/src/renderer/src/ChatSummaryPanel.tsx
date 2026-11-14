import { useCallback, useEffect, useMemo, useState } from 'react'

import type { ChatRecord, ConnectionState, CoreEvent, RepositorySummary } from '@shared/api'

import {
  buildChatSummary,
  parseBackgroundRunsEvent,
  parseContextLedgerEvent,
  parseGitDiffEvent,
  parseGitStatusEvent
} from './chat-summary'
import { formatNumber, useT } from './i18n'
import { useShellApi } from './shell-api'
import { UiIcon } from './UiIcon'

type SummaryTab = 'overview' | 'files' | 'context' | 'shells'

export interface ChatSummaryPanelProps {
  readonly connection: ConnectionState
  readonly chatId: string | null
  readonly chatRevision?: number
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
  readonly onClose: () => void
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export function ChatSummaryPanel({
  connection,
  chatId,
  chatRevision = 0,
  events,
  workspace,
  onClose
}: ChatSummaryPanelProps): React.JSX.Element {
  const t = useT()
  const api = useShellApi()
  const [tab, setTab] = useState<SummaryTab>('overview')
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [repository, setRepository] = useState<RepositorySummary | null>(null)
  const [selectedDiffPath, setSelectedDiffPath] = useState<string | null>(null)
  const [expandedShellId, setExpandedShellId] = useState<string | null>(null)
  const [actionNotice, setActionNotice] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)

  const connected = CONNECTED_STATES.includes(connection)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  useEffect(() => {
    if (!api || chatId === null) {
      setChat(null)
      return
    }
    let active = true
    void api.invoke('chat.open', { chatId }).then((outcome) => {
      if (active && outcome.ok) setChat(outcome.value)
    })
    return () => {
      active = false
    }
  }, [api, chatId, chatRevision])

  const summary = useMemo(() => buildChatSummary(events, chat), [events, chat])
  const gitStatus = useMemo(() => parseGitStatusEvent(events), [events])
  const gitDiff = useMemo(() => parseGitDiffEvent(events), [events])
  const contextLedger = useMemo(
    () => parseContextLedgerEvent(events, summary.latestTaskId),
    [events, summary.latestTaskId]
  )
  const backgroundRuns = useMemo(() => parseBackgroundRunsEvent(events), [events])

  const refreshLiveState = useCallback(async () => {
    if (!api) return
    setRefreshing(true)
    setActionNotice(null)
    try {
      const requests: Promise<unknown>[] = []
      if (workspace) {
        requests.push(
          api.invoke('repository.get', { workspacePath: workspace }).then((outcome) => {
            if (outcome.ok) setRepository(outcome.value)
            else setRepository(null)
          })
        )
        if (connected) {
          requests.push(
            api.invoke('core.gitStatus', { workspacePath: workspace, maxBytes: 128 * 1024 }).catch(() => undefined)
          )
          requests.push(
            api.invoke('core.backgroundExecution', {
              operation: 'list_runs',
              ownerScope: workspace
            }).catch(() => undefined)
          )
        }
      } else {
        setRepository(null)
      }
      if (connected && summary.latestTaskId) {
        requests.push(
          api.invoke('core.getContextLedger', { taskId: summary.latestTaskId, limit: 30 }).catch(() => undefined)
        )
      }
      await Promise.all(requests)
    } finally {
      setRefreshing(false)
    }
  }, [api, connected, summary.latestTaskId, workspace])

  useEffect(() => {
    void refreshLiveState()
  }, [refreshLiveState])

  const openFileDiff = useCallback(
    async (relativePath?: string) => {
      if (!api || !workspace || !connected) return
      setSelectedDiffPath(relativePath ?? '__all__')
      setActionNotice(null)
      const outcome = await api.invoke('core.gitDiff', {
        workspacePath: workspace,
        ...(relativePath ? { relativePath } : {}),
        maxBytes: 128 * 1024
      })
      if (!outcome.ok) setActionNotice(outcome.message)
    },
    [api, connected, workspace]
  )

  const summarizeContext = useCallback(async () => {
    if (!api || !connected || !summary.latestTaskId) return
    setActionNotice(t('Запрос сжатия контекста отправлен…'))
    const outcome = await api.invoke('core.summarizeContextNow', {
      taskId: summary.latestTaskId
    })
    if (!outcome.ok) setActionNotice(outcome.message)
  }, [api, connected, summary.latestTaskId, t])

  const changedFileCount = gitStatus?.files.length ?? 0

  return (
    <aside className="trace-panel summary-panel" aria-label={t('Сводка текущего чата')} aria-live="polite">
      <header className="trace-panel__header">
        <div>
          <h2>{t('Сводка')}</h2>
          <p>{chat ? summary.chatTitle : t('Чат не выбран')}</p>
        </div>
        <div className="trace-panel__actions">
          <button
            type="button"
            onClick={() => void refreshLiveState()}
            disabled={refreshing}
            title={t('Обновить данные сводки')}
          >
            {refreshing ? t('Обновление…') : t('Обновить')}
          </button>
          <button
            type="button"
            className="trace-panel__close"
            aria-label={t('Закрыть сводку')}
            onClick={onClose}
          >
            ×
          </button>
        </div>
      </header>

      <div className="summary-panel__tabs" role="tablist" aria-label={t('Разделы сводки')}>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'overview'}
          className={`summary-panel__tab${tab === 'overview' ? ' summary-panel__tab--active' : ''}`}
          onClick={() => setTab('overview')}
        >
          {t('Обзор')}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'files'}
          className={`summary-panel__tab${tab === 'files' ? ' summary-panel__tab--active' : ''}`}
          onClick={() => setTab('files')}
        >
          {t('Файлы')}
          {changedFileCount > 0 ? <span className="summary-panel__badge">{formatNumber(changedFileCount)}</span> : null}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'context'}
          className={`summary-panel__tab${tab === 'context' ? ' summary-panel__tab--active' : ''}`}
          onClick={() => setTab('context')}
        >
          {t('Контекст')}
          {summary.contextPercent !== null ? (
            <span className="summary-panel__badge">{formatNumber(summary.contextPercent)}%</span>
          ) : null}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={tab === 'shells'}
          className={`summary-panel__tab${tab === 'shells' ? ' summary-panel__tab--active' : ''}`}
          onClick={() => setTab('shells')}
        >
          {t('Шелы')}
          {summary.shells.length > 0 ? (
            <span className={`summary-panel__badge${summary.activeShellCount > 0 ? ' summary-panel__badge--live' : ''}`}>
              {formatNumber(summary.activeShellCount > 0 ? summary.activeShellCount : summary.shells.length)}
            </span>
          ) : null}
        </button>
      </div>

      {actionNotice ? <p className="trace-panel__reason" role="status">{actionNotice}</p> : null}

      <div className="summary-panel__body">
        {chatId === null ? (
          <p className="trace-panel__empty">{t('Выбери чат, чтобы увидеть его сводку.')}</p>
        ) : null}

        {tab === 'overview' ? (
          <div className="summary-panel__section">
            <div className="summary-cards">
              <article className="summary-card">
                <span className="summary-card__label">{t('Статус чата')}</span>
                <strong className="summary-card__value">
                  {summary.running ? t('Выполняется') : summary.tasks[0]?.status === 'failed' ? t('Ошибка') : t('Готов')}
                </strong>
                <small className="summary-card__meta">
                  {formatNumber(summary.messageCount)} {t('сообщ.')} · {formatNumber(summary.taskCount)} {t('задач')}
                </small>
              </article>

              <article className="summary-card">
                <span className="summary-card__label">{t('Контекст')}</span>
                <strong className="summary-card__value">
                  {summary.contextPercent !== null ? `${formatNumber(summary.contextPercent)}%` : '—'}
                </strong>
                <small className="summary-card__meta">
                  {summary.contextUsedTokens !== null && summary.contextLimitTokens !== null
                    ? `${formatNumber(summary.contextUsedTokens)} / ${formatNumber(summary.contextLimitTokens)}`
                    : t('ожидание оценки')}
                </small>
              </article>

              <article className="summary-card">
                <span className="summary-card__label">{t('Изменения')}</span>
                <strong className="summary-card__value">
                  {repository ? (
                    <>
                      <span className="summary-diff-add">+{formatNumber(repository.added)}</span>{' '}
                      <span className="summary-diff-del">−{formatNumber(repository.removed)}</span>
                    </>
                  ) : '—'}
                </strong>
                <small className="summary-card__meta">
                  {repository
                    ? `${repository.branch} · ${formatNumber(changedFileCount)} ${t('файлов')}`
                    : workspace ? t('не git-репозиторий') : t('проект не выбран')}
                </small>
              </article>

              <article className="summary-card">
                <span className="summary-card__label">{t('Шелы')}</span>
                <strong className="summary-card__value">
                  {summary.activeShellCount > 0
                    ? `${formatNumber(summary.activeShellCount)} ${t('акт.')}`
                    : formatNumber(summary.shells.length)}
                </strong>
                <small className="summary-card__meta">
                  {formatNumber(backgroundRuns.length)} {t('фоновых')} · {formatNumber(summary.pendingApprovals.length)} {t('ожидает')}
                </small>
              </article>
            </div>

            <dl className="trace-panel__summary">
              <div>
                <dt>{t('Модель')}</dt>
                <dd>{summary.modelName ?? '—'}</dd>
              </div>
              <div>
                <dt>{t('Провайдер')}</dt>
                <dd>{summary.providerName ?? '—'}</dd>
              </div>
              <div>
                <dt>{t('Токены (in / out)')}</dt>
                <dd>{formatNumber(summary.inputTokens)} / {formatNumber(summary.outputTokens)}</dd>
              </div>
              <div>
                <dt>Workspace</dt>
                <dd title={workspace ?? undefined}>{workspace ?? t('не выбран')}</dd>
              </div>
            </dl>

            {summary.goal ? (
              <section className="summary-block">
                <h3><UiIcon name="target" size={14} /> {t('Закреплённая цель')}</h3>
                <p className="summary-block__text">{summary.goal.objective}</p>
                <small className="summary-block__meta">{t('Статус:')} {summary.goal.status}</small>
              </section>
            ) : null}

            {summary.pendingApprovals.length > 0 ? (
              <section className="summary-block summary-block--warn">
                <h3><UiIcon name="alert-triangle" size={14} /> {t('Ожидают подтверждения')}</h3>
                <ul className="summary-list">
                  {summary.pendingApprovals.map((item) => (
                    <li key={item.approvalId} className="summary-list__item">
                      <code>{item.toolName}</code>
                      <span>{item.permission}{item.scope ? ` · ${item.scope}` : ''}</span>
                    </li>
                  ))}
                </ul>
              </section>
            ) : null}

            <section className="summary-block">
              <h3><UiIcon name="history" size={14} /> {t('Задачи чата')}</h3>
              {summary.tasks.length === 0 ? (
                <p className="summary-block__empty">{t('Задачи в этом чате ещё не запускались.')}</p>
              ) : (
                <ul className="summary-list">
                  {summary.tasks.slice(0, 8).map((task) => (
                    <li key={task.taskId} className="summary-list__item">
                      <span className={`summary-status summary-status--${task.status}`}>{task.status}</span>
                      <code>{task.taskId}</code>
                      {task.error ? <small className="summary-block__error">{task.error}</small> : null}
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        ) : null}

        {tab === 'files' ? (
          <div className="summary-panel__section">
            <section className="summary-block">
              <div className="summary-block__header">
                <h3><UiIcon name="git-branch" size={14} /> {t('Изменения Git')}</h3>
                {workspace && connected ? (
                  <button type="button" className="summary-link-btn" onClick={() => void openFileDiff()}>
                    {t('Общий diff')}
                  </button>
                ) : null}
              </div>
              {!workspace ? (
                <p className="summary-block__empty">{t('Выбери папку проекта, чтобы видеть изменения файлов.')}</p>
              ) : !gitStatus ? (
                <p className="summary-block__empty">{connected ? t('Загрузка статуса Git…') : t('Подключись к Core для просмотра Git.')}</p>
              ) : gitStatus.files.length === 0 ? (
                <p className="summary-block__empty">{t('Рабочее дерево чистое, незакоммиченных изменений нет.')}</p>
              ) : (
                <ul className="summary-list">
                  {gitStatus.files.map((file) => (
                    <li key={`${file.status}:${file.path}`} className="summary-list__row">
                      <span className="summary-git-code">{file.status}</span>
                      <code className="summary-file-path" title={file.path}>{file.path}</code>
                      <button
                        type="button"
                        className="summary-link-btn"
                        disabled={!connected}
                        onClick={() => void openFileDiff(file.path)}
                      >
                        Diff
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            {selectedDiffPath && gitDiff ? (
              <section className="summary-block">
                <div className="summary-block__header">
                  <h3>{gitDiff.path ? `Diff: ${gitDiff.path}` : t('Общий Diff')}</h3>
                  <button type="button" className="summary-link-btn" onClick={() => setSelectedDiffPath(null)}>
                    {t('Скрыть')}
                  </button>
                </div>
                <pre className="summary-diff-view">{gitDiff.diff || t('Изменений нет.')}</pre>
              </section>
            ) : null}

            <section className="summary-block">
              <h3><UiIcon name="folder" size={14} /> {t('Файлы, затронутые в чате')}</h3>
              {summary.touchedFiles.length === 0 ? (
                <p className="summary-block__empty">{t('В этом чате инструменты ещё не обращались к файлам.')}</p>
              ) : (
                <ul className="summary-list">
                  {summary.touchedFiles.map((file) => (
                    <li key={file.path} className="summary-list__row">
                      <span className={`summary-file-action summary-file-action--${file.action}`}>
                        {file.action === 'write' ? t('правка') : t('чтение')}
                      </span>
                      <code className="summary-file-path" title={file.path}>{file.path}</code>
                      <small>×{formatNumber(file.count)}</small>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        ) : null}

        {tab === 'context' ? (
          <div className="summary-panel__section">
            <section className="summary-block">
              <div className="summary-block__header">
                <h3>{t('Окно контекста')}</h3>
                {summary.latestTaskId && connected ? (
                  <button type="button" className="summary-link-btn" onClick={() => void summarizeContext()}>
                    {t('Сжать контекст')}
                  </button>
                ) : null}
              </div>
              <div className="summary-progress">
                <div
                  className="summary-progress__fill"
                  style={{ width: `${summary.contextPercent ?? 0}%` }}
                />
              </div>
              <p className="summary-block__meta">
                {summary.contextUsedTokens !== null && summary.contextLimitTokens !== null
                  ? `${formatNumber(summary.contextPercent ?? 0)}% · ${formatNumber(summary.contextUsedTokens)} ${t('из')} ${formatNumber(summary.contextLimitTokens)} ${t('токенов')}`
                  : t('Данные о заполнении контекста появятся после обращения к модели.')}
              </p>
            </section>

            <section className="summary-block">
              <h3>{t('Context Ledger')}</h3>
              {contextLedger.length === 0 ? (
                <p className="summary-block__empty">{t('Записи Context Ledger для текущей задачи отсутствуют.')}</p>
              ) : (
                <ul className="summary-list">
                  {contextLedger.map((entry) => (
                    <li key={entry.id} className="summary-list__item">
                      <div className="summary-list__row">
                        <span className="summary-git-code">{entry.kind}</span>
                        <span className="summary-file-path">{entry.title}</span>
                        {entry.tokens !== null ? <small>{formatNumber(entry.tokens)} tok</small> : null}
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        ) : null}

        {tab === 'shells' ? (
          <div className="summary-panel__section">
            <section className="summary-block">
              <h3><UiIcon name="terminal" size={14} /> {t('Команды и шелы чата')}</h3>
              {summary.shells.length === 0 ? (
                <p className="summary-block__empty">{t('В этом чате ещё не запускались shell-команды.')}</p>
              ) : (
                <ul className="summary-list">
                  {summary.shells.map((shell) => {
                    const expanded = expandedShellId === shell.id
                    return (
                      <li key={shell.id} className="summary-list__item">
                        <div className="summary-list__row">
                          <span className={`summary-status summary-status--${shell.status}`}>
                            {shell.status === 'running' ? t('активен') : t('завершён')}
                          </span>
                          <code className="summary-file-path" title={shell.command}>{shell.command}</code>
                          {shell.output ? (
                            <button
                              type="button"
                              className="summary-link-btn"
                              onClick={() => setExpandedShellId(expanded ? null : shell.id)}
                            >
                              {expanded ? t('Скрыть') : t('Вывод')}
                            </button>
                          ) : null}
                        </div>
                        {expanded && shell.output ? (
                          <pre className="summary-diff-view">{shell.output}</pre>
                        ) : null}
                      </li>
                    )
                  })}
                </ul>
              )}
            </section>

            <section className="summary-block">
              <h3><UiIcon name="activity" size={14} /> {t('Фоновые запуски')}</h3>
              {backgroundRuns.length === 0 ? (
                <p className="summary-block__empty">{t('Активных фоновых запусков нет.')}</p>
              ) : (
                <ul className="summary-list">
                  {backgroundRuns.map((run) => (
                    <li key={run.runId} className="summary-list__row">
                      <span className="summary-git-code">{run.status}</span>
                      <code className="summary-file-path" title={run.label}>{run.label}</code>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        ) : null}
      </div>
    </aside>
  )
}
