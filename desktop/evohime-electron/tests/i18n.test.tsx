// @vitest-environment jsdom
import { afterEach, describe, expect, it } from 'vitest'

import { formatDateTime, formatNumber, setAppLocale, translate } from '../src/renderer/src/i18n'

afterEach(() => setAppLocale('ru'))

describe('renderer localization', () => {
  it('translates interface copy and preserves unknown content', () => {
    setAppLocale('en')

    expect(translate('Настройки')).toBe('Settings')
    expect(translate('user supplied content')).toBe('user supplied content')
  })

  it('formats dates and numbers using the selected locale', () => {
    setAppLocale('en')

    expect(formatNumber(1234.5)).toBe('1,234.5')
    expect(formatDateTime(new Date('2026-10-06T12:00:00Z'))).toContain('Oct')
  })
})
