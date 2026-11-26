import { translate } from './i18n'
import { useEffect, useMemo, useState } from 'react'

import { PROVIDER_KINDS, type ChatProviderMode, type CodexStatus, type ConnectionState } from '@shared/api'

import { useShellApi } from './shell-api'
import { PROVIDER_PROFILE_LABELS } from './provider-labels'
import { useProviderState } from './provider-state'
import { UiIcon } from './UiIcon'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

const LABELS: Record<ChatProviderMode, string> = {
  literouter: 'LiteRouter',
  openai_compatible: 'OpenAI API',
  openai_responses: 'OpenAI Responses',
  ollama: 'Ollama',
  codex_cli: 'Codex CLI'
}

export interface ChatProviderPickerProps {
  readonly connection: ConnectionState
  readonly value: ChatProviderMode
  readonly onChange: (value: ChatProviderMode) => void
  readonly disabled?: boolean
  readonly perChat?: boolean
}

/** One provider/mode choice for the next chat task. */
export function ChatProviderPicker({ connection, value, onChange, disabled = false, perChat = false }: ChatProviderPickerProps): React.JSX.Element {
  const api = useShellApi()
  const { summary, apply } = useProviderState()
  const [codex, setCodex] = useState<CodexStatus | null>(null)
  const [switching, setSwitching] = useState(false)
  const connected = CONNECTED_STATES.includes(connection)

  useEffect(() => {
    if (!api) return
    void api.invoke('codex.getStatus', {}).then((outcome) => {
      if (outcome.ok) setCodex(outcome.value)
    })
  }, [api])

  const options = useMemo(() => {
    const apiOptions = PROVIDER_KINDS.flatMap((provider) => {
      if (provider !== 'openai_compatible') {
        return summary?.profiles?.[provider]?.configured === true ? [provider] : []
      }
      const hasCompatible = Object.values(summary?.compatibleProfiles ?? {}).some((profile) => profile?.configured === true)
      if (hasCompatible) return [provider]
      return summary?.profiles?.openai_compatible?.configured === true ? [provider] : []
    })
    return [
      ...apiOptions.map((provider) => ({
        value: provider as ChatProviderMode,
        label: provider === 'openai_compatible'
          ? PROVIDER_PROFILE_LABELS[summary?.profileId ?? summary?.profiles?.openai_compatible?.profileId ?? 'custom']
          : LABELS[provider]
      })),
      // Installation and readiness are different states. Keep an installed
      // CLI selectable even after a transient app-server/auth failure; the
      // model picker performs a fresh refresh when this route is selected.
      ...(codex?.installed ? [{ value: 'codex_cli' as const, label: LABELS.codex_cli }] : [])
    ]
  }, [codex?.installed, summary?.compatibleProfiles, summary?.profiles])

  useEffect(() => {
    if (!summary) return
    if (options.some((option) => option.value === value)) return
    const preferred = summary.provider && options.some((option) => option.value === summary.provider)
      ? summary.provider
      : options[0]?.value
    if (preferred) onChange(preferred)
  }, [onChange, options, summary, value])

  // The settings surface changes the active provider for the next task too.
  // Keep the composer selection tied to that same Core-backed snapshot rather
  // than leaving a valid but stale localStorage choice selected.
  useEffect(() => {
    if (perChat) return
    const preferred = summary?.provider
    // Codex CLI is a task route, not the persisted API-provider profile. Once
    // it is selected, the API profile must not immediately overwrite it.
    if (value === 'codex_cli' || !preferred || !options.some((option) => option.value === preferred) || preferred === value) return
    window.localStorage.setItem('evohime.chat-provider-mode', preferred)
    onChange(preferred)
  }, [onChange, options, perChat, summary?.provider, value])

  const select = async (next: ChatProviderMode): Promise<void> => {
    if (!api || next === value || switching) return
    if (!perChat && next !== 'codex_cli') {
      setSwitching(true)
      try {
        const outcome = await api.invoke('provider.select', { provider: next })
        if (!outcome.ok) return
        apply(outcome.value.summary)
      } finally {
        setSwitching(false)
      }
    }
    if (!perChat) {
      window.localStorage.setItem('evohime.chat-provider-mode', next)
    }
    onChange(next)
  }

  return (
    <label className="composer__provider">
      <span className="sr-only">{translate("Провайдер задачи")}</span>
      <select
        aria-label={translate("Провайдер задачи")}
        value={options.some((option) => option.value === value) ? value : ''}
        disabled={disabled || switching || !connected || options.length === 0}
        onChange={(event) => void select(event.target.value as ChatProviderMode)}
      >
        {options.length === 0 ? <option value="">{translate("Настрой провайдера")}</option> : null}
        {options.map((option) => <option key={option.value} value={option.value}>{translate(option.label)}</option>)}
      </select>
      <span className="composer__select-chevron" aria-hidden="true"><UiIcon name="chevron-down" size={12} /></span>
    </label>
  )
}
