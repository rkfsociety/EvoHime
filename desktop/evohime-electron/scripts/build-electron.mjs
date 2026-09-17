import { spawnSync } from 'node:child_process'
import { join } from 'node:path'

const target = process.argv[2]
if (target !== 'shell' && target !== 'updater') {
  throw new Error('Usage: node scripts/build-electron.mjs <shell|updater>')
}

const command = process.execPath
const electronVite = join('node_modules', 'electron-vite', 'bin', 'electron-vite.js')
const result = spawnSync(command, [electronVite, 'build'], {
  env: { ...process.env, EVOHIME_ELECTRON_TARGET: target },
  stdio: 'inherit'
})
if (result.error) throw result.error
process.exit(result.status ?? 1)
