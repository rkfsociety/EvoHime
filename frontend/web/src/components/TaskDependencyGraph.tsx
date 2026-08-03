import React, { useEffect, useState } from 'react';
import ReactFlow, {
  Node,
  Edge,
  Controls,
  Background,
  useNodesState,
  useEdgesState,
} from 'reactflow';
import 'reactflow/dist/style.css';
import { PlanStep } from '../protocol';
import './TaskDependencyGraph.css';

interface TaskDependencyGraphProps {
  steps: PlanStep[];
  executionStates?: Record<string, { status: string }>;
}

export function TaskDependencyGraph({
  steps,
  executionStates = {},
}: TaskDependencyGraphProps) {
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);

  useEffect(() => {
    if (steps.length === 0) {
      setNodes([]);
      setEdges([]);
      return;
    }

    // Build nodes from steps
    // Use a simple grid layout: arrange by batch level
    const batchLevels = new Map<string, number>();
    const visited = new Set<string>();

    // Compute batch level for each step (topological depth)
    const computeLevel = (stepId: string): number => {
      if (batchLevels.has(stepId)) {
        return batchLevels.get(stepId)!;
      }

      const step = steps.find(s => s.id === stepId);
      const deps = step?.depends_on ?? [];
      if (!step || deps.length === 0) {
        batchLevels.set(stepId, 0);
        return 0;
      }

      const maxDependencyLevel = Math.max(
        ...deps.map(dep => computeLevel(dep))
      );
      const level = maxDependencyLevel + 1;
      batchLevels.set(stepId, level);
      return level;
    };

    steps.forEach(step => computeLevel(step.id));

    // Group steps by level
    const levelGroups = new Map<number, PlanStep[]>();
    steps.forEach(step => {
      const level = batchLevels.get(step.id) || 0;
      if (!levelGroups.has(level)) {
        levelGroups.set(level, []);
      }
      levelGroups.get(level)!.push(step);
    });

    // Create nodes with computed positions
    const newNodes: Node[] = [];
    levelGroups.forEach((stepsInLevel, level) => {
      const stepsPerRow = Math.ceil(Math.sqrt(stepsInLevel.length));
      stepsInLevel.forEach((step, index) => {
        const x = (index % stepsPerRow) * 180;
        const y = level * 200;

        newNodes.push({
          id: step.id,
          data: {
            label: (
              <div className="step-node-content">
                <div className="step-id">{step.id}</div>
                <div className="step-tool">{step.tool_name}</div>
              </div>
            ),
          },
          position: { x, y },
          className: `step-node status-${executionStates[step.id]?.status || 'pending'}`,
        });
      });
    });

    // Build edges from dependencies
    const newEdges: Edge[] = [];
    steps.forEach(step => {
      const deps = step.depends_on ?? [];
      deps.forEach(dep => {
        newEdges.push({
          id: `${dep}->${step.id}`,
          source: dep,
          target: step.id,
        });
      });
    });

    setNodes(newNodes);
    setEdges(newEdges);
  }, [steps, executionStates, setNodes, setEdges]);

  return (
    <div className="task-dependency-graph">
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        fitView
      >
        <Background />
        <Controls />
      </ReactFlow>
    </div>
  );
}
