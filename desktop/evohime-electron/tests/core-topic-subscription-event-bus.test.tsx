// @vitest-environment jsdom
import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { CoreTopicSubscriptionEventBusPanel } from '../src/renderer/src/CoreTopicSubscriptionEventBusPanel'
describe('CoreTopicSubscriptionEventBusPanel',()=>{it('shows publish and durable delivery actions',()=>{render(<CoreTopicSubscriptionEventBusPanel connection="disconnected"/>);expect(screen.getByRole('region',{name:'Core Topic Subscription Event Bus'})).toBeTruthy();expect(screen.getByRole('button',{name:'nack'})).toBeTruthy()})})
