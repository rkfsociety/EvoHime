import { describe, expect, it } from 'vitest'
import { parseRoutingTrace, routingViewState } from '../src/shared/routing-trace'

const trace = (overrides: Record<string, unknown> = {}) => JSON.stringify({ schema_version: 1, terminal_status: 'success', selected_route: 'local', reason_code: 'only_candidate', candidates: [{ route_id: 'local', health_state: 'healthy' }], fallback_count: 0, privacy_label: 'non_sensitive', trace_id: 't', run_id: 'r', sequence: 1, ...overrides })
describe('routing trace', () => {
  it('renders actual route and derives degraded only with an explicit hint', () => {
    const parsed = parseRoutingTrace(trace())!
    expect(routingViewState(parsed, null)).toBe('normal')
    expect(routingViewState(parsed, 'cloud')).toBe('degraded')
  })
  it('does not partially render malformed or incompatible payloads', () => {
    expect(parseRoutingTrace(trace({ schema_version: 2 }))).toBeNull()
    expect(parseRoutingTrace(trace({ selected_route: null }))).toBeNull()
  })
  it('keeps refusals distinct from fallback', () => {
    const parsed = parseRoutingTrace(trace({ terminal_status: 'no_routes_configured', selected_route: null, candidates: [], safe_next_action: 'contact_support' }))!
    expect(routingViewState(parsed, 'cloud')).toBe('refusal')
  })
})
