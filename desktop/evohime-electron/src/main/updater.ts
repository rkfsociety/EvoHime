import { app } from 'electron'

import { JsonlLogger } from './diagnostics/logger'
import { logDirectory } from './paths'
import { hardenProcess, type HardeningOptions } from './security'
import { runUpdaterApplication } from './updater-window'

const logger = new JsonlLogger({ directory: logDirectory(), stream: 'main' })
const log: HardeningOptions['log'] = (level, event, fields) => logger.write(level, event, fields)
const hardening: HardeningOptions = { rendererOrigin: 'file://', log }

hardenProcess(hardening)
void runUpdaterApplication(hardening).catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error))
  app.exit(1)
})
