import { translate } from './i18n'
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'

import type { ShellEvent } from '@shared/api'

import { useShellApi } from './shell-api'

interface SavedRepository {
  readonly owner: string
  readonly repo: string
  readonly created_at_ms: number
}

interface RepositoryItem {
  readonly number: number
  readonly title: string
  readonly url: string
}

interface RepositoryProjection {
  readonly full_name: string
  readonly description: string | null
  readonly language: string | null
  readonly stars: number
  readonly forks: number
  readonly open_issues: number
  readonly issues: readonly RepositoryItem[]
  readonly pull_requests: readonly RepositoryItem[]
}

interface IntegrationResponse {
  readonly request_id: string
  readonly operation: string
  readonly status: string
  readonly error_code: string
  readonly providers?: readonly { readonly id: string; readonly display_name: string }[]
  readonly repositories?: readonly SavedRepository[]
  readonly repository?: RepositoryProjection
  readonly added?: boolean
}

const OWNER_SCOPE = 'settings'

function errorText(code: string): string {
  switch (code) {
    case 'invalid_repository': return 'Проверь формат owner/repo.'
    case 'repository_not_saved': return 'Сначала добавь этот репозиторий в список.'
    case 'repository_not_found': return 'Репозиторий не найден или он не публичный.'
    case 'github_rate_limited': return 'GitHub временно ограничил запросы. Попробуй позже.'
    case 'refresh_in_progress': return 'Уже загружаю данные GitHub.'
    case 'github_timeout': return 'GitHub не ответил вовремя.'
    case 'github_response_too_large': return 'Ответ GitHub превысил допустимый размер.'
    case 'github_invalid_response': return 'GitHub вернул данные в неожиданном формате.'
    case 'storage_error': return 'Не удалось сохранить список подключений.'
    case 'repository_limit': return 'В списке уже максимальные 50 репозиториев. Удали ненужный, чтобы добавить новый.'
    default: return 'Не удалось связаться с GitHub.'
  }
}

