import { translate } from './i18n'
import { useState } from 'react'

import type { ToolCall } from './transcript'
import { toolLabel } from './tool-names'
import { UiIcon } from './UiIcon'

/**
 * One stretch of tool work as a stable summary line with an optional subline.
 *
 * The summary line always keeps the count and categories so the header never
 * flickers or jumps in width between consecutive tool calls. While a tool call
 * is actively running, the current action is shown beneath it until the user
 * expands the full list or the call finishes.
 */

export interface ActivityLineProps {
  readonly calls: readonly ToolCall[]
  readonly running: boolean
  /** True while this activity stretch is still the active tail of a running task. */
  readonly active?: boolean
}

export function ActivityLine({ calls, running, active = false }: ActivityLineProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const [expandedCalls, setExpandedCalls] = useState<ReadonlySet<number>>(() => new Set())
  const current = calls.at(-1)
  const inProgress = running || active
  const isCurrentRunning = running || Boolean(current?.running)
  const showCurrentSubline = !open && inProgress && isCurrentRunning && current !== undefined

  function toggleCall(index: number): void {
    setExpandedCalls((prev) => {
      const next = new Set(prev)
      if (next.has(index)) {
        next.delete(index)
      } else {
        next.add(index)
      }
      return next
    })
  }

  return (
    <div className={`activity${inProgress ? ' activity--running' : ''}`}>
      <button
        type="button"
        className="activity__summary"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="activity__icon" aria-hidden="true">
          <UiIcon name={inProgress ? 'loader' : 'check'} size={14} />
        </span>
        <span className="activity__label">
          {translate(summarize(calls))}
        </span>
        <span className="activity__chevron" aria-hidden="true">
          <UiIcon name={open ? 'chevron-down' : 'chevron-right'} size={12} />
        </span>
      </button>

      {showCurrentSubline ? (
        <div
          className={`activity__current${running ? ' activity__current--live' : ''}`}
          aria-live="polite"
        >
          <span className="activity__current-dot" aria-hidden="true" />
          <span className="activity__current-text">
            {translate(currentActionLabel(current))}
          </span>
        </div>
      ) : null}

      {open ? (
        <ol className="activity__calls" aria-label={translate("Шаги работы модели")}>
          {calls.map((call, index) => {
            const isExpanded = expandedCalls.has(index)
            const detail = callDetail(call.output)
            return (
              <li key={`${call.tool}-${index}`} className="activity__call">
                <button
                  type="button"
                  className="activity__call-toggle"
                  onClick={() => toggleCall(index)}
                  aria-expanded={isExpanded}
                >
                  <span className="activity__call-icon" aria-hidden="true">{call.running ? '◌' : '·'}</span>
                  <span className="activity__call-copy">
                    <span className="activity__tool">{translate(toolLabel(call.tool))}</span>
                    {detail ? <span className="activity__detail">{translate(detail)}</span> : null}
                  </span>
                  {call.running ? (
                    <span className="activity__pending">{translate("выполняется…")}</span>
                  ) : null}
                  <span className="activity__chevron" aria-hidden="true">
                    <UiIcon name={isExpanded ? 'chevron-down' : 'chevron-right'} size={12} />
                  </span>
                </button>
                {isExpanded ? (
                  <pre className="activity__output">{call.output || translate("без вывода")}</pre>
                ) : null}
              </li>
            )
          })}
        </ol>
      ) : null}
    </div>
  )
}

function currentActionLabel(call: ToolCall): string {
  const tool = toolLabel(call.tool) || 'действие'
  const detail = liveDetail(call.output ?? '') || callDetail(call.output)
  return detail ? `Выполняю: ${tool} — ${detail}` : `Выполняю: ${tool}`
}

function liveDetail(output: string): string {
  const lines = output.split(/\r?\n/).reverse()
  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed) continue
    try {
      const event = JSON.parse(trimmed) as {
        item?: { type?: string; command?: string; text?: string }
        type?: string
      }
      if (event.item?.type === 'command_execution' && event.item.command) {
        return shorten(event.item.command)
      }
      if (event.item?.type === 'agent_message') return 'формирую ответ'
      if (event.type === 'turn.started') return 'подготавливаю ответ'
    } catch {
      // A chunk can end in the middle of a JSONL record; wait for the next one.
    }
  }
  return ''
}

/** Extract only useful navigation metadata for the always-visible progress row. */
function callDetail(output: string | null): string {
  if (!output) return ''
  const lines = output.split(/\r?\n/).reverse()
  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed) continue
    try {
      const event = JSON.parse(trimmed) as Record<string, unknown>
      const item = event.item && typeof event.item === 'object' ? event.item as Record<string, unknown> : event
      for (const key of ['path', 'pattern', 'include', 'command', 'query']) {
        const value = item[key]
        if (typeof value === 'string' && value.trim()) return shorten(value)
      }
    } catch {
      // Tool output is commonly JSONL; ignore partial records and keep the row compact.
    }
  }
  return ''
}

function shorten(value: string): string {
  const compact = value.replace(/\s+/g, ' ').trim()
  return compact.length > 140 ? `${compact.slice(0, 137)}…` : compact
}

/** "3 действия · читаю файл, ищу по файлам" */
function summarize(calls: readonly ToolCall[]): string {
  const names = [...new Set(calls.map((call) => toolLabel(call.tool).toLowerCase()))]
  const shown = names.slice(0, 3).join(', ')
  const rest = names.length > 3 ? ` и ещё ${names.length - 3}` : ''
  return `${plural(calls.length)} · ${shown}${rest}`
}

function plural(count: number): string {
  const tail = count % 10
  const teen = count % 100
  if (teen >= 11 && teen <= 14) return `${count} действий`
  if (tail === 1) return `${count} действие`
  if (tail >= 2 && tail <= 4) return `${count} действия`
  return `${count} действий`
}
