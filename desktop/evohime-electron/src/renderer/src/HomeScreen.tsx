import { useCallback, useEffect, useState } from 'react'

import type { ChatSummary } from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * What the user sees before a conversation is open.
 *
 * Not a placeholder: it greets by name, offers the work already started in
 * this project, and suggests concrete first tasks. The composer below stays
 * usable — typing there creates the chat, so nothing has to be set up first.
 */

const SUGGESTIONS: readonly string[] = [
  'Изучи проект и расскажи, как он устроен',
  'Найди, где обрабатываются ошибки, и покажи слабые места',
  'Запусти тесты и объясни, что упало',
  'Опиши, что делает этот модуль'
]

export interface HomeScreenProps {
  readonly workspace: string | null
  readonly identityName: string | null
  readonly onOpenChat: (chatId: string) => void
  readonly onPickSuggestion: (prompt: string) => void
  /** Reloaded when the chat list changes elsewhere. */
  readonly revision: number
}

export function HomeScreen({
  workspace,
  identityName,
  onOpenChat,
  onPickSuggestion,
  revision
}: HomeScreenProps): React.JSX.Element {
  const api = useShellApi()
  const [recent, setRecent] = useState<readonly ChatSummary[]>([])

  const load = useCallback(async () => {
    if (!api || workspace === null) {
      setRecent([])
      return
    }
    const outcome = await api.invoke('chat.list', { workspacePath: workspace })
    if (outcome.ok) setRecent(outcome.value.slice(0, 5))
  }, [api, workspace])

  useEffect(() => {
    void load()
  }, [load, revision])

  return (
    <div className="home">
      <span className="home__logo" aria-hidden="true">E</span>
      <h2 className="home__greeting">
        {identityName ? `Чем займёмся, ${identityName}?` : 'Чем займёмся?'}
      </h2>
      <p className="home__subtitle">
        {workspace === null
          ? 'Выбери проект в левой панели — и можно ставить задачи.'
          : `Проект ${projectName(workspace)}. Опиши задачу внизу — я создам чат и возьмусь за неё.`}
      </p>

      {recent.length > 0 ? (
        <section className="home__section" aria-label="Недавние чаты">
          <h3>Продолжить</h3>
          <ul className="home__chats">
            {recent.map((chat) => (
              <li key={chat.id}>
                <button type="button" onClick={() => onOpenChat(chat.id)}>
                  <span className="home__chat-title">{chat.title}</span>
                  <span className="home__chat-meta">
                    {chat.messageCount > 0 ? `${plural(chat.messageCount)}` : 'пусто'}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {workspace !== null ? (
        <section className="home__section" aria-label="С чего начать">
          <h3>С чего начать</h3>
          <ul className="home__suggestions">
            {SUGGESTIONS.map((suggestion) => (
              <li key={suggestion}>
                <button type="button" onClick={() => onPickSuggestion(suggestion)}>
                  {suggestion}
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  )
}

function projectName(workspace: string): string {
  const parts = workspace.split(/[\\/]/).filter((part) => part.length > 0)
  return parts.at(-1) ?? workspace
}

function plural(count: number): string {
  const tail = count % 10
  const teen = count % 100
  if (teen >= 11 && teen <= 14) return `${count} сообщений`
  if (tail === 1) return `${count} сообщение`
  if (tail >= 2 && tail <= 4) return `${count} сообщения`
  return `${count} сообщений`
}
