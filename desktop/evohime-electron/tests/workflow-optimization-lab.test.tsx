// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { WorkflowOptimizationLabPanel } from '../src/renderer/src/WorkflowOptimizationLabPanel'
describe('WorkflowOptimizationLabPanel',()=>{it('shows bounded offline actions',()=>{render(<WorkflowOptimizationLabPanel connection="disconnected"/>);expect(screen.getByRole('region',{name:'Workflow Optimization Lab'})).toBeTruthy();expect(screen.getByRole('button',{name:'promote'})).toBeTruthy()})})
