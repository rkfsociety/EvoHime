import DOMPurify from 'dompurify'
import { marked } from 'marked'

interface MarkdownMessageProps {
  readonly text: string
}

/** Renders model output as safe, readable Markdown inside the transcript. */
export function MarkdownMessage({ text }: MarkdownMessageProps): React.JSX.Element {
  const html = renderMarkdown(text)
  return <div className="markdown-message" dangerouslySetInnerHTML={{ __html: html }} />
}

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
