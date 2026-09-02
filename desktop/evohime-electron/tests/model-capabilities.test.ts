import { describe, expect, it } from 'vitest'

import { capabilityForModel, sortModelsForUse } from '../src/shared/model-capabilities'

describe('model capabilities', () => {
  it('allows the verified LiteRouter Haiku route for agent work', () => {
    expect(capabilityForModel('literouter', 'claude-haiku-4.5-cheap:free')).toMatchObject({ agent: true, text: true, rank: 100 })
  })

  it('keeps unverified future-provider models available for text only', () => {
    expect(capabilityForModel('literouter', 'new-local-model:free')).toMatchObject({ agent: false, text: true })
    expect(capabilityForModel('openai_compatible', 'future-model')).toMatchObject({ agent: false, text: true })
  })

  it('sorts verified agent models ahead of unknown and rejected models', () => {
    expect(sortModelsForUse('literouter', [
      'mythomax-l2-13b:free',
      'claude-haiku-4.5-cheap:free',
      'future-model:free'
    ], 'agent')).toEqual(['claude-haiku-4.5-cheap:free'])
  })
})
