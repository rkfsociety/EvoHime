import type { ConnectionState, CoreEvent } from '@shared/api'
import type { JSX } from 'react'
import { useShellApi } from './shell-api'

export function ContextNamespacePanel({ connection, events }: { readonly connection: ConnectionState; readonly events: readonly CoreEvent[] }): JSX.Element {
  const api = useShellApi()
  const latest = events.find((event) => event.contextNamespace !== null && event.contextNamespace !== undefined)?.contextNamespace ?? null
  const projection = latest?.projection as { nodes?: readonly Record<string, unknown>[]; trace?: Record<string, unknown>; status?: string } | null | undefined
  const nodes = Array.isArray(projection?.nodes) ? projection.nodes : []
  const trace = projection?.trace
  const traceVisits = Array.isArray(trace?.['visited_nodes']) ? trace['visited_nodes'] as readonly Record<string, unknown>[] : []
  const fallbackPath = Array.isArray(trace?.['fallback_path']) ? trace['fallback_path'].map(String) : []
  const tokenContributions = trace?.['token_contributions'] && typeof trace['token_contributions'] === 'object' ? Object.entries(trace['token_contributions'] as Record<string, unknown>) : []
  const request = async (operation: 'list_children' | 'search_within' | 'get_abstract' | 'get_overview' | 'resolve_detail' | 'explain_selection') => {
    if (!api || connection !== 'connected') return
    await api.invoke('core.contextNamespace', { operation, namespaceId: operation === 'explain_selection' ? 'trace' : 'root', payload: operation === 'search_within' ? 'context' : '' })
  }
  return <section className="panel" aria-label="Context Namespace Explorer">
    <h2>Context Namespace</h2>
    <p>Read-only каталог и retrieval trace из Core. Logical path не является ACL.</p>
    <p>Операции чтения принимают Core-issued ContextViewSnapshot; панель не создаёт
      такой snapshot и не подменяет им policy.</p>
    <div className="panel__actions">
      {(['list_children', 'search_within', 'get_abstract', 'get_overview', 'resolve_detail', 'explain_selection'] as const).map((operation) => <button key={operation} type="button" onClick={() => void request(operation)} disabled={!api || connection !== 'connected'}>{operation}</button>)}
    </div>
    <p role="status">Статус: {latest?.status ?? 'unknown'}</p>
    {nodes.length > 0 ? <ul aria-label="Context nodes">{nodes.map((node, index) => <li key={`${String(node['node_id'] ?? 'node')}-${index}`}>{String(node['display_name'] ?? node['stable_ref'] ?? 'node')} · {String(node['kind'] ?? 'unknown')} · {String(node['freshness'] ?? 'unknown')}</li>)}</ul> : <p>Дерево пока не загружено.</p>}
    <div aria-label="Context trace">
      <p>Trace: {trace ? 'доступен' : 'нет данных'} · Core revision: {latest?.revision ?? 0}</p>
      {trace ? <>
        <p>Посещения: {traceVisits.length} · Выбрано: {Array.isArray(trace['selected_projections']) ? trace['selected_projections'].length : 0} · Index: {String(trace['index_health'] ?? 'unknown')}</p>
        <p>Fallback: {fallbackPath.length > 0 ? fallbackPath.join(' → ') : 'нет'}</p>
        {tokenContributions.length > 0 ? <ul aria-label="Token contributions">{tokenContributions.map(([key, value]) => <li key={key}>{key}: {String(value)}</li>)}</ul> : null}
        {traceVisits.length > 0 ? <ul aria-label="Trace visits">{traceVisits.slice(0, 64).map((visit, index) => <li key={`${String(visit['node_id'] ?? 'node')}-${index}`}>{String(visit['node_id'] ?? 'node')} · {String(visit['action'] ?? 'unknown')} · {String(visit['reason_code'] ?? 'unknown')}</li>)}</ul> : null}
      </> : null}
    </div>
  </section>
}
