import { useCallback, useEffect, useState } from 'react'

import type { RepositorySummary } from '@shared/api'

import { useShellApi } from './shell-api'

/**
 * Project, branch and uncommitted volume, above the composer.
 *
 * It answers the question a user has right before sending a task: where am I
 * about to change things, and how much is already changed. Nothing here can be
 * acted on by mistake — it is read-only, and the row can be dismissed.
 */

export interface RepositoryBarProps {
  readonly workspace: string
  /** Rises whenever the agent finishes work, so the counts stay current. */
  readonly refreshKey: number
}

export function RepositoryBar({
  workspace,
  refreshKey
}: RepositoryBarProps): React.JSX.Element | null {
  const api = useShellApi()
  const [repository, setRepository] = useState<RepositorySummary | null>(null)
  const [hidden, setHidden] = useState(false)

  const load = useCallback(async () => {
    if (!api) return
    const outcome = await api.invoke('repository.get', { workspacePath: workspace })
    if (outcome.ok) setRepository(outcome.value)
  }, [api, workspace])

  useEffect(() => {
    void load()
  }, [load, refreshKey])

  // A folder outside git has no branch to show, so the row stays out of the way.
  if (hidden || repository === null) {
    return null
  }

  const changed = repository.added > 0 || repository.removed > 0

  return (
    <div className="repobar">
      <span className="repobar__project">{projectName(workspace)}</span>
      <span className="repobar__branch">{repository.branch}</span>
      <span className="repobar__spacer" />
      {changed ? (
        <span className="repobar__diff" title="Незакоммиченные изменения">
          <span className="repobar__added">+{repository.added}</span>
          <span className="repobar__removed">−{repository.removed}</span>
        </span>
      ) : (
        <span className="repobar__clean">без изменений</span>
      )}
      <button
        type="button"
        className="repobar__close"
        aria-label="Скрыть строку проекта"
        onClick={() => setHidden(true)}
      >
        ✕
      </button>
    </div>
  )
}

function projectName(workspace: string): string {
  const parts = workspace.split(/[\\/]/).filter((part) => part.length > 0)
  return parts.at(-1) ?? workspace
}
