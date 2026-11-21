// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it } from 'vitest'

import { HiveGraph } from '../src/renderer/src/components/HiveGraph'

describe('HiveGraph', () => {
  afterEach(() => {
    cleanup()
  })

  it('renders queen node and active/idle agent nodes', () => {
    render(
      <HiveGraph
        queenName="EvoHime"
        queenActive={true}
        activeAgents={[
          { id: 'worker-run-1', role: 'developer', currentAction: 'executing', depth: 1, spawnedSubagents: 0, ceiling: 4 },
          { id: 'worker-idle-2', role: 'researcher', currentAction: 'idle', depth: 1, spawnedSubagents: 0, ceiling: 2 }
        ]}
      />
    )

    const graph = screen.getByTestId('hive-graph')
    expect(graph.getAttribute('data-queen-state')).toBe('active')
    expect(screen.getByTestId('hive-graph-queen')).toBeTruthy()
    expect(screen.getAllByTestId('hive-graph-agent')).toHaveLength(2)
    expect(screen.getAllByTestId('hive-graph-edge')).toHaveLength(2)
  })
})
