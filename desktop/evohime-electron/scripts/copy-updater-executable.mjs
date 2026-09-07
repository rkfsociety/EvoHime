import { copyFileSync, existsSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const packageRoot = join(root, 'release', 'win-unpacked')
const shell = join(packageRoot, 'EvoHime.exe')
const updater = join(packageRoot, 'EvoHimeUpdater.exe')

if (!existsSync(shell)) {
  throw new Error(`Electron shell is missing: ${shell}`)
}
copyFileSync(shell, updater)
console.log(`Electron updater executable: ${updater}`)
