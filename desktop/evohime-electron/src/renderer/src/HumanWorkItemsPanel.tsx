import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { translate } from './i18n'
import { useShellApi } from './shell-api'
import type { ConnectionState } from './types'

type ResponseSchema =
  | { kind: 'text' }
  | { kind: 'choice'; choices: string[] }

type WorkItemSummary = {
  id: string
  revision: number
  title?: string
  state: string
  expires_at_ms?: number | null
}

type WorkItemDetails = {
  revision: number
  instructions?: string
  response_schema?: ResponseSchema
}

type EventPayload = {
  operation?: string
  status?: string
  item_id?: string
  revision?: number
  state?: string
  error_code?: string
  projection_json?: {
    items?: WorkItemSummary[]
    id?: string
    revision?: number
    title?: string
    instructions?: string
    response_schema?: ResponseSchema
    state?: string
    error_code?: string
  }
}

const ACTIVE_STATES = new Set(['waiting_for_human', 'in_progress', 'needs_revision'])

export interface HumanChatQuestionsProps {
  readonly connection: ConnectionState
}

/**
 * Renders pending Core-owned human questions directly in the chat when any exist.
 * Core owns validation, revisions, and persistence; UI only collects typed data.
 */
export function HumanChatQuestions({ connection }: HumanChatQuestionsProps): React.JSX.Element | null {
  const api = useShellApi()
  const [items, setItems] = useState<WorkItemSummary[]>([])
  const [detailsById, setDetailsById] = useState<Record<string, WorkItemDetails>>({})
  const [drafts, setDrafts] = useState<Record<string, string>>({})
  const [busyId, setBusyId] = useState<string | null>(null)
  const [errorText, setErrorText] = useState<string | null>(null)

  const pendingSubmitRef = useRef<Map<string, string>>(new Map())
  const requestedGetRef = useRef<Set<string>>(new Set())

  const requestList = useCallback(() => {
    if (!api) return
    void api.invoke('humanWorkItems.list', {
      requestId: crypto.randomUUID(),
      ownerScope: 'human-work-items',
      idempotencyKey: crypto.randomUUID()
    })
  }, [api])

  const requestGet = useCallback(
    (itemId: string) => {
      if (!api) return
      void api.invoke('humanWorkItems.get', {
        requestId: crypto.randomUUID(),
        ownerScope: 'human-work-items',
        idempotencyKey: crypto.randomUUID(),
        itemId
      })
    },
    [api]
  )

  useEffect(() => {
    if (!api) return
    const off = api.subscribe((event) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'human_work_items.result') {
        return
      }
      try {
        const value = JSON.parse(event.event.payload) as EventPayload
        const projection = value.projection_json

        if (value.status === 'rejected' && value.error_code) {
          if (value.item_id) {
            pendingSubmitRef.current.delete(value.item_id)
          }
          setBusyId(null)
          setErrorText(value.error_code)
          return
        }

        if (Array.isArray(projection?.items)) {
          setErrorText(null)
          setItems(projection.items)
          return
        }

        if (value.operation === 'get' && projection?.id) {
          const id = projection.id
          const rev = projection.revision ?? value.revision ?? 0
          setDetailsById((prev) => ({
            ...prev,
            [id]: {
              revision: rev,
              instructions: projection.instructions,
              response_schema: projection.response_schema
            }
          }))
          return
        }

        if (value.operation === 'start' && value.status === 'ok' && value.item_id) {
          const itemId = value.item_id
          const pendingResponse = pendingSubmitRef.current.get(itemId)
          if (pendingResponse !== undefined && typeof value.revision === 'number') {
            pendingSubmitRef.current.delete(itemId)
            void api.invoke('humanWorkItems.submit', {
              requestId: crypto.randomUUID(),
              ownerScope: 'human-work-items',
              idempotencyKey: crypto.randomUUID(),
              itemId,
              expectedRevision: value.revision,
              response: pendingResponse
            })
            return
          }
          requestList()
          return
        }

        if (
          value.operation === 'submit' ||
          value.operation === 'cancel' ||
          value.operation === 'create' ||
          value.operation === 'accept' ||
          value.operation === 'revise' ||
          value.operation === 'return' ||
          value.operation === 'expire_due'
        ) {
          if (value.item_id && value.operation === 'submit') {
            setDrafts((prev) => {
              const next = { ...prev }
              delete next[value.item_id!]
              return next
            })
          }
          setBusyId(null)
          setErrorText(null)
          requestList()
        }
      } catch {
        setBusyId(null)
        setErrorText('invalid_projection')
      }
    })
    return off
  }, [api, requestList])

  useEffect(() => {
    if (connection !== 'connected') return
    requestList()
  }, [connection, requestList])

  const activeItems = useMemo(
    () => items.filter((item) => ACTIVE_STATES.has(item.state)),
    [items]
  )

  useEffect(() => {
    for (const item of activeItems) {
      const key = `${item.id}:${item.revision}`
      const loaded = detailsById[item.id]
      if ((!loaded || loaded.revision !== item.revision) && !requestedGetRef.current.has(key)) {
        requestedGetRef.current.add(key)
        requestGet(item.id)
      }
    }
  }, [activeItems, detailsById, requestGet])

  const submitAnswer = useCallback(
    async (item: WorkItemSummary) => {
      if (!api) return
      const answer = (drafts[item.id] ?? '').trim()
      if (!answer) return

      setBusyId(item.id)
      setErrorText(null)

      if (item.state === 'waiting_for_human' || item.state === 'draft') {
        pendingSubmitRef.current.set(item.id, answer)
        const outcome = await api.invoke('humanWorkItems.start', {
          requestId: crypto.randomUUID(),
          ownerScope: 'human-work-items',
          idempotencyKey: crypto.randomUUID(),
          itemId: item.id,
          expectedRevision: item.revision
        })
        if (!outcome?.ok || !outcome.value.accepted) {
          pendingSubmitRef.current.delete(item.id)
          setBusyId(null)
          setErrorText(outcome?.ok ? 'rejected' : (outcome?.message ?? 'rejected'))
        }
        return
      }

      const outcome = await api.invoke('humanWorkItems.submit', {
        requestId: crypto.randomUUID(),
        ownerScope: 'human-work-items',
        idempotencyKey: crypto.randomUUID(),
        itemId: item.id,
        expectedRevision: item.revision,
        response: answer
      })
      if (!outcome?.ok || !outcome.value.accepted) {
        setBusyId(null)
        setErrorText(outcome?.ok ? 'rejected' : (outcome?.message ?? 'rejected'))
      }
    },
    [api, drafts]
  )

  const cancelItem = useCallback(
    async (item: WorkItemSummary) => {
      if (!api) return
      setBusyId(item.id)
      setErrorText(null)
      const outcome = await api.invoke('humanWorkItems.cancel', {
        requestId: crypto.randomUUID(),
        ownerScope: 'human-work-items',
        idempotencyKey: crypto.randomUUID(),
        itemId: item.id,
        expectedRevision: item.revision
      })
      if (!outcome?.ok || !outcome.value.accepted) {
        setBusyId(null)
        setErrorText(outcome?.ok ? 'rejected' : (outcome?.message ?? 'rejected'))
      }
    },
    [api]
  )

  if (activeItems.length === 0 && !errorText) {
    return null
  }

  return (
    <section className="human-questions" aria-label={translate('Вопросы от модели')}>
      {activeItems.map((item) => {
        const details = detailsById[item.id]
        const schema = details?.response_schema
        const draft = drafts[item.id] ?? ''
        const isBusy = busyId === item.id

        return (
          <div className="approval task-timeline__approval human-questions__card" key={item.id} role="region" aria-label={item.title ?? item.id}>
            <strong>{translate('Вопрос от модели:')} {item.title ?? item.id}</strong>
            {details?.instructions ? <p className="human-questions__instructions">{details.instructions}</p> : null}

            {schema?.kind === 'choice' ? (
              <div className="human-questions__choices">
                {schema.choices.map((choice) => (
                  <button
                    key={choice}
                    type="button"
                    className={`human-questions__choice${draft === choice ? ' human-questions__choice--selected' : ''}`}
                    onClick={() => setDrafts((prev) => ({ ...prev, [item.id]: choice }))}
                    disabled={isBusy}
                  >
                    {choice}
                  </button>
                ))}
              </div>
            ) : (
              <textarea
                className="human-questions__input"
                rows={2}
                value={draft}
                placeholder={translate('Введите ответ…')}
                onChange={(event) => setDrafts((prev) => ({ ...prev, [item.id]: event.target.value }))}
                disabled={isBusy}
              />
            )}

            <div className="human-questions__actions">
              <button
                type="button"
                onClick={() => void submitAnswer(item)}
                disabled={isBusy || draft.trim().length === 0}
              >
                {translate('Отправить ответ')}
              </button>
              <button
                type="button"
                onClick={() => void cancelItem(item)}
                disabled={isBusy}
              >
                {translate('Отменить')}
              </button>
            </div>
            <small className="human-questions__hint">
              {translate('Ответ передаётся как данные (не выдаёт разрешений на инструменты).')}
            </small>
          </div>
        )
      })}
      {errorText ? <p role="alert">{translate('Ошибка:')}{translate(errorText)}</p> : null}
    </section>
  )
}
