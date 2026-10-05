// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import type { ShellEvent } from '../src/shared/api'
import { IntegrationProviderPanel } from '../src/renderer/src/IntegrationProviderPanel'

describe('GitHub public repository integration', () => {
  it('loads only the catalog and local saved list on open, then explicitly fetches a selected repository', async () => {
    let onShellEvent: ((event: ShellEvent) => void) | null = null
    const invoke = vi.fn().mockResolvedValue({ ok: true, value: { accepted: true } })
    const subscribe = vi.fn((listener: (event: ShellEvent) => void) => {
      onShellEvent = listener
      return () => { onShellEvent = null }
    })
    Object.defineProperty(window, 'evohime', {
      configurable: true,
      value: { v1: { invoke, subscribe, openExternal: vi.fn().mockResolvedValue(true) } }
    })
    render(<IntegrationProviderPanel />)

    await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2))
    expect(invoke.mock.calls.some(([command]) => command === 'integrationProvider.listCatalog')).toBe(true)
    expect(invoke.mock.calls.some(([, payload]) => (payload as { operation?: string }).operation === 'list_repositories')).toBe(true)
    expect(invoke.mock.calls.some(([, payload]) => (payload as { operation?: string }).operation === 'refresh_repository')).toBe(false)

    const catalogRequest = invoke.mock.calls.find(([command]) => command === 'integrationProvider.listCatalog')?.[1] as { requestId: string }
    const listRequest = invoke.mock.calls.find(([, payload]) => (payload as { operation?: string }).operation === 'list_repositories')?.[1] as { requestId: string }
    const emit = (requestId: string, operation: string, fields: object = {}) => {
      act(() => {
        onShellEvent?.({
          kind: 'core-event',
          event: {
            eventType: 'integration_provider_sdk.result',
            payload: JSON.stringify({ request_id: requestId, operation, status: 'ok', error_code: '', ...fields })
          }
        } as ShellEvent)
      })
    }
    emit(catalogRequest.requestId, 'list_catalog', { providers: [{ id: 'github.public', display_name: 'GitHub' }] })
    emit(listRequest.requestId, 'list_repositories', { repositories: [] })
    expect(await screen.findByText('Пока нет подключённых репозиториев.')).toBeTruthy()

    fireEvent.change(screen.getByLabelText('Публичный репозиторий owner/repo'), { target: { value: 'octocat/Hello-World' } })
    fireEvent.click(screen.getByRole('button', { name: 'Добавить' }))
    await waitFor(() => expect(invoke.mock.calls.some(([, payload]) => (payload as { operation?: string }).operation === 'add_repository')).toBe(true))
    const addRequest = invoke.mock.calls.find(([, payload]) => (payload as { operation?: string }).operation === 'add_repository')?.[1] as { requestId: string }
    emit(addRequest.requestId, 'add_repository', { added: true })

    await waitFor(() => expect(invoke.mock.calls.filter(([, payload]) => (payload as { operation?: string }).operation === 'list_repositories')).toHaveLength(2))
    const refreshedList = invoke.mock.calls.filter(([, payload]) => (payload as { operation?: string }).operation === 'list_repositories')[1]?.[1] as { requestId: string }
    emit(refreshedList.requestId, 'list_repositories', { repositories: [{ owner: 'octocat', repo: 'Hello-World', created_at_ms: 1 }] })
    fireEvent.click(await screen.findByRole('button', { name: 'octocat/Hello-World' }))

    await waitFor(() => expect(invoke.mock.calls.some(([, payload]) => (payload as { operation?: string }).operation === 'refresh_repository')).toBe(true))
    const refreshRequest = invoke.mock.calls.find(([, payload]) => (payload as { operation?: string }).operation === 'refresh_repository')?.[1] as { requestId: string }
    emit(refreshRequest.requestId, 'refresh_repository', {
      repository: {
        full_name: 'octocat/Hello-World', description: 'Example', language: 'TypeScript', stars: 1, forks: 0,
        open_issues: 1, issues: [{ number: 7, title: 'A public issue', url: 'https://github.com/octocat/Hello-World/issues/7' }],
        pull_requests: []
      }
    })
    expect(await screen.findByText(/#7 A public issue/)).toBeTruthy()

    vi.spyOn(window, 'confirm').mockReturnValue(true)
    fireEvent.click(screen.getByRole('button', { name: 'Удалить octocat/Hello-World' }))
    await waitFor(() => expect(invoke.mock.calls.some(([, payload]) => (payload as { operation?: string }).operation === 'remove_repository')).toBe(true))
    const removeRequest = invoke.mock.calls.find(([, payload]) => (payload as { operation?: string }).operation === 'remove_repository')?.[1] as { requestId: string }
    emit(removeRequest.requestId, 'remove_repository', { removed: true })
    await waitFor(() => expect(invoke.mock.calls.filter(([, payload]) => (payload as { operation?: string }).operation === 'list_repositories')).toHaveLength(3))
    const removedList = invoke.mock.calls.filter(([, payload]) => (payload as { operation?: string }).operation === 'list_repositories')[2]?.[1] as { requestId: string }
    emit(removedList.requestId, 'list_repositories', { repositories: [] })
    expect(await screen.findByText('Пока нет подключённых репозиториев.')).toBeTruthy()
  })
})
