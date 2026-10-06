import { useEffect, useState } from 'react'

import { ListenerRuntimeSection } from './ListenerRuntimeSection'
import { ProviderForm } from './ProviderForm'
import { CodexPanel } from './CodexPanel'
import { SafetyPanel } from './SafetyPanel'
import { SkillCatalogPanel } from './SkillCatalogPanel'
import { IntegrationProviderPanel } from './IntegrationProviderPanel'
import { EventTriggerRuntimePanel } from './EventTriggerRuntimePanel'
import { AdaptiveToolCatalogPanel } from './AdaptiveToolCatalogPanel'
import { DiagnosticsAndSupportBundlePanel } from './DiagnosticsAndSupportBundlePanel'
import { ExternalCodingAgentAdapterPanel } from './ExternalCodingAgentAdapterPanel'
import { MultiReviewerEnsemblePanel } from './MultiReviewerEnsemblePanel'
import { LanguageIntelligencePanel } from './LanguageIntelligencePanel'
import { PromptStrategyPanel } from './PromptStrategyPanel'

import type { ConnectionState, CoreEvent } from '@shared/api'
import { DEFAULT_APPEARANCE, type AppearanceSettings } from './appearance'
import { translate, useT } from './i18n'

export type SettingsTab = 'provider' | 'agents' | 'integrations' | 'triggers' | 'speech' | 'skills' | 'tools' | 'diagnostics' | 'appearance' | 'security'

interface SettingsModalProps {
  readonly workspace: string | null
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly appearance: AppearanceSettings
  readonly onAppearanceChange: (settings: AppearanceSettings) => void
  readonly initialTab?: SettingsTab
  readonly onClose: () => void
}

const TABS: readonly { readonly id: SettingsTab; readonly label: string }[] = [
  { id: 'provider', label: 'Провайдер и модели' },
  { id: 'agents', label: 'Внешние агенты' },
  { id: 'integrations', label: 'Интеграции' },
  { id: 'triggers', label: 'Триггеры событий' },
  { id: 'speech', label: 'Распознавание речи' },
  { id: 'skills', label: 'Agent Skills' },
  { id: 'tools', label: 'Каталог tools' },
  { id: 'diagnostics', label: 'Диагностика' },
  { id: 'appearance', label: 'Внешний вид' },
  { id: 'security', label: 'Безопасность' }
]

export function SettingsModal({ workspace, connection, events, appearance, onAppearanceChange, initialTab = 'provider', onClose }: SettingsModalProps): React.JSX.Element {
  const t = useT()
  const [tab, setTab] = useState<SettingsTab>(initialTab)
  const [providerSurface, setProviderSurface] = useState<'api' | 'codex'>('api')

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  return (
    <div className="settings-modal" role="presentation" onMouseDown={(event) => {
      if (event.target === event.currentTarget) onClose()
    }}>
      <section className="settings-modal__window" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header className="settings-modal__header">
          <div>
            <p className="settings-modal__eyebrow">EvoHime</p>
            <h2 id="settings-title">{t('Настройки')}</h2>
          </div>
          <button type="button" className="settings-modal__close" aria-label={t('Закрыть настройки')} onClick={onClose}>
            ×
          </button>
        </header>

        <div className="settings-modal__body">
          <nav className="settings-tabs" aria-label={t('Разделы настроек')}>
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                className="settings-tabs__item"
                aria-selected={tab === item.id}
                role="tab"
                onClick={() => setTab(item.id)}
              >
                {t(item.label)}
              </button>
            ))}
          </nav>

          <div className="settings-modal__content">
            {tab === 'provider' ? (
              <section className="provider-hub" aria-label={translate("Провайдер и модели")}>
                <div className="provider-hub__tabs" role="tablist" aria-label={translate("Источник моделей")}>
                  <button type="button" role="tab" aria-selected={providerSurface === 'api'} onClick={() => setProviderSurface('api')}>
                    {translate("API-провайдеры")}</button>
                  <button type="button" role="tab" aria-selected={providerSurface === 'codex'} onClick={() => setProviderSurface('codex')}>
                    Codex CLI
                  </button>
                </div>
                {translate(providerSurface === 'api' ? <ProviderForm connection={connection} events={events} /> : <CodexPanel />)}
              </section>
            ) : null}
            {tab === 'agents' ? <><ExternalCodingAgentAdapterPanel /><MultiReviewerEnsemblePanel connection={connection} /><LanguageIntelligencePanel connection={connection} /><PromptStrategyPanel events={events} /></> : null}
            {translate(tab === 'integrations' ? <IntegrationProviderPanel /> : null)}
            {translate(tab === 'triggers' ? <EventTriggerRuntimePanel workspace={workspace} /> : null)}
            {translate(tab === 'speech' ? <ListenerRuntimeSection /> : null)}
            {translate(tab === 'skills' ? <SkillCatalogPanel workspace={workspace} connection={connection} events={events} /> : null)}
            {translate(tab === 'tools' ? <AdaptiveToolCatalogPanel connection={connection} events={events} /> : null)}
            {translate(tab === 'diagnostics' ? <DiagnosticsAndSupportBundlePanel connection={connection} events={events} /> : null)}
            {translate(tab === 'appearance' ? <AppearanceSettingsPanel settings={appearance} onChange={onAppearanceChange} /> : null)}
            {translate(tab === 'security' ? <SafetyPanel connection={connection} events={events} /> : null)}
          </div>
        </div>
      </section>
    </div>
  )
}

