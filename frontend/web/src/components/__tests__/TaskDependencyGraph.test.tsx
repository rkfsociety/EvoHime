import React from 'react';
import { render } from '@testing-library/react';
import { TaskDependencyGraph } from '../TaskDependencyGraph';
import { PlanStep } from '../../protocol';

// Mock ReactFlow to avoid complex setup
jest.mock('reactflow', () => ({
  __esModule: true,
  default: ({ nodes }: any) => <div data-testid="reactflow">{nodes.length} nodes</div>,
  Controls: () => null,
  Background: () => null,
  useNodesState: (initial: any) => [initial, jest.fn(), jest.fn()],
  useEdgesState: (initial: any) => [initial, jest.fn(), jest.fn()],
}));

describe('TaskDependencyGraph', () => {
  it('renders without crashing', () => {
    const steps: PlanStep[] = [];
    render(<TaskDependencyGraph steps={steps} />);
  });

  it('renders with steps', () => {
    const steps: PlanStep[] = [
      {
        id: 'step-1',
        tool_name: 'filesystem.read',
        description: 'Read file',
        depends_on: [],
      },
      {
        id: 'step-2',
        tool_name: 'filesystem.patch',
        description: 'Patch file',
        depends_on: ['step-1'],
      },
    ];

    const { container } = render(<TaskDependencyGraph steps={steps} />);
    expect(container.querySelector('.task-dependency-graph')).toBeInTheDocument();
  });

  it('renders with execution states', () => {
    const steps: PlanStep[] = [
      {
        id: 'step-1',
        tool_name: 'filesystem.read',
        description: 'Read file',
        depends_on: [],
      },
    ];

    const executionStates = {
      'step-1': { status: 'running' },
    };

    const { container } = render(
      <TaskDependencyGraph steps={steps} executionStates={executionStates} />
    );
    expect(container.querySelector('.task-dependency-graph')).toBeInTheDocument();
  });
});
