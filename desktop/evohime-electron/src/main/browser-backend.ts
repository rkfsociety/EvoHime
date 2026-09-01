import { app, BrowserWindow, session } from 'electron'
import { createInterface } from 'node:readline'
import { lookup } from 'node:dns/promises'
import { isIP } from 'node:net'

const MAX_URL = 8 * 1024
const MAX_TEXT = 16 * 1024
const MAX_ELEMENTS = 512
const BLOCKED_HOSTS = new Set(['localhost', 'localhost.localdomain', 'metadata', 'metadata.google.internal', 'metadata.goog'])

type Request = { id?: string; op?: string; url?: string; ref?: string; text?: string; key?: string; value?: string; delta?: number; fileName?: string; fileBase64?: string }

function blockedIp(address: string): boolean {
  if (isIP(address) === 4) {
    const octets = address.split('.').map(Number)
    const a = octets[0] ?? -1
    const b = octets[1] ?? -1
    return a === 0 || a === 10 || a === 127 || a === 169 && b === 254 || a === 172 && b >= 16 && b <= 31 || a === 192 && b === 168 || a === 100 && b >= 64 && b <= 127
  }
  const lower = address.toLowerCase()
  return lower === '::1' || lower === '::' || lower.startsWith('fc') || lower.startsWith('fd') || lower.startsWith('fe80:') || lower.startsWith('::ffff:127.')
}

async function assertSafeUrl(raw: string): Promise<URL> {
  if (raw.length === 0 || raw.length > MAX_URL) throw new Error('invalid_url')
  const url = new URL(raw)
  if (url.protocol !== 'http:' && url.protocol !== 'https:') throw new Error('scheme_denied')
  if (url.username || url.password) throw new Error('credentials_in_url_denied')
  const hostname = url.hostname.toLowerCase().replace(/\.$/, '')
  if (BLOCKED_HOSTS.has(hostname) || hostname.endsWith('.localhost') || hostname.endsWith('.local') || hostname.endsWith('.internal')) throw new Error('private_host_denied')
  if (isIP(hostname)) { if (blockedIp(hostname)) throw new Error('private_ip_denied'); return url }
  const addresses = await lookup(hostname, { all: true, verbatim: true })
  if (addresses.length === 0 || addresses.some(({ address }) => blockedIp(address))) throw new Error('resolved_private_ip_denied')
  return url
}

async function evaluate(window: BrowserWindow, script: string): Promise<unknown> {
  return window.webContents.executeJavaScript(script, true)
}

