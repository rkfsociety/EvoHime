import { useEffect, useState } from 'react'
import type { ConnectionState, ProjectInstructionStackProjection, ShellEvent } from '@shared/api'
import { useShellApi } from './shell-api'

const OPERATIONS = ['discover', 'compile', 'get', 'toggle'] as const
type Operation = typeof OPERATIONS[number]

export function ProjectInstructionStackPanel({ connection }: { readonly connection: ConnectionState }): React.JSX.Element {
  const api = useShellApi()
  const [operation, setOperation] = useState<Operation>('discover')
  const [workspaceRoot, setWorkspaceRoot] = useState('C:\\github\\EvoHime')
  const [payload, setPayload] = useState('')
  const [paths, setPaths] = useState('')
  const [projection, setProjection] = useState<ProjectInstructionStackProjection | null>(null)
  const [message, setMessage] = useState('')
  useEffect(() => api?.subscribe((event: ShellEvent) => {
    if (event.kind === 'core-event' && event.event.projectInstructionStack) setProjection(event.event.projectInstructionStack)
  }), [api])
  const send = async (): Promise<void> => {
    if (!api || connection !== 'connected') { setMessage('Нет подключения к Core.'); return }
    const result = await api.invoke('core.projectInstructionStack', { operation, workspaceRoot, payload, relevantPaths: paths.split(/[,\n]/).map(value => value.trim()).filter(Boolean) })
    setMessage(result.ok ? 'Запрос принят Core.' : result.message)
  }
  const data = projection?.projection && typeof projection.projection === 'object' && projection.projection !== null ? projection.projection as Record<string, unknown> : null
  const activeRules = Array.isArray(data?.['active_rules']) ? data['active_rules'] : []
  return <section aria-label="Project Instruction Stack"><h3>Project Instruction Stack</h3><p>Core-owned правила проекта: allowlisted discovery, AGENTS.md compatibility, path activation and deterministic precedence. Markdown не исполняется и не расширяет capabilities.</p><div aria-label="Active project rules"><strong>Активных правил:</strong> {String(data?.['rule_count'] ?? activeRules.length)} <strong>Snapshot hash:</strong> {String(data?.['content_hash'] ?? '—')} <strong>Budget:</strong> {String(data?.['total_bytes'] ?? 0)} bytes / {String(data?.['estimated_tokens'] ?? 0)} tokens</div><label>Операция <select value={operation} onChange={event => setOperation(event.target.value as Operation)}>{OPERATIONS.map(item => <option key={item}>{item}</option>)}</select></label><label>Workspace root<input value={workspaceRoot} onChange={event => setWorkspaceRoot(event.target.value)} /></label><label>Relevant paths<textarea aria-label="Relevant project paths" value={paths} onChange={event => setPaths(event.target.value)} /></label><label>Payload JSON<textarea aria-label="Project Instruction Stack JSON" value={payload} onChange={event => setPayload(event.target.value)} maxLength={64 * 1024} /></label><button type="button" onClick={() => void send()}>Отправить в Core</button>{message ? <p role="status">{message}</p> : null}{activeRules.length > 0 ? <ul aria-label="Active rules">{activeRules.map((rule, index) => <li key={index}>{typeof rule === 'object' && rule !== null ? String((rule as Record<string, unknown>)['id'] ?? 'rule') : 'rule'}</li>)}</ul> : null}</section>
}
