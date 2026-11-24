import { translate } from './i18n'
import { useState } from 'react'

import type { UpdateStatus } from '@shared/update'
import { updateProgress } from '@shared/update'
import { isDownloadPhaseMessage, isInstallPhaseMessage } from '@shared/updater'

import { useShellApi } from './shell-api'
import './UpdateSurface.css'

const VISIBLE_PHASES = ['checking', 'available', 'preparing', 'ready', 'applying', 'failed'] as const

type IndicatorStepState = 'pending' | 'active' | 'done' | 'failed'

interface IndicatorStep {
  readonly id: string
  readonly label: string
  readonly state: IndicatorStepState
}

function resolveIndicatorSteps(status: UpdateStatus, percent: number | null): readonly IndicatorStep[] {
  const hasRealSteps = status.steps.some((step) => step.state !== 'skipped')
  if (hasRealSteps) {
    return status.steps
      .filter((step): step is typeof step & { readonly state: IndicatorStepState } => step.state !== 'skipped')
      .map((step) => ({ id: step.id, label: step.label, state: step.state }))
  }

  let check: IndicatorStepState = 'pending'
  let download: IndicatorStepState = 'pending'
  let install: IndicatorStepState = 'pending'

  if (status.phase === 'checking') {
    check = 'active'
  } else if (status.phase === 'available') {
    check = 'done'
  } else if (status.phase === 'preparing' || status.phase === 'applying') {
    check = 'done'
    const installing = isInstallPhaseMessage(status.message) && !isDownloadPhaseMessage(status.message)
    if (installing) {
      download = 'done'
      install = 'active'
    } else {
      download = 'active'
    }
  } else if (status.phase === 'ready') {
    check = 'done'
    download = 'done'
    install = status.restartRequired ? 'pending' : 'done'
  } else if (status.phase === 'failed') {
    const hasModules = (status.availableModules?.length ?? 0) > 0
    if (!hasModules && percent === null && !isDownloadPhaseMessage(status.message) && !isInstallPhaseMessage(status.message)) {
      check = 'failed'
    } else if (isInstallPhaseMessage(status.message) && !isDownloadPhaseMessage(status.message)) {
      check = 'done'
      download = 'done'
      install = 'failed'
    } else {
      check = 'done'
      download = 'failed'
    }
  }

  return [
    { id: 'check', label: 'Проверка', state: check },
    { id: 'download', label: 'Загрузка', state: download },
    { id: 'install', label: 'Установка', state: install }
  ]
}

function resolveActiveModule(message: string, modules: readonly string[] | undefined): string | null {
  if (!modules || modules.length === 0) return null
  const lower = message.toLowerCase()
  return modules.find((module) => lower.includes(module.toLowerCase())) ?? null
}

interface UpdateIndicatorProps {
  readonly status: UpdateStatus | null
}