export async function runBrowserBackend(): Promise<void> {
  await app.whenReady()
  const partition = `evohime-browser-ephemeral-${process.pid}`
  const browserSession = session.fromPartition(partition, { cache: false })
  browserSession.webRequest.onBeforeRequest({ urls: ['http://*/*', 'https://*/*'] }, (details, callback) => {
    void assertSafeUrl(details.url).then(() => callback({})).catch(() => callback({ cancel: true }))
  })
  const window = new BrowserWindow({ show: false, webPreferences: { partition, sandbox: true, contextIsolation: true, nodeIntegration: false, webSecurity: true } })
  let revision = 0
  const line = createInterface({ input: process.stdin, crlfDelay: Infinity })
  const reply = (id: string, value: Record<string, unknown>) => process.stdout.write(`${JSON.stringify({ id, ...value })}\n`)
  line.on('line', (raw) => {
    void (async () => {
      let request: Request
      try { request = JSON.parse(raw) as Request } catch { reply('', { status: 'rejected', error_code: 'invalid_json' }); return }
      const id = typeof request.id === 'string' ? request.id : ''
      try {
        switch (request.op) {
          case 'navigate': {
            const url = await assertSafeUrl(request.url ?? '')
            await window.loadURL(url.toString())
            revision += 1
            reply(id, { status: 'ok', revision })
            return
          }
          case 'snapshot': {
            const result = await evaluate(window, `(() => { const nodes = Array.from(document.querySelectorAll('a,button,input,textarea,select,[role]')).slice(0, ${MAX_ELEMENTS}); const elements = nodes.map((el, i) => { const ref = 'e' + i; el.setAttribute('data-evohime-ref', ref); return { ref_id: ref, role: el.getAttribute('role') || el.tagName.toLowerCase(), name: (el.getAttribute('aria-label') || el.textContent || '').trim().slice(0, 256) }; }); return { url: location.href, title: document.title.slice(0, 512), text: (document.body?.innerText || '').slice(0, ${MAX_TEXT}), elements }; })()`)
            reply(id, { status: 'ok', revision, snapshot: result })
            return
          }
          case 'click': {
            if (!request.ref || !/^e[0-9]+$/.test(request.ref)) throw new Error('invalid_element_ref')
            const clicked = await evaluate(window, `(() => { const el = document.querySelector('[data-evohime-ref=${JSON.stringify(request.ref)}]'); if (!el) return false; el.click(); return true })()`)
            if (clicked !== true) throw new Error('stale_element_ref')
            revision += 1
            reply(id, { status: 'ok', revision })
            return
          }
          case 'fill': {
            if (!request.ref || !/^e[0-9]+$/.test(request.ref) || typeof request.text !== 'string' || request.text.length > MAX_TEXT) throw new Error('invalid_fill')
            const filled = await evaluate(window, `(() => { const el = document.querySelector('[data-evohime-ref=${JSON.stringify(request.ref)}]'); if (!el) return false; el.focus(); el.value = ${JSON.stringify(request.text)}; el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true })); return true })()`)
            if (filled !== true) throw new Error('stale_element_ref')
            revision += 1
            reply(id, { status: 'ok', revision, text_length: request.text.length })
            return
          }
          case 'back': await window.webContents.goBack(); revision += 1; reply(id, { status: 'ok', revision }); return
          case 'forward': await window.webContents.goForward(); revision += 1; reply(id, { status: 'ok', revision }); return
          case 'reload': await window.webContents.reload(); revision += 1; reply(id, { status: 'ok', revision }); return
          case 'scroll': {
            const delta = Math.max(-10000, Math.min(10000, Number(request.delta ?? 0)))
            await evaluate(window, `window.scrollBy(0, ${delta})`)
            revision += 1
            reply(id, { status: 'ok', revision })
            return
          }
          case 'press': {
            if (!request.key || request.key.length > 64) throw new Error('invalid_key')
            await evaluate(window, `document.activeElement?.dispatchEvent(new KeyboardEvent('keydown', { key: ${JSON.stringify(request.key)}, bubbles: true }))`)
            revision += 1
            reply(id, { status: 'ok', revision })
            return
          }
          case 'select': {
            if (!request.ref || !/^e[0-9]+$/.test(request.ref) || typeof request.value !== 'string' || request.value.length > 1024) throw new Error('invalid_select')
            const selected = await evaluate(window, `(() => { const el = document.querySelector('[data-evohime-ref=${JSON.stringify(request.ref)}]'); if (!el) return false; el.value = ${JSON.stringify(request.value)}; el.dispatchEvent(new Event('change', { bubbles: true })); return true })()`)
            if (selected !== true) throw new Error('stale_element_ref')
            revision += 1
            reply(id, { status: 'ok', revision })
            return
          }
          case 'wait': {
            const delay = Math.max(0, Math.min(5000, Number(request.delta ?? 0)))
            await new Promise(resolve => setTimeout(resolve, delay))
            reply(id, { status: 'ok', revision })
            return
          }
          case 'screenshot': {
            const png = (await window.webContents.capturePage()).toPNG()
            if (png.length > 16 * 1024 * 1024) throw new Error('artifact_too_large')
            reply(id, { status: 'ok', revision, artifact_base64: png.toString('base64'), content_type: 'image/png' })
            return
          }
          case 'upload': {
            if (!request.ref || !/^e[0-9]+$/.test(request.ref) || !request.fileBase64 || !request.fileName) throw new Error('invalid_upload')
            if (request.fileBase64.length > 2 * 1024 * 1024 || request.fileName.length > 128) throw new Error('artifact_too_large')
            const uploaded = await evaluate(window, `(() => { const el = document.querySelector('[data-evohime-ref=${JSON.stringify(request.ref)}]'); if (!el) return false; const bytes = Uint8Array.from(atob(${JSON.stringify(request.fileBase64)}), c => c.charCodeAt(0)); const file = new File([bytes], ${JSON.stringify(request.fileName)}, { type: 'application/octet-stream' }); const transfer = new DataTransfer(); transfer.items.add(file); el.files = transfer.files; el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true })); return true })()`)
            if (uploaded !== true) throw new Error('stale_element_ref')
            revision += 1
            reply(id, { status: 'ok', revision })
            return
          }
          case 'download': {
            if (!request.ref || !/^e[0-9]+$/.test(request.ref)) throw new Error('invalid_element_ref')
            const downloaded = await evaluate(window, `(async () => { const el = document.querySelector('[data-evohime-ref=${JSON.stringify(request.ref)}]'); if (!el || !(el instanceof HTMLAnchorElement)) return null; const response = await fetch(el.href); const blob = await response.blob(); if (blob.size > 16 * 1024 * 1024) throw new Error('artifact_too_large'); const buffer = await blob.arrayBuffer(); const bytes = new Uint8Array(buffer); let binary = ''; for (const byte of bytes) binary += String.fromCharCode(byte); return { name: (el.download || 'download.bin').slice(0, 128), base64: btoa(binary), type: blob.type || 'application/octet-stream' } })()`) as { name: string; base64: string; type: string } | null
            if (!downloaded) throw new Error('stale_element_ref')
            reply(id, { status: 'ok', revision, artifact_base64: downloaded.base64, file_name: downloaded.name, content_type: downloaded.type })
            return
          }
          case 'close':
            line.close(); await window.close(); reply(id, { status: 'ok' }); app.quit(); return
          default: throw new Error('unsupported_operation')
        }
      } catch (error) { reply(id, { status: 'rejected', error_code: error instanceof Error ? error.message : 'backend_error' }) }
    })()
  })
  process.stdin.resume()
}
