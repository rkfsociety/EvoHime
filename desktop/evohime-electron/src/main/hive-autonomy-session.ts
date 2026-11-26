import type { ShellLog } from './diagnostics/logger'
import type { CorePipeClient } from './ipc/pipe-client'

/**
 * Сессионная память о выбранной Королевой автономности Улья.
 *
 * Пока Ева работает, шелл запоминает последнее значение переключателя
 * «Автономность Королевы» и после каждого перезапуска Core (например, после
 * сохранения API-ключа) возвращает его: демон при своём старте всегда сбрасывает
 * автономность, чтобы улей не продолжил работу без ведома, а шелл дополняет это
 * восстановлением выбора, сделанного пользователем в этой сессии.
 *
 * Память живёт только в этом процессе: при повторном запуске Евы она пуста,
 * автономность остаётся выключенной до явного включения пользователем.
 */

const HIVE_AUTONOMY_KEY = 'proactive_evolution_enabled'

/** Задержка перед восстановлением: демон успевает подключиться к Core и выполнить свой сброс. */
const RESTORE_DELAY_MS = 3_000

let desiredAutonomy: 'true' | 'false' | null = null
let restoreTimer: ReturnType<typeof setTimeout> | null = null

/** Запоминает последнее значение переключателя автономности, выставленное в этой сессии Евы. */
export function rememberHiveAutonomyChoice(value: string): void {
  if (value !== 'true' && value !== 'false') return
  desiredAutonomy = value
  if (value === 'false' && restoreTimer !== null) {
    clearTimeout(restoreTimer)
    restoreTimer = null
  }
}

/**
 * После подключения к Core откладывает восстановление автономности, если
 * пользователь включал её в этой сессии. Идемпотентно: повторные подключения
 * лишь переносят таймер, само значение Core получает заново.
 */
export function scheduleHiveAutonomyRestore(client: CorePipeClient, log: ShellLog): void {
  if (desiredAutonomy !== 'true') return
  if (restoreTimer !== null) clearTimeout(restoreTimer)
  restoreTimer = setTimeout(() => {
    restoreTimer = null
    if (desiredAutonomy !== 'true') return
    const queued = client.send({
      setHiveSetting: { schemaVersion: 1, requestId: '', key: HIVE_AUTONOMY_KEY, value: 'true' }
    })
    log('info', 'shell.hive_autonomy_restore_sent', { queued })
  }, RESTORE_DELAY_MS)
  restoreTimer.unref?.()
}

/** Сбрасывает сессионную память: для повторных запусков процесса в тестах. */
export function resetHiveAutonomySessionForTests(): void {
  desiredAutonomy = null
  if (restoreTimer !== null) {
    clearTimeout(restoreTimer)
    restoreTimer = null
  }
}
