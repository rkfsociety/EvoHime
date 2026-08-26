import { useEffect, useMemo, useState } from 'react'

import { PROVIDER_KINDS, type ChatProviderMode, type CodexStatus, type ConnectionState, type ProviderSummary } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

const LABELS: Record<ChatProviderMode, string> = {
  literouter: 'LiteRouter',
  openai_compatible: 'OpenAI API',
  openai_responses: 'OpenAI Responses',
  codex_cli: 'Codex CLI'
}

export interface ChatProviderPickerProps {
  readonly connection: ConnectionState
  readonly value: ChatProviderMode
  readonly onChange: (value: ChatProviderMode) => void
  readonly disabled?: boolean
}

/** One provider/mode choice for the next chat task. */
export function ChatProviderPicker({ connection, value, onChange, disabled = false }: ChatProviderPickerProps): React.JSX.Element {
  const api = useShellApi()
  const [summary, setSummary] = useState<ProviderSummary | null>(null)
  const [codex, setCodex] = useState<CodexStatus | null>(null)
  const connected = CONNECTED_STATES.includes(connection)

  useEffect(() => {
    if (!api) return
    void api.invoke('provider.get', {}).then((outcome) => {
      if (outcome.ok) setSummary(outcome.value)
    })
    void api.invoke('codex.getStatus', {}).then((outcome) => {
      if (outcome.ok) setCodex(outcome.value)
    })
  }, [api])

  const options = useMemo(() => {
    const apiOptions = PROVIDER_KINDS.filter((provider) => summary?.profiles[provider].configured)
    return [
      ...apiOptions.map((provider) => ({ value: provider as ChatProviderMode, label: LABELS[provider] })),
      ...(codex?.available ? [{ value: 'codex_cli' as const, label: LABELS.codex_cli }] : [])
    ]
  }, [codex?.available, summary?.profiles])

  useEffect(() => {
    if (options.some((option) => option.value === value)) return
    const preferred = summary?.provider && options.some((option) => option.value === summary.provider)
      ? summary.provider
      : options[0]?.value
    if (preferred) onChange(preferred)
  }, [onChange, options, summary?.provider, value])

  const select = async (next: ChatProviderMode): Promise<void> => {
    if (!api || next === value) return
    if (next !== 'codex_cli') {
      const outcome = await api.invoke('provider.select', { provider: next })
      if (!outcome.ok) return
    }
    window.localStorage.setItem('evohime.chat-provider-mode', next)
    onChange(next)
  }

  return (
    <label className="composer__provider">
      <span className="sr-only">Провайдер задачи</span>
      <select
        aria-label="Провайдер задачи"
        value={options.some((option) => option.value === value) ? value : ''}
        disabled={disabled || !connected || options.length === 0}
        onChange={(event) => void select(event.target.value as ChatProviderMode)}
      >
        {options.length === 0 ? <option value="">Настрой провайдера</option> : null}
        {options.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
      </select>
    </label>
  )
}
