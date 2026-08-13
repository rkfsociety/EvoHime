import { useCallback, useEffect, useState } from 'react'

import type { ConnectionState, WorkspaceOption, WorkspaceSelection } from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Workspace picker (plan 0, stage 3, slice 1).
 *
 * The renderer holds no filesystem access: picking a folder, remembering it and
 * checking whether it still exists all happen in the main process. This
 * component only renders the returned state and the failure cases.
 */

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

export interface WorkspacePickerProps {
  readonly connection: ConnectionState
}

type Status =
  | { readonly kind: 'loading' }
  | { readonly kind: 'ready'; readonly selection: WorkspaceSelection }
  | { readonly kind: 'failed'; readonly message: string }

export function WorkspacePicker({ connection }: WorkspacePickerProps): React.JSX.Element {
  const api = useShellApi()
  const [status, setStatus] = useState<Status>({ kind: 'loading' })
  const [busy, setBusy] = useState(false)

  const load = useCallback(async () => {
    if (!api) {
      setStatus({ kind: 'failed', message: 'Мост оболочки недоступен.' })
      return
    }
    const outcome = await api.invoke('workspace.list', {})
    setStatus(
      outcome.ok
        ? { kind: 'ready', selection: outcome.value }
        : { kind: 'failed', message: outcome.message }
    )
  }, [api])

  useEffect(() => {
    void load()
  }, [load])

  const run = useCallback(
    async (action: () => Promise<Status>) => {
      setBusy(true)
      try {
        setStatus(await action())
      } finally {
        setBusy(false)
      }
    },
    []
  )

  const pick = useCallback(async () => {
    if (!api) {
      return
    }
    await run(async () => {
      const outcome = await api.invoke('workspace.pick', {})
      return outcome.ok
        ? { kind: 'ready', selection: outcome.value.selection }
        : { kind: 'failed', message: outcome.message }
    })
  }, [api, run])

  const select = useCallback(
    async (path: string) => {
      if (!api) {
        return
      }
      await run(async () => {
        const outcome = await api.invoke('workspace.select', { path })
        return outcome.ok
          ? { kind: 'ready', selection: outcome.value }
          : { kind: 'failed', message: outcome.message }
      })
    },
    [api, run]
  )

  const forget = useCallback(
    async (path: string) => {
      if (!api) {
        return
      }
      await run(async () => {
        const outcome = await api.invoke('workspace.forget', { path })
        return outcome.ok
          ? { kind: 'ready', selection: outcome.value }
          : { kind: 'failed', message: outcome.message }
      })
    },
    [api, run]
  )

  if (status.kind === 'loading') {
    return (
      <section className="shell__panel" aria-busy="true">
        <h2>Рабочая папка</h2>
        <p className="shell__empty">Загружаю список…</p>
      </section>
    )
  }

  if (status.kind === 'failed') {
    return (
      <section className="shell__panel">
        <h2>Рабочая папка</h2>
        <p role="alert" className="shell__reason">
          {status.message}
        </p>
        <button type="button" onClick={() => void load()}>
          Повторить
        </button>
      </section>
    )
  }

  const { selected, options } = status.selection
  const selectedOption = options.find((option) => option.path === selected) ?? null
  const connected = CONNECTED_STATES.includes(connection)

  return (
    <section className="shell__panel">
      <h2>Рабочая папка</h2>

      {selected === null ? (
        <p className="shell__empty">Папка не выбрана — выбери её, чтобы начать работу.</p>
      ) : (
        <p className="shell__selected">
          <span className="shell__path">{selected}</span>
          {selectedOption?.available === false ? (
            <span role="alert" className="shell__reason">
              Папка недоступна: её переименовали, удалили или диск не подключён.
            </span>
          ) : null}
        </p>
      )}

      {!connected ? (
        <p className="shell__reason">
          Core недоступен, задачи не запускаются. Выбор папки сохранится и применится после
          восстановления связи.
        </p>
      ) : null}

      <button type="button" onClick={() => void pick()} disabled={busy}>
        Выбрать папку…
      </button>

      {options.length > 0 ? (
        <ul className="shell__workspaces">
          {options.map((option) => (
            <WorkspaceRow
              key={option.path}
              option={option}
              isSelected={option.path === selected}
              busy={busy}
              onSelect={() => void select(option.path)}
              onForget={() => void forget(option.path)}
            />
          ))}
        </ul>
      ) : null}
    </section>
  )
}

interface WorkspaceRowProps {
  readonly option: WorkspaceOption
  readonly isSelected: boolean
  readonly busy: boolean
  readonly onSelect: () => void
  readonly onForget: () => void
}

function WorkspaceRow({
  option,
  isSelected,
  busy,
  onSelect,
  onForget
}: WorkspaceRowProps): React.JSX.Element {
  return (
    <li className={option.available ? 'shell__workspace' : 'shell__workspace shell__workspace--missing'}>
      <button
        type="button"
        onClick={onSelect}
        disabled={busy || isSelected}
        aria-current={isSelected ? 'true' : undefined}
      >
        {option.path}
      </button>
      {!option.available ? <span className="shell__badge">недоступна</span> : null}
      <button type="button" onClick={onForget} disabled={busy} aria-label={`Забыть ${option.path}`}>
        Забыть
      </button>
    </li>
  )
}
