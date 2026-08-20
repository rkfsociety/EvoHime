/**
 * Состояние набора рантайма распознавания речи, каким его видит renderer.
 *
 * Тип общий для main и renderer: главный экран показывает ровно то, что
 * посчитал main-процесс, и не выводит «готово» самостоятельно.
 */

export type ListenerRuntimeState =
  | 'unknown'
  | 'missing'
  | 'ready'
  | 'update-available'
  | 'downloading'
  | 'failed'

export interface ListenerRuntimeStatus {
  readonly state: ListenerRuntimeState
  /** Версия установленного набора либо `null`, если его нет. */
  readonly installedVersion: string | null
  /** Версия, опубликованная в релизе, если проверка удалась. */
  readonly availableVersion: string | null
  readonly progressPct: number
  readonly message: string
  /**
   * Опциональные файлы, объявленные манифестом и отсутствующие на диске.
   * Показываются прямо: без ONNX-рантайма остаётся энергетический VAD, и это
   * пользователю надо знать, а не догадываться по качеству.
   */
  readonly missingOptional: readonly string[]
  /** Каталог, в котором листенер ищет рантайм. */
  readonly toolsDirectory: string
}

export function initialListenerRuntimeStatus(toolsDirectory: string): ListenerRuntimeStatus {
  return {
    state: 'unknown',
    installedVersion: null,
    availableVersion: null,
    progressPct: 0,
    message: 'Состояние рантайма распознавания ещё не проверялось.',
    missingOptional: [],
    toolsDirectory
  }
}

/** Понятное имя опционального файла для интерфейса. */
export function optionalFileLabel(role: string): string {
  switch (role) {
    case 'onnxruntime_dll':
      return 'ONNX Runtime'
    case 'silero_vad':
      return 'модель Silero VAD'
    default:
      return role
  }
}
