import { useCallback, useEffect, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

interface TerminalPanelProps {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

interface TerminalApproval {
  readonly approvalId: string
  readonly taskId: string
  readonly toolName: string
  readonly scope: string
}

export function TerminalPanel({ connection, events }: TerminalPanelProps): React.JSX.Element {
  const api = useShellApi()
  const connected = CONNECTED_STATES.includes(connection)
  const [workspacePath, setWorkspacePath] = useState<string | null>(null)
  const [program, setProgram] = useState('git')
  const [args, setArgs] = useState('status --short')
  const [cwd, setCwd] = useState('')
  const [output, setOutput] = useState<string | null>(null)
  const [approval, setApproval] = useState<TerminalApproval | null>(null)
  const [message, setMessage] = useState<string | null>(null)

  useEffect(() => {
    if (!api) return
    void api.invoke('workspace.list', {}).then((outcome) => {
      if (outcome.ok) setWorkspacePath(outcome.value.selected)
    })
  }, [api])

  useEffect(() => {
    const required = latestEvent(events, 'approval.required')
    if (required) {
      const payload = parseJson(required.payload)
      const approvalId = stringValue(payload, 'approval_id')
      const taskId = stringValue(payload, 'task_id')
      if (approvalId && taskId) {
        setApproval({
          approvalId,
          taskId,
          toolName: stringValue(payload, 'tool_name') ?? 'shell.execute',
          scope: stringValue(payload, 'scope') ?? 'workspace'
        })
      }
    }
    const result = latestEvent(events, 'terminal.result')
    if (result) {
      const payload = parseJson(result.payload)
      setOutput(typeof payload.output === 'string' ? payload.output : null)
      setMessage(payload.ok === false ? String(payload.error ?? 'Terminal завершился ошибкой.') : null)
      if (payload.ok === true) setApproval(null)
    }
  }, [events])

  const execute = useCallback(async (approvalId = '') => {
    if (!api || !workspacePath || !connected || program.trim().length === 0) return
    setMessage(null)
    const outcome = await api.invoke('core.terminalExecute', {
      taskId: approval?.taskId ?? makeTaskId(),
      workspacePath,
      program: program.trim(),
      args: splitArgs(args),
      cwd: cwd.trim(),
      timeoutMs: 30_000,
      approvalId
    })
    if (!outcome.ok) setMessage(outcome.message)
  }, [api, approval?.taskId, args, connected, cwd, program, workspacePath])

  return (
    <section className="shell__panel terminal-panel" aria-label="Bounded Terminal">
      <div className="terminal-panel__heading">
        <div>
          <h2>Bounded Terminal</h2>
          <p className="shell__empty">Команды выполняются только через Core policy и approval.</p>
        </div>
        <span className="terminal-panel__limit">timeout 30 c · output 512 KiB</span>
      </div>

      <div className="terminal-panel__form">
        <label>Программа<input value={program} onChange={(event) => setProgram(event.target.value)} /></label>
        <label>Аргументы<input value={args} onChange={(event) => setArgs(event.target.value)} /></label>
        <label>cwd внутри workspace<input value={cwd} onChange={(event) => setCwd(event.target.value)} placeholder="не задан" /></label>
        <button type="button" onClick={() => void execute()} disabled={!connected || !workspacePath || program.trim().length === 0}>Выполнить</button>
      </div>

      {!workspacePath ? <p className="shell__reason">Сначала выбери рабочую папку.</p> : null}
      {!connected ? <p className="shell__reason">Core недоступен: Terminal приостановлен.</p> : null}
      {message ? <p role="alert" className="shell__reason">{message}</p> : null}
      {approval ? (
        <div className="terminal-panel__approval" role="alert">
          <strong>Terminal требует approval: {approval.toolName}</strong>
          <span>{approval.scope}</span>
          <div>
            <button type="button" onClick={() => void execute(approval.approvalId)} disabled={!connected}>Разрешить выполнение</button>
            <button type="button" onClick={() => setApproval(null)}>Отклонить</button>
          </div>
        </div>
      ) : null}
      <pre className="terminal-panel__output">{output ?? 'Вывод появится после выполнения команды.'}</pre>
    </section>
  )
}

function latestEvent(events: readonly CoreEvent[], eventType: string): CoreEvent | null {
  return [...events].reverse().find((event) => event.eventType === eventType) ?? null
}

function parseJson(payload: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(payload)
    return typeof value === 'object' && value !== null ? value as Record<string, unknown> : {}
  } catch {
    return {}
  }
}

function stringValue(payload: Record<string, unknown>, key: string): string | null {
  return typeof payload[key] === 'string' && payload[key] ? String(payload[key]) : null
}

function splitArgs(value: string): string[] {
  return value.trim().length === 0 ? [] : value.trim().split(/\s+/).slice(0, 64)
}

function makeTaskId(): string {
  return typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function' ? crypto.randomUUID() : `terminal-${Date.now()}`
}