export function IntegrationProviderPanel(): React.JSX.Element {
  const api = useShellApi()
  const pending = useRef(new Map<string, string>())
  const busyRequests = useRef(new Set<string>())
  const [repositoryInput, setRepositoryInput] = useState('')
  const [repositories, setRepositories] = useState<readonly SavedRepository[]>([])
  const [selected, setSelected] = useState<RepositoryProjection | null>(null)
  const [selectedId, setSelectedId] = useState('')
  const [catalogReady, setCatalogReady] = useState(false)
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('Загружаю каталог и список подключений…')

  const send = useCallback(async (operation: string, payload?: { owner: string; repo: string }) => {
    if (!api) {
      setStatus('Core недоступен.')
      return
    }
    const requestId = crypto.randomUUID()
    pending.current.set(requestId, operation)
    busyRequests.current.add(requestId)
    setBusy(true)
    try {
      const outcome = await api.invoke('integrationProvider.command', {
        requestId,
        ownerScope: OWNER_SCOPE,
        operation,
        ...(payload ? { payload: JSON.stringify(payload) } : {}),
        idempotencyKey: requestId
      })
      if (outcome.ok && outcome.value.accepted) return
      pending.current.delete(requestId)
      setStatus('Core отклонил запрос интеграции.')
    } catch {
      pending.current.delete(requestId)
      setStatus('Не удалось отправить запрос интеграции в Core.')
    }
    busyRequests.current.delete(requestId)
    setBusy(busyRequests.current.size > 0)
  }, [api])

  const refreshList = useCallback(async () => {
    if (!api) return
    const requestId = crypto.randomUUID()
    pending.current.set(requestId, 'list_repositories')
    try {
      const outcome = await api.invoke('integrationProvider.command', {
        requestId,
        ownerScope: OWNER_SCOPE,
        operation: 'list_repositories',
        idempotencyKey: requestId
      })
      if (outcome.ok && outcome.value.accepted) return
      pending.current.delete(requestId)
      setStatus('Core не смог прочитать список подключений.')
    } catch {
      pending.current.delete(requestId)
      setStatus('Не удалось прочитать список подключений из Core.')
    }
  }, [api])

  useEffect(() => {
    if (!api) {
      setStatus('Core недоступен.')
      return
    }
    const unsubscribe = api.subscribe((event: ShellEvent) => {
      if (event.kind !== 'core-event' || event.event.eventType !== 'integration_provider_sdk.result') return
      let response: IntegrationResponse
      try {
        response = JSON.parse(event.event.payload) as IntegrationResponse
      } catch {
        pending.current.clear()
        busyRequests.current.clear()
        setBusy(false)
        setStatus('Core вернул некорректный ответ интеграции.')
        return
      }
      if (!pending.current.has(response.request_id)) return
      const operation = pending.current.get(response.request_id)
      pending.current.delete(response.request_id)
      busyRequests.current.delete(response.request_id)
      if (response.status !== 'ok') {
        setBusy(busyRequests.current.size > 0)
        setStatus(errorText(response.error_code))
        return
      }
      if (operation === 'list_catalog') {
        setCatalogReady(response.providers?.some((provider) => provider.id === 'github.public') ?? false)
      } else if (operation === 'list_repositories') {
        setRepositories(response.repositories ?? [])
        setStatus('Список подключений загружен.')
      } else if (operation === 'add_repository') {
        setRepositoryInput('')
        setStatus(response.added === false
          ? 'Этот репозиторий уже есть в списке.'
          : 'Публичный репозиторий добавлен. Открой карточку, чтобы загрузить данные.')
        void refreshList()
      } else if (operation === 'remove_repository') {
        setSelected(null)
        setSelectedId('')
        setStatus('Подключение удалено из EvoHime.')
        void refreshList()
      } else if (operation === 'refresh_repository') {
        setSelected(response.repository ?? null)
        setStatus(response.repository ? 'Данные загружены из GitHub.' : 'GitHub не вернул данные.')
      }
      setBusy(busyRequests.current.size > 0)
    })

    const catalogId = crypto.randomUUID()
    pending.current.set(catalogId, 'list_catalog')
    void api.invoke('integrationProvider.listCatalog', { requestId: catalogId, ownerScope: OWNER_SCOPE }).then((outcome) => {
      if (outcome.ok && outcome.value.accepted) return
      pending.current.delete(catalogId)
      setStatus('Каталог интеграций недоступен.')
    }).catch(() => {
        pending.current.delete(catalogId)
        setStatus('Каталог интеграций недоступен.')
    })
    void refreshList()
    return unsubscribe
  }, [api, refreshList])

  const addRepository = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const parts = repositoryInput.trim().split('/')
    if (parts.length !== 2 || !parts[0] || !parts[1]) {
      setStatus('Укажи репозиторий в формате owner/repo.')
      return
    }
    setSelected(null)
    setSelectedId(`${parts[0]}/${parts[1]}`)
    void send('add_repository', { owner: parts[0], repo: parts[1] })
  }

  const openRepository = (repository: SavedRepository) => {
    const id = `${repository.owner}/${repository.repo}`
    setSelectedId(id)
    setSelected(null)
    setStatus(`Загружаю ${id} из GitHub…`)
    void send('refresh_repository', { owner: repository.owner, repo: repository.repo })
  }

  const removeRepository = (repository: SavedRepository) => {
    const id = `${repository.owner}/${repository.repo}`
    if (!window.confirm(translate('Удалить {id} из списка интеграций?').replace('{id}', id))) return
    void send('remove_repository', { owner: repository.owner, repo: repository.repo })
  }

  return <section className="settings-info integration-provider" aria-label={translate("Интеграции")}>
    <h3>{translate("Интеграции")}</h3>
    <p>{translate("Подключай публичные GitHub-репозитории, чтобы смотреть их описание, открытые issues и pull requests. EvoHime отправляет только owner/repo в GitHub API; вход в аккаунт не требуется. GitHub может ограничить частоту анонимных запросов.")}</p>
    <span className="settings-info__badge">{translate(catalogReady ? translate("GitHub · только чтение") : translate("Каталог загружается"))}</span>

    <form className="integration-provider__form" onSubmit={addRepository}>
      <label htmlFor="integration-github-repo">{translate("Публичный репозиторий")}</label>
      <div className="integration-provider__add-row">
        <input
          id="integration-github-repo"
          aria-label={translate("Публичный репозиторий owner/repo")}
          placeholder="owner/repo"
          value={repositoryInput}
          onChange={(event) => setRepositoryInput(event.target.value)}
          maxLength={140}
        />
        <button type="submit" disabled={busy || !catalogReady}>{translate("Добавить")}</button>
      </div>
    </form>

    <p role="status" aria-live="polite">{translate(status)}</p>
    {repositories.length === 0 ? <p>{translate("Пока нет подключённых репозиториев.")}</p> : (
      <ul className="integration-provider__repos">
        {repositories.map((repository) => {
          const id = `${repository.owner}/${repository.repo}`
          return <li key={id}>
            <button type="button" disabled={busy} onClick={() => openRepository(repository)}>{translate(id)}</button>
            <button type="button" disabled={busy} aria-label={`Удалить ${id}`} onClick={() => removeRepository(repository)}>{translate("Удалить")}</button>
          </li>
        })}
      </ul>
    )}

    {selectedId && selected ? <div className="integration-provider__detail">
      <h4><button className="integration-provider__link" type="button" onClick={() => { void api?.openExternal(`https://github.com/${encodeURIComponent(selected.full_name.split('/')[0] ?? '')}/${encodeURIComponent(selected.full_name.split('/')[1] ?? '')}`) }}>{translate(selected.full_name)}</button></h4>
      {selected.description ? <p>{selected.description}</p> : null}
      <p>{translate(selected.language ?? translate("Язык не указан"))} · ★ {translate(selected.stars)} · forks {translate(selected.forks)}</p>
      <div className="integration-provider__columns">
        <section aria-label={translate("Открытые issues")}>
          <h4>{translate("Открытые issues")}</h4>
          {selected.issues.length ? <ul>{selected.issues.map((item) => <li key={item.number}><button className="integration-provider__link" type="button" onClick={() => { void api?.openExternal(item.url) }}>#{translate(item.number)} {item.title}</button></li>)}</ul> : <p>{translate("Нет открытых issues.")}</p>}
        </section>
        <section aria-label={translate("Открытые pull requests")}>
          <h4>{translate("Открытые pull requests")}</h4>
          {selected.pull_requests.length ? <ul>{selected.pull_requests.map((item) => <li key={item.number}><button className="integration-provider__link" type="button" onClick={() => { void api?.openExternal(item.url) }}>#{translate(item.number)} {item.title}</button></li>)}</ul> : <p>{translate("Нет открытых pull requests.")}</p>}
        </section>
      </div>
      <button type="button" disabled={busy} onClick={() => {
        const [owner, repo] = selected.full_name.split('/')
        if (owner && repo) void send('refresh_repository', { owner, repo })
      }}>{translate("Обновить")}</button>
    </div> : null}
  </section>
}
