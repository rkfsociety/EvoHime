import DOMPurify from 'dompurify'
import { marked } from 'marked'
import { memo, useEffect, useMemo, useState } from 'react'

interface MarkdownMessageProps {
  readonly text: string
}

/** Renders model output as safe, readable Markdown inside the transcript. */
export const MarkdownMessage = memo(function MarkdownMessage({ text }: MarkdownMessageProps): React.JSX.Element {
  const [renderedText, setRenderedText] = useState(text)
  useEffect(() => {
    if (text === renderedText) return
    const frame = window.requestAnimationFrame(() => setRenderedText(text))
    return () => window.cancelAnimationFrame(frame)
  }, [text, renderedText])
  const html = useMemo(() => renderMarkdown(renderedText), [renderedText])
  return <div className="markdown-message" dangerouslySetInnerHTML={{ __html: html }} />
}, (previous, next) => previous.text === next.text)

function renderMarkdown(text: string): string {
  const rendered = marked.parse(text, {
    async: false,
    breaks: true,
    gfm: true
  })

  return DOMPurify.sanitize(rendered, {
    FORBID_TAGS: ['button', 'embed', 'form', 'iframe', 'input', 'object', 'script', 'style'],
    FORBID_ATTR: ['autofocus', 'formaction', 'onerror', 'onclick', 'onload']
  })
}
