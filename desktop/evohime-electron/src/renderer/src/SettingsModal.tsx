import { useEffect, useState } from 'react'

import { ListenerRuntimeSection } from './ListenerRuntimeSection'
import { ProviderForm } from './ProviderForm'
import { CodexPanel } from './CodexPanel'
import { SafetyPanel } from './SafetyPanel'
import { SkillCatalogPanel } from './SkillCatalogPanel'

import type { ConnectionState, CoreEvent } from '@shared/api'

type SettingsTab = 'provider' | 'workspace' | 'speech' | 'skills' | 'appearance' | 'security'

interface SettingsModalProps {
  readonly workspace: string | null
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly onClose: () => void
}

const TABS: readonly { readonly id: SettingsTab; readonly label: string }[] = [
  { id: 'provider', label: 'Провайдер и модели' },
  { id: 'workspace', label: 'Рабочая область' },
  { id: 'speech', label: 'Распознавание речи' },
  { id: 'skills', label: 'Agent Skills' },
  { id: 'appearance', label: 'Внешний вид' },
  { id: 'security', label: 'Безопасность' }
]

export function SettingsModal({ workspace, connection, events, onClose }: SettingsModalProps): React.JSX.Element {
  const [tab, setTab] = useState<SettingsTab>('provider')
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
            <h2 id="settings-title">Настройки</h2>
          </div>
          <button type="button" className="settings-modal__close" aria-label="Закрыть настройки" onClick={onClose}>
            ×
          </button>
        </header>

        <div className="settings-modal__body">
          <nav className="settings-tabs" aria-label="Разделы настроек">
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                className="settings-tabs__item"
                aria-selected={tab === item.id}
                role="tab"
                onClick={() => setTab(item.id)}
              >
                {item.label}
              </button>
            ))}
          </nav>

          <div className="settings-modal__content">
            {tab === 'provider' ? (
              <section className="provider-hub" aria-label="Провайдер и модели">
                <div className="provider-hub__tabs" role="tablist" aria-label="Источник моделей">
                  <button type="button" role="tab" aria-selected={providerSurface === 'api'} onClick={() => setProviderSurface('api')}>
                    API-провайдеры
                  </button>
                  <button type="button" role="tab" aria-selected={providerSurface === 'codex'} onClick={() => setProviderSurface('codex')}>
                    Codex CLI
                  </button>
                </div>
                {providerSurface === 'api' ? <ProviderForm /> : <CodexPanel />}
              </section>
            ) : null}
            {tab === 'workspace' ? <WorkspaceSettings workspace={workspace} /> : null}
            {tab === 'speech' ? <ListenerRuntimeSection /> : null}
            {tab === 'skills' ? <SkillCatalogPanel workspace={workspace} connection={connection} events={events} /> : null}
            {tab === 'appearance' ? <InfoSettings title="Внешний вид" text="Тёмная тема и компактная плотность интерфейса используются как основной режим EvoHime." /> : null}
            {tab === 'security' ? <SafetyPanel connection={connection} events={events} /> : null}
          </div>
        </div>
      </section>
    </div>
  )
}

function WorkspaceSettings({ workspace }: { readonly workspace: string | null }): React.JSX.Element {
  return (
    <section className="settings-info" aria-label="Рабочая область">
      <h3>Рабочая область</h3>
      <p>Текущая папка проекта выбирается в левой панели и используется агентом для задач.</p>
      <dl className="settings-info__details">
        <dt>Открытая папка</dt>
        <dd>{workspace ?? 'не выбрана'}</dd>
      </dl>
    </section>
  )
}

function InfoSettings({ title, text }: { readonly title: string; readonly text: string }): React.JSX.Element {
  return (
    <section className="settings-info" aria-label={title}>
      <h3>{title}</h3>
      <p>{text}</p>
      <span className="settings-info__badge">Настройка доступна в текущем режиме</span>
    </section>
  )
}
