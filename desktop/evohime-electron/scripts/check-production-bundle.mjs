#!/usr/bin/env node
/**
 * Static security checks over the built production bundles (plan 0, gate 2).
 *
 * These run against `out/` after `npm run build`, so they catch a hardening
 * regression that unit tests cannot see: a stripped guard, a leaked source
 * map, a relaxed CSP, or a Node primitive pulled into the sandboxed preload.
 */
import { readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const outDir = join(projectRoot, 'out')

const failures = []

function fail(message) {
  failures.push(message)
}

function walk(directory) {
  const entries = []
  for (const name of readdirSync(directory)) {
    const full = join(directory, name)
    if (statSync(full).isDirectory()) {
      entries.push(...walk(full))
    } else {
      entries.push(full)
    }
  }
  return entries
}

let files
try {
  files = walk(outDir)
} catch {
  console.error('out/ is missing — run "npm run build" first')
  process.exit(1)
}

const relative = (path) => path.slice(projectRoot.length + 1).replaceAll('\\', '/')

// 1. A production package must not ship source maps.
for (const file of files) {
  if (file.endsWith('.map')) {
    fail(`source map in production bundle: ${relative(file)}`)
  }
}

const mainBundle = files.find((file) => relative(file) === 'out/main/index.js')
const preloadBundle = files.find((file) => relative(file) === 'out/preload/index.js')
const rendererHtml = files.find((file) => relative(file) === 'out/renderer/index.html')
const rendererScripts = files.filter(
  (file) => relative(file).startsWith('out/renderer/assets/') && file.endsWith('.js')
)

for (const [label, file] of [
  ['main', mainBundle],
  ['preload', preloadBundle],
  ['renderer html', rendererHtml]
]) {
  if (!file) {
    fail(`${label} bundle is missing from out/`)
  }
}
if (rendererScripts.length === 0) {
  fail('renderer bundle has no script assets')
}

// 2. The hardening the shell depends on must survive bundling and minification.
if (mainBundle) {
  const main = readFileSync(mainBundle, 'utf8')
  for (const marker of [
    'enableSandbox',
    'setPermissionRequestHandler',
    'setPermissionCheckHandler',
    'setWindowOpenHandler',
    'will-navigate',
    'Content-Security-Policy',
    'requestSingleInstanceLock'
  ]) {
    if (!main.includes(marker)) {
      fail(`main bundle lost the "${marker}" guard`)
    }
  }
  if (main.includes('openDevTools')) {
    fail('main bundle opens DevTools')
  }
  if (/sandbox\s*:\s*(!1|false)/.test(main)) {
    fail('main bundle disables the renderer sandbox')
  }
  if (/contextIsolation\s*:\s*(!1|false)/.test(main)) {
    fail('main bundle disables context isolation')
  }
  if (/nodeIntegration\s*:\s*(!0|true)/.test(main)) {
    fail('main bundle enables node integration')
  }
  if (/webviewTag\s*:\s*(!0|true)/.test(main)) {
    fail('main bundle enables the webview tag')
  }
}

// 3. The sandboxed preload may reach Electron and nothing else.
if (preloadBundle) {
  const preload = readFileSync(preloadBundle, 'utf8')
  for (const forbidden of [
    'node:fs',
    'node:child_process',
    'node:os',
    'node:net',
    'child_process',
    '@electron/remote',
    'webFrame'
  ]) {
    if (preload.includes(forbidden)) {
      fail(`preload bundle references "${forbidden}"`)
    }
  }
  const requires = [...preload.matchAll(/require\(["']([^"']+)["']\)/g)].map((match) => match[1])
  for (const module of requires) {
    if (module !== 'electron') {
      fail(`preload bundle requires "${module}" beyond electron`)
    }
  }
}

// 4. The packaged renderer is loaded over file://, where the enforced policy is
//    the meta CSP, so it must be present and strict.
if (rendererHtml) {
  const html = readFileSync(rendererHtml, 'utf8')
  const csp = /http-equiv="Content-Security-Policy"\s+content="([^"]+)"/.exec(html)?.[1]
  if (!csp) {
    fail('renderer html has no Content-Security-Policy meta tag')
  } else {
    if (!csp.includes("default-src 'self'")) {
      fail("renderer CSP is missing default-src 'self'")
    }
    if (csp.includes('unsafe-eval')) {
      fail('renderer CSP allows unsafe-eval')
    }
    if (/script-src[^;]*unsafe-inline/.test(csp)) {
      fail('renderer CSP allows inline script')
    }
    if (/https?:\/\//.test(csp)) {
      fail('renderer CSP allows remote content')
    }
  }
  if (/<script[^>]+src="https?:/.test(html)) {
    fail('renderer html loads a remote script')
  }
}

// 5. The renderer is untrusted and must not carry shell capabilities.
for (const script of rendererScripts) {
  const content = readFileSync(script, 'utf8')
  for (const forbidden of ['require(', 'openDevTools', 'child_process', 'ipcRenderer']) {
    if (content.includes(forbidden)) {
      fail(`renderer asset ${relative(script)} references "${forbidden}"`)
    }
  }
}

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`production bundle check failed: ${failure}`)
  }
  process.exit(1)
}

console.log('production bundle checks passed')
