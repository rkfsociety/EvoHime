/** @vitest-environment jsdom */
import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { RemoteConversationChannelsPanel } from '../src/renderer/src/RemoteConversationChannelsPanel'
vi.mock('../src/renderer/src/shell-api', () => ({ useShellApi: () => null }))
describe('remote conversation channels panel', () => { it('is bounded and metadata-only', () => { render(<RemoteConversationChannelsPanel connection="disconnected" events={[]} />); expect(screen.getByRole('region', { name: 'Remote Conversation Channels' })).toBeTruthy(); expect(screen.getByRole('button', { name: 'revoke' })).toBeTruthy(); }) })
