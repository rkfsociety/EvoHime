import { app } from 'electron'

import { JsonlLogger } from './diagnostics/logger'
import { logDirectory } from './paths'
import { hardenProcess, type HardeningOptions } from './security'
import { runUpdaterApplication } from './updater-window'

const logger = new JsonlLogger({ directory: logDirectory(), stream: 'updater' })
const log: HardeningOptions['log'] = (level, event, fields) => logger.write(level, event, fields)
const hardening: HardeningOptions = { rendererOrigin: 'file://', log }

hardenProcess(hardening)
void runUpdaterApplication({ ...hardening, flushLogs: () => logger.flush() }).catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error))
  void logger.flush().then((flushed) => {
    if (!flushed) process.stderr.write('EvoHime updater diagnostics flush timed out; exiting with a bounded log tail.\n')
    app.exit(1)
  })
})
