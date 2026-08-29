import { useEffect, useMemo, useState } from 'react'

import type { ConnectionState, CoreEvent, SkillCatalog, SkillContentResult } from '@shared/api'

import { useShellApi } from './shell-api'

const CONNECTED_STATES: readonly ConnectionState[] = ['connected', 'replaying', 'resyncing']

interface SkillCatalogPanelProps {
  readonly workspace: string | null
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
}

/** Metadata-first skills surface. Full SKILL.md content appears only after an explicit click. */
export function SkillCatalogPanel({ workspace, connection, events }: SkillCatalogPanelProps): React.JSX.Element {
  const api = useShellApi()
  const [catalog, setCatalog] = useState<SkillCatalog | null>(null)
  const [loaded, setLoaded] = useState<SkillContentResult | null>(null)
  const projectedCatalog = useMemo(
    () => events.find((event) => event.skillCatalog)?.skillCatalog ?? null,
    [events]
  )
  const projectedContent = useMemo(
    () => events.find((event) => event.skillContent)?.skillContent ?? null,
    [events]
  )

  useEffect(() => {
    setCatalog(null)
    setLoaded(null)
  }, [workspace])

  useEffect(() => {
    if (projectedCatalog) setCatalog(projectedCatalog)
  }, [projectedCatalog])

  useEffect(() => {
    if (projectedContent) setLoaded(projectedContent)
  }, [projectedContent])

  useEffect(() => {
    if (!api || !workspace || !CONNECTED_STATES.includes(connection)) return
    void api.invoke('core.listSkills', { workspacePath: workspace, limit: 128 })
  }, [api, connection, workspace])

  const requestSkill = (skillId: string) => {
    if (!api || !workspace || !CONNECTED_STATES.includes(connection)) return
    void api.invoke('core.loadSkill', { workspacePath: workspace, skillId, maxBytes: 256 * 1024 })
  }

  return (
    <section className="settings-info skill-catalog" aria-label="Agent Skills">
      <h3>Agent Skills</h3>
      <p>Core показывает только bounded metadata. Полный SKILL.md загружается отдельным явным действием.</p>
      {!workspace ? <span className="settings-info__badge">Сначала выбери рабочую область</span> : null}
      {catalog?.diagnostics.map((diagnostic) => (
        <p className="skill-catalog__diagnostic" role="alert" key={`${diagnostic.code}-${diagnostic.sourceRef}`}>
          {diagnostic.code}: {diagnostic.skillId || diagnostic.sourceRef}
        </p>
      ))}
      <div className="skill-catalog__list">
        {catalog?.skills.map((skill) => (
          <article className={`skill-catalog__item skill-catalog__item--${skill.validationStatus}`} key={skill.skillId}>
            <div className="skill-catalog__heading">
              <strong>{skill.name || skill.skillId}</strong>
              <code>{skill.version || 'invalid'}</code>
            </div>
            <p>{skill.description || `Ошибка: ${skill.validationErrorCode || 'invalid_skill'}`}</p>
            <small>{skill.sourceKind} · {skill.scope} · {skill.contentHash.slice(0, 12)}…</small>
            {skill.allowedTools.length > 0 ? <small>Tools: {skill.allowedTools.join(', ')}</small> : null}
            <button
              type="button"
              onClick={() => requestSkill(skill.skillId)}
              disabled={skill.validationStatus !== 'valid' || !CONNECTED_STATES.includes(connection)}
            >
              Загрузить skill явно
            </button>
          </article>
        ))}
      </div>
      {catalog && catalog.skills.length === 0 ? <span className="settings-info__badge">В разрешённых roots skills не найдены</span> : null}
      {loaded ? (
        <details className="skill-catalog__loaded" open={loaded.errorCode === ''}>
          <summary>{loaded.errorCode ? `Ошибка ${loaded.errorCode}` : `Загружен ${loaded.skillId} · ${loaded.version}`}</summary>
          {loaded.errorCode ? <p role="alert">{loaded.errorMessage}</p> : <pre>{loaded.content}</pre>}
        </details>
      ) : null}
    </section>
  )
}
