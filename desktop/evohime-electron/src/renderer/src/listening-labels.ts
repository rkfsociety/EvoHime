import type { ListeningReason, ListeningState } from '@shared/api'

export const STATE_TITLES: Record<ListeningState, string> = {
  stopped: 'Слушание выключено',
  starting: 'Слушание запускается',
  listening: 'Ева слушает',
  paused_by_user: 'Микрофон на паузе',
  paused_by_policy: 'Микрофон закрыт политикой',
  device_conflict: 'Микрофон занят',
  device_disconnected: 'Микрофон отключён',
  engine_unavailable: 'Слушание: проверка состояния…',
  denied: 'Слушание запрещено'
}

export const REASON_TEXTS: Record<ListeningReason, string> = {
  user_request: 'по решению пользователя',
  quiet_hours: 'идут тихие часы',
  blocklist: 'активное окно в чёрном списке',
  stop_word: 'прозвучало стоп-слово',
  permission_denied: 'доступ к микрофону не разрешён',
  device_conflict: 'устройство занято другим приложением',
  device_disconnected: 'выбранное устройство пропало',
  engine_unavailable: 'нет связи с процессом слушателя или движком распознавания',
  engine_degraded: 'распознавание не укладывается в бюджет даже на лёгкой модели',
  system_sleep: 'система уходила в сон',
  storage_failed: 'сбой локального хранилища',
  unknown: 'причина не сообщена'
}
