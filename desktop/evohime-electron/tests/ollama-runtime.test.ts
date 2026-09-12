import { stat } from 'node:fs/promises'

import { describe, expect, it, vi } from 'vitest'

import {
  isAllowedInstallerUrl,
  OllamaRuntimeService,
  OLLAMA_API_URL,
  OLLAMA_INSTALLER_URL
} from '../src/main/ollama-runtime'

describe('Ollama runtime service', () => {
  it('allows only the official download and its known release hosts', () => {
    expect(isAllowedInstallerUrl(OLLAMA_INSTALLER_URL)).toBe(true)
    expect(isAllowedInstallerUrl('https://github.com/ollama/ollama/releases/download/v0.34.0/OllamaSetup.exe')).toBe(true)
    expect(isAllowedInstallerUrl('https://release-assets.githubusercontent.com/asset/OllamaSetup.exe')).toBe(true)
    expect(isAllowedInstallerUrl('http://ollama.com/download/OllamaSetup.exe')).toBe(false)
    expect(isAllowedInstallerUrl('https://example.com/OllamaSetup.exe')).toBe(false)
  })

  it('downloads the official installer as a stream and reports ready after installation', async () => {
    let installed = false
    const emitted: string[] = []
    const launchInstaller = vi.fn(async (path: string) => {
      expect((await stat(path)).isFile()).toBe(true)
      installed = true
      return 0
    })
    const fetch = vi.fn(async (input: string | URL) => {
      if (String(input) === OLLAMA_INSTALLER_URL) {
        return new Response(new Uint8Array([77, 90, 1, 2]), {
          status: 200,
          headers: { 'content-length': '4' }
        })
      }
      expect(String(input)).toBe(OLLAMA_API_URL)
      if (!installed) return new Response('', { status: 503 })
      return new Response(JSON.stringify({ version: '0.34.0' }), { status: 200 })
    })
    const service = new OllamaRuntimeService({
      fetch: fetch as never,
      emit: (status) => emitted.push(status.state),
      log: () => {},
      exists: async () => installed,
      launchInstaller,
      wait: async () => {}
    })

    const status = await service.install()

    expect(status.state).toBe('ready')
    expect(status.version).toBe('0.34.0')
    expect(launchInstaller).toHaveBeenCalledTimes(1)
    expect(fetch).toHaveBeenCalledWith(OLLAMA_INSTALLER_URL, expect.objectContaining({ redirect: 'follow' }))
    expect(emitted).toContain('installing')
  })

  it('does not launch an empty download and exposes the failure', async () => {
    const launchInstaller = vi.fn(async () => 0)
    const service = new OllamaRuntimeService({
      fetch: async () => new Response(new Uint8Array(), { status: 200 }),
      emit: () => {},
      log: () => {},
      launchInstaller,
      wait: async () => {}
    })

    const status = await service.install()

    expect(status.state).toBe('failed')
    expect(status.message).toContain('пустой установщик')
    expect(launchInstaller).not.toHaveBeenCalled()
  })

  it('does not launch a non-executable response from the official URL', async () => {
    const launchInstaller = vi.fn(async () => 0)
    const service = new OllamaRuntimeService({
      fetch: async (input) => String(input) === OLLAMA_INSTALLER_URL
        ? new Response('<html>not an installer</html>', { status: 200 })
        : new Response('', { status: 503 }),
      emit: () => {},
      log: () => {},
      launchInstaller,
      wait: async () => {}
    })

    const status = await service.install()

    expect(status.state).toBe('failed')
    expect(status.message).toContain('не Windows-инсталлятор')
    expect(launchInstaller).not.toHaveBeenCalled()
  })
})
