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

type SummarySection = 'review' | 'files' | 'terminal' | 'context' | 'background'

export interface ChatSummaryPanelProps {
  readonly connection: ConnectionState
  readonly chatId: string | null
  readonly chatRevision?: number
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
}

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export function ChatSummaryPanel({
  connection,
  chatId,
  chatRevision = 0,
  events,
  workspace
}: ChatSummaryPanelProps): React.JSX.Element {
  const t = useT()
  const api = useShellApi()
  const [expandedSection, setExpandedSection] = useState<SummarySection | null>(null)
  const [chat, setChat] = useState<ChatRecord | null>(null)
  const [repository, setRepository] = useState<RepositorySummary | null>(null)
  const [selectedDiffPath, setSelectedDiffPath] = useState<string | null>(null)
  const [expandedShellId, setExpandedShellId] = useState<string | null>(null)
  const [actionNotice, setActionNotice] = useState<string | null>(null)
  const [refreshing, setRefreshing] = useState(false)

  const connected = CONNECTED_STATES.includes(connection)

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

  const toggleSection = (section: SummarySection): void => {
    setExpandedSection((current) => (current === section ? null : section))
  }

  const changedFileCount = gitStatus?.files.length ?? 0
  const filesBadgeCount = changedFileCount > 0 ? changedFileCount : summary.touchedFiles.length
  const shellBadgeCount = summary.activeShellCount > 0 ? summary.activeShellCount : summary.shells.length

  return (
    <section className="summary-widget" aria-label={t('Сводка текущего чата')} aria-live="polite">
      <header className="summary-widget__header">
        <span className="summary-widget__title" title={chat ? summary.chatTitle : undefined}>
          {t('Сводка')}
        </span>
        <button
          type="button"
          className={`summary-widget__refresh${refreshing ? ' summary-widget__refresh--spinning' : ''}`}
          onClick={() => void refreshLiveState()}
          disabled={refreshing}
          aria-label={t('Обновить данные сводки')}
          title={t('Обновить данные сводки')}
        >
          <UiIcon name="repeat" size={13} />
        </button>
      </header>

      <div className="summary-widget__rows">
        <button
          type="button"
          className={`summary-widget__row${expandedSection === 'review' ? ' summary-widget__row--active' : ''}`}
          aria-expanded={expandedSection === 'review'}
          onClick={() => toggleSection('review')}
        >
          <span className="summary-widget__row-main">
            <UiIcon name="git-branch" size={14} />
            <span>{t('Изменения')}</span>
          </span>
          <span className="summary-widget__row-meta">
            {repository ? (
              <>
                <span className="summary-diff-add">+{formatNumber(repository.added)}</span>{' '}
                <span className="summary-diff-del">−{formatNumber(repository.removed)}</span>
              </>
            ) : (
              <span className="summary-widget__muted">—</span>
            )}
          </span>
        </button>

        <button
          type="button"
          className={`summary-widget__row${expandedSection === 'files' ? ' summary-widget__row--active' : ''}`}
          aria-expanded={expandedSection === 'files'}
          onClick={() => toggleSection('files')}
        >
          <span className="summary-widget__row-main">
            <UiIcon name="folder" size={14} />
            <span>{t('Файлы')}</span>
          </span>
          <span className="summary-widget__row-meta">
            {filesBadgeCount > 0 ? (
              <span className="summary-panel__badge">{formatNumber(filesBadgeCount)}</span>
            ) : null}
          </span>
        </button>

        <button
          type="button"
          className={`summary-widget__row${expandedSection === 'terminal' ? ' summary-widget__row--active' : ''}`}
          aria-expanded={expandedSection === 'terminal'}
          onClick={() => toggleSection('terminal')}
        >
          <span className="summary-widget__row-main">
            <UiIcon name="terminal" size={14} />
            <span>{t('Терминал')}</span>
          </span>
          <span className="summary-widget__row-meta">
            {shellBadgeCount > 0 ? (
              <span className={`summary-panel__badge${summary.activeShellCount > 0 ? ' summary-panel__badge--live' : ''}`}>
                {formatNumber(shellBadgeCount)}
              </span>
            ) : null}
          </span>
        </button>

        <button
          type="button"
          className={`summary-widget__row${expandedSection === 'context' ? ' summary-widget__row--active' : ''}`}
          aria-expanded={expandedSection === 'context'}
          onClick={() => toggleSection('context')}
        >
          <span className="summary-widget__row-main">
            <UiIcon name="activity" size={14} />
            <span>{t('Контекст')}</span>
          </span>
          <span className="summary-widget__row-meta">
            {summary.contextPercent !== null ? (
              <span className="summary-panel__badge">{formatNumber(summary.contextPercent)}%</span>
            ) : (
              <span className="summary-widget__muted">—</span>
            )}
          </span>
        </button>
      </div>

      <div className="summary-widget__footer">
        <button
          type="button"
          className={`summary-widget__row summary-widget__row--footer${expandedSection === 'background' ? ' summary-widget__row--active' : ''}`}
          aria-expanded={expandedSection === 'background'}
          onClick={() => toggleSection('background')}
        >
          <span className="summary-widget__row-main">
            <span>{t('Фоновые задачи')}</span>
          </span>
          <span className="summary-widget__row-meta">
            <span className="summary-panel__badge">{formatNumber(backgroundRuns.length)}</span>
          </span>
        </button>
      </div>

      {actionNotice ? <p className="summary-widget__notice" role="status">{actionNotice}</p> : null}

      {expandedSection !== null ? (
        <div className="summary-widget__detail">
          {expandedSection === 'review' ? (
            <div className="summary-panel__section">
              <div className="summary-block__header">
                <h3>{repository ? repository.branch : t('Репозиторий')}</h3>
                {workspace && connected ? (
                  <button type="button" className="summary-link-btn" onClick={() => void openFileDiff()}>
                    {t('Общий diff')}
                  </button>
                ) : null}
              </div>
              {summary.modelName ? (
                <p className="summary-block__meta">{summary.modelName}{summary.providerName ? ` · ${summary.providerName}` : ''}</p>
              ) : null}
              {summary.goal ? (
                <p className="summary-block__text">{summary.goal.objective}</p>
              ) : null}
              {selectedDiffPath && gitDiff ? (
                <pre className="summary-diff-view">{gitDiff.diff || t('Изменений нет.')}</pre>
              ) : null}
            </div>
          ) : null}

          {expandedSection === 'files' ? (
            <div className="summary-panel__section">
              {workspace && gitStatus && gitStatus.files.length > 0 ? (
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
              ) : (
                <p className="summary-block__empty">
                  {!workspace
                    ? t('Выбери папку проекта.')
                    : gitStatus
                      ? t('Незакоммиченных изменений нет.')
                      : t('Загрузка статуса Git…')}
                </p>
              )}

              {selectedDiffPath && gitDiff ? (
                <div className="summary-block">
                  <div className="summary-block__header">
                    <h3>{gitDiff.path ? gitDiff.path : t('Общий Diff')}</h3>
                    <button type="button" className="summary-link-btn" onClick={() => setSelectedDiffPath(null)}>
                      {t('Скрыть')}
                    </button>
                  </div>
                  <pre className="summary-diff-view">{gitDiff.diff || t('Изменений нет.')}</pre>
                </div>
              ) : null}

              {summary.touchedFiles.length > 0 ? (
                <div className="summary-block">
                  <h3>{t('Затронуто в чате')}</h3>
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
                </div>
              ) : null}
            </div>
          ) : null}

          {expandedSection === 'terminal' ? (
            <div className="summary-panel__section">
              {summary.shells.length === 0 ? (
                <p className="summary-block__empty">{t('Команды ещё не запускались.')}</p>
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
            </div>
          ) : null}

          {expandedSection === 'context' ? (
            <div className="summary-panel__section">
              <div className="summary-block__header">
                <h3>{summary.modelName ?? t('Окно контекста')}</h3>
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
                  ? `${formatNumber(summary.contextPercent ?? 0)}% · ${formatNumber(summary.contextUsedTokens)} / ${formatNumber(summary.contextLimitTokens)}`
                  : t('Ожидание обращения к модели.')}
              </p>
              {contextLedger.length > 0 ? (
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
              ) : null}
            </div>
          ) : null}

          {expandedSection === 'background' ? (
            <div className="summary-panel__section">
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
            </div>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}