/** Compact, always-available entry point for the non-blocking installer update. */
export function UpdateIndicator({ status }: UpdateIndicatorProps): React.JSX.Element | null {
  const api = useShellApi()
  const [confirmOpen, setConfirmOpen] = useState(false)

  const visible = status !== null && (VISIBLE_PHASES as readonly string[]).includes(status.phase)

  if (!visible || !status) return null

  const running = status.phase === 'checking' || status.phase === 'preparing' || status.phase === 'applying'
  const failed = status.phase === 'failed'
  const progress = status.downloadProgress ?? updateProgress(status)
  const percent = progress === null ? null : Math.round(progress * 100)
  const ready = status.phase === 'ready'
  const available = status.phase === 'available' && (status.availableModules?.length ?? 0) > 0
  const steps = resolveIndicatorSteps(status, percent)
  const activeModule = running ? resolveActiveModule(status.message, status.availableModules) : null
  const showProgressBar = running || ready || failed

  return (
    <div className="update-indicator">
      <button
        type="button"
        className={`update-indicator__button${failed ? ' update-indicator__button--failed' : ''}${ready ? ' update-indicator__button--ready' : ''}`}
        aria-label={ready ? translate("Подтвердить установку обновления") : available ? translate("Открыть обновление") : translate("Прогресс скачивания обновления")}
        aria-expanded={ready || available ? confirmOpen : undefined}
        title={status.message}
        onClick={() => {
          setConfirmOpen(true)
        }}
      >
        <span
          className={`update-indicator__circle${running && percent === null ? ' update-indicator__circle--indeterminate' : ''}`}
          style={percent === null ? undefined : { '--update-percent': `${percent}%` } as React.CSSProperties}
          aria-hidden="true"
        >
          <span>{translate(failed ? '!' : percent === null ? '…' : `${percent}%`)}</span>
        </span>
      </button>

      {confirmOpen ? (
        <section className="update-confirm" role="dialog" aria-label={translate("Подтверждение обновления")}>
          <div className="update-confirm__header">
            <div>
              <h2>{translate(ready ? translate("Обновление готово") : translate("Доступно обновление"))}</h2>
              <p>{translate(ready ? translate("Изменения проверены. Применить обновление сейчас?") : status.message)}</p>
            </div>
          </div>
          <div className="update-confirm__status">
            <ol className="update-confirm__steps" aria-label={translate("Этапы обновления")}>
              {steps.map((step) => (
                <li key={step.id} className={`update-confirm__step update-confirm__step--${step.state}`}>
                  <span className="update-confirm__step-dot" aria-hidden="true" />
                  <span>{translate(step.label)}</span>
                </li>
              ))}
            </ol>
            {showProgressBar ? (
              <div className="update-confirm__progress-row">
                <div
                  className="update-confirm__progress"
                  role="progressbar"
                  aria-label={translate("Прогресс обновления")}
                  aria-valuemin={0}
                  aria-valuemax={100}
                  aria-valuenow={percent ?? undefined}
                >
                  <div
                    className={`update-confirm__progress-fill${running && percent === null ? ' update-confirm__progress-fill--indeterminate' : ''}${failed ? ' update-confirm__progress-fill--failed' : ''}`}
                    style={percent === null ? undefined : { width: `${percent}%` }}
                  />
                </div>
                <span className="update-confirm__progress-label">
                  {failed ? translate('Ошибка') : percent === null ? '…' : `${percent}%`}
                </span>
              </div>
            ) : null}
          </div>
          {status.availableModules && status.availableModules.length > 0 ? (
            <ul className="update-confirm__modules">
              {status.availableModules.map((module) => {
                const isActive = module === activeModule
                return (
                  <li key={module} className={isActive ? 'update-confirm__module--active' : undefined}>
                    <span>
                      <strong>{translate(module)}</strong>
                      {status.availableModuleSummaries?.[module] ? <small>{translate(status.availableModuleSummaries[module])}</small> : null}
                      {status.availableModuleChanges?.[module]?.length ? (
                        <small>{translate(status.availableModuleChanges[module]!.join(' '))}</small>
                      ) : null}
                      {isActive ? (
                        <span className="update-confirm__module-bar" aria-hidden="true">
                          <span
                            className={`update-confirm__module-bar-fill${percent === null ? ' update-confirm__progress-fill--indeterminate' : ''}`}
                            style={percent === null ? undefined : { width: `${percent}%` }}
                          />
                        </span>
                      ) : null}
                    </span>
                    <strong className="update-confirm__module-meta">
                      <span>{translate(status.installedModules?.[module] ?? '—')} → {translate(status.availableModuleVersions?.[module] ?? translate("новая версия"))}</span>
                      {isActive && percent !== null ? <small className="update-confirm__module-percent">{`${percent}%`}</small> : null}
                    </strong>
                  </li>
                )
              })}
            </ul>
          ) : null}
          <div className="update-confirm__actions">
            <button type="button" onClick={() => setConfirmOpen(false)}>{translate("Закрыть")}</button>
            {ready ? <button type="button" onClick={() => void api?.invoke('update.restart', {})}>{translate("Обновить")}</button> : null}
            {available ? (
              <button
                type="button"
                onClick={() => {
                  setConfirmOpen(false)
                  void api?.invoke('update.prepareComponents', { selected: status.availableModules ?? [] })
                }}
              >
                {translate("Обновить сейчас")}</button>
            ) : null}
          </div>
        </section>
      ) : null}
    </div>
  )
}
