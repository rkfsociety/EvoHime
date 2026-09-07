import { describe, expect, it } from 'vitest'
import { compareSemver, selectOutdatedModules, type ModuleVersionRecord } from '../src/main/update/module-versions'

const module = (name: string, version: string, dependencies: string[] = []): ModuleVersionRecord => ({
  module: name, version, artifact: `${name}.exe`, size: 1, sha256: 'a'.repeat(64), dependencies
})

describe('module versions', () => {
  it('compares semver numerically', () => {
    expect(compareSemver('1.10.0', '1.9.9')).toBe(1)
    expect(compareSemver('2.0.0', '2.0.0')).toBe(0)
  })

  it('selects only newer modules and their dependents', () => {
    const available = [module('core', '2.0.0'), module('ui-bundle', '1.0.0', ['core']), module('cli', '1.0.0')]
    expect(selectOutdatedModules({ core: '1.0.0', 'ui-bundle': '1.0.0', cli: '1.0.0' }, available).map((item) => item.module)).toEqual(['core', 'ui-bundle'])
  })

  it('rejects malformed versions', () => {
    expect(() => compareSemver('1.0', '1.0.0')).toThrow()
  })
})
