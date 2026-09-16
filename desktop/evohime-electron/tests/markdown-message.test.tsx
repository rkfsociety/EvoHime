// @vitest-environment jsdom
import { render, screen } from '@testing-library/react'
import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { describe, expect, it, vi } from 'vitest'

import { MarkdownMessage } from '../src/renderer/src/MarkdownMessage'

describe('markdown message', () => {
  it('renders common Markdown structures', () => {
    render(
      <MarkdownMessage
        text={'## Результат\n\n**Готово**\n\n- первый пункт\n- второй пункт\n\n```powershell\nGet-ChildItem\n```'}
      />
    )

    expect(screen.getByRole('heading', { level: 2, name: 'Результат' })).toBeTruthy()
    expect(screen.getByText('Готово').tagName).toBe('STRONG')
    expect(screen.getByText('первый пункт')).toBeTruthy()
    expect(screen.getByText('Get-ChildItem')).toBeTruthy()
  })

  it('removes executable HTML and unsafe links', () => {
    const { container } = render(
      <MarkdownMessage
        text={'Безопасный текст\n\n<script>alert(1)</script><img src="x" onerror="alert(1)">\n\n[опасная ссылка](javascript:alert(1))'}
      />
    )

    expect(container.querySelector('script')).toBeNull()
    expect(container.querySelector('[onerror]')).toBeNull()
    expect(container.querySelector('a[href]')).toBeNull()
    expect(screen.getByText('опасная ссылка')).toBeTruthy()
    expect(screen.getByText('Безопасный текст')).toBeTruthy()
  })

  it('does not re-parse or re-sanitize unchanged text', () => {
    const parse = vi.spyOn(marked, 'parse')
    const sanitize = vi.spyOn(DOMPurify, 'sanitize')
    const { rerender } = render(<MarkdownMessage text="Один и тот же ответ" />)

    rerender(<MarkdownMessage text="Один и тот же ответ" />)
    expect(parse).toHaveBeenCalledTimes(1)
    expect(sanitize).toHaveBeenCalledTimes(1)

    rerender(<MarkdownMessage text="Обновлённый ответ" />)
    expect(parse).toHaveBeenCalledTimes(2)
    expect(sanitize).toHaveBeenCalledTimes(2)
  })
})
