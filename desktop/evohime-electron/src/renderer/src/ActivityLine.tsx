import { useState } from 'react'

import type { ToolCall } from './transcript'
import { toolLabel } from './tool-names'

/**
 * One stretch of tool work as a single line.
 *
 * While the agent works the line names what it is doing right now; once the
 * stretch ends it collapses to a count the user can expand to read the actual
 * outputs. This keeps a long run of calls from pushing the answer off screen.
 */

export interface ActivityLineProps {
  readonly calls: readonly ToolCall[]
  readonly running: boolean
}

export function ActivityLine({ calls, running }: ActivityLineProps): React.JSX.Element {
  const [open, setOpen] = useState(false)
  const current = calls.at(-1)

  return (
    <div className={`activity${running ? ' activity--running' : ''}`}>
      <button
        type="button"
        className="activity__summary"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <span className="activity__icon" aria-hidden="true">{running ? '◐' : '✓'}</span>
        <span className="activity__label">
          {running ? toolLabel(current?.tool ?? '') || 'Работаю' : summarize(calls)}
        </span>
        <span className="activity__chevron" aria-hidden="true">{open ? '▾' : '▸'}</span>
      </button>

      {open ? (
        <ol className="activity__calls">
          {calls.map((call, index) => (
            <li key={`${call.tool}-${index}`}>
              <span className="activity__tool">{toolLabel(call.tool)}</span>
              {call.running ? (
                <span className="activity__pending">выполняется…</span>
              ) : (
                <pre className="activity__output">{call.output || 'без вывода'}</pre>
              )}
            </li>
          ))}
        </ol>
      ) : null}
    </div>
  )
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
