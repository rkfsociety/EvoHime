import { describe, expect, it } from 'vitest'

import { modelMetadataHint, parseCoreModelDescriptor } from '../src/shared/model-capabilities'

describe('model capabilities', () => {
  it('reads explicit tool support from the Core descriptor', () => {
    const descriptor = parseCoreModelDescriptor({
      id: 'future-model',
      limits: { context_tokens: 128000, max_output_tokens: 8192 },
      capabilities: [{ capability: 'tool_calls', state: 'supported', provenance: 'observed' }],
      privacy: 'provider_controlled',
      lifecycle: 'active'
    })

    expect(descriptor).not.toBeNull()
    expect(modelMetadataHint(descriptor ?? undefined, 'agent')).toContain('tool_calls подтверждён Core')
    expect(modelMetadataHint(descriptor ?? undefined, 'agent')).toContain('контекст 128k')
  })

  it('keeps unknown capability visibly unknown instead of guessing from the model name', () => {
    const descriptor = parseCoreModelDescriptor({ id: 'claude-haiku-4.5-cheap:free', capabilities: [] })

    expect(modelMetadataHint(descriptor ?? undefined, 'agent')).toContain('не подтверждён Core')
  })

  it('rejects malformed Core descriptors', () => {
    expect(parseCoreModelDescriptor({ id: '', capabilities: [] })).toBeNull()
  })
})
