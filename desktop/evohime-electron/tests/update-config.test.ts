import { describe, expect, it } from 'vitest'

import {
  DEFAULT_BRANCH,
  DEFAULT_REPOSITORY_URL,
  loadUpdateConfig,
  normalizeBranch,
  normalizeCommit,
  normalizeRepositoryUrl,
  readBuildMarker
} from '../src/main/update/config'

const COMMIT = 'a'.repeat(40)

function load(file: unknown, environment: NodeJS.ProcessEnv = {}) {
  return loadUpdateConfig({
    dataDirectory: 'C:\\data\\EvoHime',
    executablePath: 'C:\\Programs\\EvoHime\\EvoHime.exe',
    environment,
    readFile: () => (file === undefined ? null : JSON.stringify(file))
  })
}

describe('update config', () => {
  it('falls back to the compiled defaults without a config file', () => {
    const config = load(undefined)

    expect(config.enabled).toBe(true)
    expect(config.repositoryUrl).toBe(DEFAULT_REPOSITORY_URL)
    expect(config.branch).toBe(DEFAULT_BRANCH)
    expect(config.launchPolicy).toBe('build')
    expect(config.sourceDirectory).toBe('C:\\data\\EvoHime\\source')
    expect(config.stagingDirectory).toBe('C:\\data\\EvoHime\\update-staging')
    expect(config.installDirectory).toBe('C:\\Programs\\EvoHime')
  })

  it('reads the values written by the installer', () => {
    const config = load({
      enabled: true,
      repositoryUrl: 'https://example.invalid/evo.git',
      branch: 'release/next',
      launchPolicy: 'apply-ready',
      checkIntervalMinutes: 120
    })

    expect(config.repositoryUrl).toBe('https://example.invalid/evo.git')
    expect(config.branch).toBe('release/next')
    expect(config.launchPolicy).toBe('apply-ready')
    expect(config.checkIntervalMs).toBe(120 * 60_000)
  })

  it('never launches an update run when it is disabled', () => {
    const config = load({ enabled: false, launchPolicy: 'build' })

    expect(config.enabled).toBe(false)
    expect(config.launchPolicy).toBe('off')
  })

  it('keeps a hostile config file from redirecting or bounding out the rebuild', () => {
    // Only https remotes are accepted, so a writable config cannot point the
    // rebuild at a local path or make git run a helper command.
    expect(normalizeRepositoryUrl('ssh://git@example.invalid/evo.git')).toBeNull()
    expect(normalizeRepositoryUrl('file:///C:/evil')).toBeNull()
    expect(normalizeRepositoryUrl('ext::sh -c evil')).toBeNull()
    expect(normalizeRepositoryUrl(42)).toBeNull()
    expect(load({ repositoryUrl: 'file:///C:/evil' }).repositoryUrl).toBe(DEFAULT_REPOSITORY_URL)

    expect(normalizeBranch('--upload-pack=evil')).toBeNull()
    expect(normalizeBranch('a/../b')).toBeNull()
    expect(normalizeBranch('main')).toBe('main')

    // The interval is clamped, so a config file cannot turn the check into a
    // busy loop.
    expect(load({ checkIntervalMinutes: 0 }).checkIntervalMs).toBe(5 * 60_000)
    expect(load({ checkIntervalMinutes: 10_000 }).checkIntervalMs).toBe(24 * 60 * 60_000)
  })

  it('lets the environment override the source and install locations', () => {
    const config = load(undefined, {
      EVOHIME_UPDATE_ENABLED: '0',
      EVOHIME_UPDATE_BRANCH: 'work',
      EVOHIME_UPDATE_SOURCE_DIR: 'D:\\src\\EvoHime',
      EVOHIME_UPDATE_INSTALL_DIR: 'relative-is-ignored'
    })

    expect(config.enabled).toBe(false)
    expect(config.branch).toBe('work')
    expect(config.sourceDirectory).toBe('D:\\src\\EvoHime')
    expect(config.installDirectory).toBe('C:\\Programs\\EvoHime')
  })
})

describe('build marker', () => {
  it('reads the commit a package was built from', () => {
    const marker = readBuildMarker('C:\\Programs\\EvoHime', () =>
      JSON.stringify({ commit: COMMIT, branch: 'main', builtAtMs: 7 })
    )

    expect(marker).toEqual({ commit: COMMIT, branch: 'main', builtAtMs: 7 })
  })

  it('treats a missing or malformed marker as an unknown version', () => {
    expect(readBuildMarker('C:\\Programs\\EvoHime', () => null)).toBeNull()
    expect(readBuildMarker('C:\\Programs\\EvoHime', () => 'not json')).toBeNull()
    expect(readBuildMarker('C:\\Programs\\EvoHime', () => '{"commit":"HEAD"}')).toBeNull()
    expect(normalizeCommit(COMMIT.toUpperCase())).toBeNull()
    expect(normalizeCommit(COMMIT)).toBe(COMMIT)
  })
})
