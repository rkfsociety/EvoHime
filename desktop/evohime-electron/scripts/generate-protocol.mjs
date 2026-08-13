#!/usr/bin/env node
/**
 * Generates the TypeScript/JavaScript `desktop-ipc-v1` bindings from the
 * canonical protobuf definition owned by the Rust workspace.
 *
 * The canonical source is `crates/desktop-ipc/proto/evohime.desktop.proto`.
 * Hand-editing the generated files is forbidden; CI runs this script with
 * `--check` so a stale binding fails the build (plan 0, gate 1).
 */
import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const projectRoot = resolve(here, '..')
const repoRoot = resolve(projectRoot, '..', '..')

const protoPath = join(repoRoot, 'crates', 'desktop-ipc', 'proto', 'evohime.desktop.proto')
const outputDir = join(projectRoot, 'src', 'main', 'ipc', 'generated')
const jsPath = join(outputDir, 'protocol.js')
const dtsPath = join(outputDir, 'protocol.d.ts')

const header = [
  '// GENERATED FILE - DO NOT EDIT.',
  '// Source: crates/desktop-ipc/proto/evohime.desktop.proto',
  '// Regenerate with: npm run generate:protocol',
  ''
].join('\n')

const check = process.argv.includes('--check')

// The CLI entry points are invoked through node directly: Windows refuses to
// spawn the `.cmd` shims from execFile without a shell.
function cli(name) {
  return join(projectRoot, 'node_modules', 'protobufjs-cli', 'bin', name)
}

function generate(targetJs, targetDts) {
  mkdirSync(dirname(targetJs), { recursive: true })
  execFileSync(
    process.execPath,
    [
      cli('pbjs'),
      '--target',
      'static-module',
      '--wrap',
      'es6',
      '--es6',
      '--no-create',
      '--no-verify',
      '--no-convert',
      '--no-delimited',
      // 64-bit fields (sequence ids, epochs, timestamps) decode as plain
      // numbers so the adapter has one deterministic representation. Values
      // stay far below 2^53 by protocol contract.
      '--force-number',
      '--out',
      targetJs,
      protoPath
    ],
    { stdio: 'inherit', cwd: projectRoot }
  )
  writeFileSync(targetJs, header + readFileSync(targetJs, 'utf8'), 'utf8')

  execFileSync(process.execPath, [cli('pbts'), '--out', targetDts, targetJs], {
    stdio: 'inherit',
    cwd: projectRoot
  })
  writeFileSync(targetDts, header + readFileSync(targetDts, 'utf8'), 'utf8')
}

if (!check) {
  generate(jsPath, dtsPath)
  console.log(`generated ${jsPath}`)
  process.exit(0)
}

const tempDir = join(projectRoot, 'node_modules', '.protocol-check')
const tempJs = join(tempDir, 'protocol.js')
const tempDts = join(tempDir, 'protocol.d.ts')
try {
  generate(tempJs, tempDts)
  const stale = [
    [jsPath, tempJs],
    [dtsPath, tempDts]
  ].filter(([committed, fresh]) => readOrEmpty(committed) !== readFileSync(fresh, 'utf8'))
  if (stale.length > 0) {
    for (const [committed] of stale) {
      console.error(`stale generated binding: ${committed}`)
    }
    console.error('run "npm run generate:protocol" and commit the result')
    process.exit(1)
  }
  console.log('generated protobuf bindings match the canonical proto')
} finally {
  rmSync(tempDir, { recursive: true, force: true })
}

function readOrEmpty(path) {
  try {
    return readFileSync(path, 'utf8')
  } catch {
    return ''
  }
}
