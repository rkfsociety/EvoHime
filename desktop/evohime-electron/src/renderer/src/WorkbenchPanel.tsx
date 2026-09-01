import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, ConversationWorkbenchProjection, WorkbenchPresentation } from '@shared/api'

import { useShellApi } from './shell-api'

const TAB_IDS = ['files', 'diff', 'tasks', 'terminal', 'browser', 'usage'] as const

export function WorkbenchPanel({
  connection,
  chatId,
  workspace,
  events
}: {
  readonly connection: ConnectionState
  readonly chatId: string | null
  readonly workspace: string | null
  readonly events: readonly CoreEvent[]
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
  if (!chatId) return <section className="workbench workbench--empty"><h3>Conversation Workbench</h3><p>Откройте чат, чтобы привязать рабочую поверхность к conversation.</p></section>

  return (
    <section className={`workbench${presentation.collapsed ? ' workbench--collapsed' : ''}`} aria-label="Conversation Workbench">
      <header className="workbench__header">
        <div><h3>Conversation Workbench</h3><span>{connection === 'connected' ? 'Core projection' : 'Ожидание Core'}</span></div>
        <button type="button" onClick={() => savePresentation({ ...presentation, collapsed: !presentation.collapsed })}>{presentation.collapsed ? 'Развернуть' : 'Свернуть'}</button>
      </header>
      {!presentation.collapsed ? <>
        <div className="workbench__tabs" role="tablist" aria-label="Вкладки conversation">
          {(projection?.tabs ?? TAB_IDS.map((id) => ({ id, label: id, availability: 'unavailable', reason: 'projection_pending', badgeSource: 'core', persistence: 'presentation_only' }))).map((tab) => (
            <button key={tab.id} type="button" role="tab" aria-selected={tab.id === presentation.activeTab} disabled={tab.availability !== 'available'} className={tab.id === presentation.activeTab ? 'workbench__tab workbench__tab--active' : 'workbench__tab'} onClick={() => savePresentation({ ...presentation, activeTab: tab.id })} title={tab.reason || undefined}>
              {tab.label}<small>{tab.availability === 'available' ? 'доступно' : 'недоступно'}</small>
            </button>
          ))}
        </div>
        <div className="workbench__body">
          {!projection ? <p className="workbench__muted">Получаю bounded projection Core…</p> : selected?.availability === 'unavailable' ? <p className="workbench__muted">Вкладка недоступна: {selected.reason}.</p> : presentation.activeTab === 'usage' ? <p>Событий: {projection.eventCount} · задач: {projection.taskCount} · input tokens: {projection.usageInputTokens} · output tokens: {projection.usageOutputTokens}</p> : <p>Состояние привязано к conversation и cursor {projection.eventCursor}.</p>}
        </div>
      </> : null}
    </section>
  )
}
