// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { ExecutionEnvironmentProfilesPanel } from '../src/renderer/src/ExecutionEnvironmentProfilesPanel'
test('environment profiles remain a projection-only Core surface', () => { render(<ExecutionEnvironmentProfilesPanel connection="disconnected" />); expect(screen.getByRole('region', { name: 'Execution Environment Profiles' })).toBeTruthy(); expect(screen.getByText(/safe boundary и activation выполняются исключительно Core/)).toBeTruthy() })
