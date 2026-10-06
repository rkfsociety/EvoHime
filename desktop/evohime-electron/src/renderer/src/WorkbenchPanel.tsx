import { translate } from './i18n'
import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, ConversationWorkbenchProjection, WorkbenchPresentation } from '@shared/api'

import { useShellApi } from './shell-api'

const TAB_IDS = ['files', 'diff', 'tasks', 'terminal', 'browser', 'usage'] as const

export function WorkbenchPanel({
  connection,
  chatId,
  workspace,
  events,
  onClose
}: {
  readonly connection: ConnectionState
  readonly chatId: string | null
  readonly workspace: string | null
  readonly events: readonly CoreEvent[]
  readonly onClose: () => void
}): React.JSX.Element {
  const api = useShellApi()
  const [projection, setProjection] = useState<ConversationWorkbenchProjection | null>(null)
  const [presentation, setPresentation] = useState<WorkbenchPresentation>({ activeTab: 'tasks', splitRatio: 0.5, collapsed: false })
  const latestConversationSequence = useMemo(() => events
    .map((event) => event.conversationEventLog)
    .filter((page) => page?.conversationId === chatId)
    .reduce((latest, page) => Math.max(latest, page?.newestSequence ?? 0), 0), [events, chatId])

  useEffect(() => {
    setProjection(null)
    if (!api || !chatId || !workspace) return
    let alive = true
    void api.invoke('chat.getWorkbenchPresentation', { chatId }).then((result) => {
      if (alive && result.ok) setPresentation(result.value)
    })
    void api.invoke('core.getConversationWorkbench', { conversationId: chatId, workspaceId: workspace, limit: 100 }).catch(() => undefined)
    return () => { alive = false }
  }, [api, chatId, workspace])

  useEffect(() => {
    if (!api || !chatId || !workspace || latestConversationSequence === 0) return
    void api.invoke('core.getConversationWorkbench', { conversationId: chatId, workspaceId: workspace, afterSequence: 0, limit: 100 }).catch(() => undefined)
  }, [api, chatId, workspace, latestConversationSequence])

  useEffect(() => {
    const incoming = events.find((event) => event.conversationWorkbench?.conversationId === chatId)?.conversationWorkbench
    if (incoming && incoming.conversationId === chatId) setProjection(incoming)
  }, [events, chatId])

  const savePresentation = (next: WorkbenchPresentation): void => {
    setPresentation(next)
    if (api && chatId) void api.invoke('chat.saveWorkbenchPresentation', { chatId, presentation: next })
  }

  const selected = useMemo(() => projection?.tabs.find((tab) => tab.id === presentation.activeTab) ?? null, [projection, presentation.activeTab])
  if (!chatId) return (
    <section className="workbench workbench--empty" aria-label="Conversation Workbench">
      <header className="workbench__header">
        <div><h3>Conversation Workbench</h3><span>{translate("Дополнительная панель")}</span></div>
        <button type="button" onClick={onClose}>{translate("Скрыть")}</button>
      </header>
      <p>{translate("Откройте чат, чтобы привязать рабочую поверхность к conversation.")}</p>
    </section>
  )

  return (
    <section className={`workbench${presentation.collapsed ? ' workbench--collapsed' : ''}`} aria-label="Conversation Workbench">
      <header className="workbench__header">
        <div><h3>Conversation Workbench</h3><span>{translate(connection === 'connected' ? 'Core projection' : translate("Ожидание Core"))}</span></div>
        <div className="workbench__actions">
          <button type="button" onClick={() => savePresentation({ ...presentation, collapsed: !presentation.collapsed })}>{translate(presentation.collapsed ? translate("Развернуть") : translate("Свернуть"))}</button>
          <button type="button" onClick={onClose}>{translate("Скрыть")}</button>
        </div>
      </header>
      {!presentation.collapsed ? <>
        <div className="workbench__tabs" role="tablist" aria-label={translate("Вкладки conversation")}>
          {(projection?.tabs ?? TAB_IDS.map((id) => ({ id, label: id, availability: 'unavailable', reason: 'projection_pending', badgeSource: 'core', persistence: 'presentation_only' }))).map((tab) => (
            <button key={tab.id} type="button" role="tab" aria-selected={tab.id === presentation.activeTab} disabled={tab.availability !== 'available'} className={tab.id === presentation.activeTab ? 'workbench__tab workbench__tab--active' : 'workbench__tab'} onClick={() => savePresentation({ ...presentation, activeTab: tab.id })} title={tab.reason || undefined}>
              {translate(tab.label)}<small>{translate(tab.availability === 'available' ? translate("доступно") : translate("недоступно"))}</small>
            </button>
          ))}
        </div>
        <div className="workbench__body">
          {!projection ? <p className="workbench__muted">{translate("Получаю bounded projection Core…")}</p> : selected?.availability === 'unavailable' ? <p className="workbench__muted">{translate("Вкладка недоступна:")}{translate(selected.reason)}.</p> : presentation.activeTab === 'usage' ? <p>{translate("Событий:")}{translate(projection.eventCount)} {translate("· задач:")}{translate(projection.taskCount)} · input tokens: {translate(projection.usageInputTokens)} · output tokens: {translate(projection.usageOutputTokens)}</p> : <p>{translate("Состояние привязано к conversation и cursor")}{translate(projection.eventCursor)}.</p>}
        </div>
      </> : null}
    </section>
  )
}
