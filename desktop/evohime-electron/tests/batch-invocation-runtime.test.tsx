// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import { expect, test } from 'vitest'
import { BatchInvocationRuntimePanel } from '../src/renderer/src/BatchInvocationRuntimePanel'
test('batch invocation panel exposes Core-owned item lifecycle', () => { render(<BatchInvocationRuntimePanel connection="disconnected" />); expect(screen.getByText(/Каждый item имеет отдельный state/)).toBeTruthy(); expect(screen.getByRole('button', { name: 'Отправить в Core' })).toBeTruthy() })
