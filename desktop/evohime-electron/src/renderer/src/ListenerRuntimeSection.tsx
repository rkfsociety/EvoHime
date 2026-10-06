import { useCallback, useEffect, useState } from 'react'

import {
  initialListenerRuntimeStatus,
  optionalFileLabel,
  type ListenerRuntimeStatus
} from '@shared/listener-runtime'

import { useShellApi } from './shell-api'
import { useT, translate} from './i18n'

/**
 * Экран рантайма распознавания речи.
 *
 * Загрузку выполняет main-процесс; здесь только показывается его состояние и
 * запрашивается действие. Ничего не устанавливается само: набор весит сотни
 * мегабайт, и решение принимает пользователь.
 *
 * Отдельно называется то, чего не хватает: без ONNX-рантайма остаётся
 * энергетический VAD, и это влияет на качество — умалчивать об этом значило бы
 * выдать неполную установку за полную.
 */

const STATE_LABELS: Record<ListenerRuntimeStatus['state'], string> = {
  unknown: 'не проверялось',
  missing: 'не установлено',
  ready: 'готово',
  'update-available': 'доступно обновление',
  downloading: 'загрузка',
  failed: 'ошибка'
}

export function ListenerRuntimeSection(): React.JSX.Element {
  const t = useT()

  const api = useShellApi()
  const [status, setStatus] = useState<ListenerRuntimeStatus>(() => initialListenerRuntimeStatus(''))
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    if (!api) return
    void api.invoke('listener.getRuntimeStatus', {}).then((outcome) => {
      if (outcome.ok) setStatus(outcome.value)
    })
    // Ход загрузки приходит событиями: команда возвращается только в конце, а
    // прогресс нужен всё это время.
    return api.subscribe((event) => {
      if (event.kind === 'listener-runtime') setStatus(event.status)
    })
  }, [api])

  const run = useCallback(
    async (command: 'listener.checkRuntime' | 'listener.downloadRuntime') => {
      if (!api || busy) return
      setBusy(true)
      const outcome = await api.invoke(command, {})
      if (outcome.ok) setStatus(outcome.value)
      setBusy(false)
    },
    [api, busy]
  )

  const downloading = status.state === 'downloading'
  const canInstall = status.state === 'missing' || status.state === 'update-available' || status.state === 'failed'

  return (
    <section className="settings-info" aria-label={t("Распознавание речи")}>
      <h3>{t("Распознавание речи")}</h3>
      <p>
        {t("Локальный движок whisper.cpp и модели загружаются отдельно от приложения и проверяются по SHA-256 из манифеста релиза. Пока набор не установлен, постоянное слушание остаётся выключенным.")}
      </p>

      <dl className="settings-info__details">
        <dt>{t("Состояние")}</dt>
        <dd>{t(STATE_LABELS[status.state])}</dd>
        <dt>{t("Установлено")}</dt>
        <dd>{status.installedVersion ?? translate("нет")}</dd>
        <dt>{t("Доступно")}</dt>
        <dd>{status.availableVersion ?? translate("неизвестно")}</dd>
        <dt>{t("Каталог")}</dt>
        <dd>{status.toolsDirectory || translate("не определён")}</dd>
      </dl>

      <p role="status">{status.message}</p>

      {downloading ? (
        <progress
          className="listener-runtime__progress"
          value={status.progressPct}
          max={100}
          aria-label={t("Ход загрузки распознавания речи")}
        />
      ) : null}

      {status.missingOptional.length > 0 ? (
        <p className="listener-runtime__warning">
          {t("Не установлено:")} {status.missingOptional.map(optionalFileLabel).join(', ')}{t(". Распознавание работает, но обнаружение речи остаётся энергетическим.")}
        </p>
      ) : null}

      <div className="listener-runtime__actions">
        <button type="button" disabled={!api || busy || downloading} onClick={() => void run('listener.checkRuntime')}>
          {t("Проверить")}
        </button>
        <button
          type="button"
          disabled={!api || busy || downloading || !canInstall}
          onClick={() => void run('listener.downloadRuntime')}
        >
          {status.state === 'update-available' ? translate("Обновить") : translate("Установить")}
        </button>
      </div>
    </section>
  )
}