function AppearanceSettingsPanel({ settings, onChange }: { readonly settings: AppearanceSettings; readonly onChange: (settings: AppearanceSettings) => void }): React.JSX.Element {
  const t = useT()
  return (
    <section className="settings-info appearance-settings" aria-label={t('Внешний вид')}>
      <div className="appearance-settings__heading">
        <div><h3>{t('Внешний вид')}</h3><p>{t('Настройте оформление и читаемость приложения. Изменения применяются сразу и сохраняются на этом компьютере.')}</p></div>
        <button type="button" onClick={() => onChange({ ...DEFAULT_APPEARANCE, locale: settings.locale })}>{t('Сбросить')}</button>
      </div>
      <div className="appearance-settings__grid">
        <label>{t('Тема')} <select value={settings.theme} onChange={event => onChange({ ...settings, theme: event.target.value as AppearanceSettings['theme'] })}><option value="system">{t('Как в Windows')}</option><option value="dark">{t('Тёмная')}</option><option value="light">{t('Светлая')}</option></select></label>
        <label>{t('Размер интерфейса')} <select value={settings.scale} onChange={event => onChange({ ...settings, scale: event.target.value as AppearanceSettings['scale'] })}><option value="90">{t('Меньше')}</option><option value="100">{t('Стандартный')}</option><option value="110">{t('Крупнее')}</option></select></label>
        <label>{t('Плотность')} <select value={settings.density} onChange={event => onChange({ ...settings, density: event.target.value as AppearanceSettings['density'] })}><option value="comfortable">{t('Комфортная')}</option><option value="compact">{t('Компактная')}</option></select></label>
        <label>{t('Цвет акцента')} <select value={settings.accent} onChange={event => onChange({ ...settings, accent: event.target.value as AppearanceSettings['accent'] })}><option value="violet">{t('Фиолетовый')}</option><option value="blue">{t('Синий')}</option><option value="teal">{t('Бирюзовый')}</option><option value="rose">{t('Розовый')}</option></select></label>
        <label>{t('Язык')} <select value={settings.locale} onChange={event => onChange({ ...settings, locale: event.target.value as AppearanceSettings['locale'] })}><option value="ru">{t('Русский')}</option><option value="en">{t('Английский')}</option></select></label>
      </div>
      <label className="appearance-settings__motion"><input type="checkbox" checked={settings.reduceMotion} onChange={event => onChange({ ...settings, reduceMotion: event.target.checked })} /> {t('Уменьшить анимацию')}</label>
      <p className="appearance-settings__note">{t('Настройки относятся к основному окну EvoHime.')}</p>
    </section>
  )
}
