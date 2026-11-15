import { Suspense, lazy, useCallback, useState } from 'react'

import type { ConnectionState, CoreEvent } from '@shared/api'
import { translate } from './i18n'

const WorkflowPanel = lazy(() => import('./WorkflowPanel').then((module) => ({ default: module.WorkflowPanel })))
const CapabilityRecipePanel = lazy(() => import('./CapabilityRecipePanel').then((module) => ({ default: module.CapabilityRecipePanel })))
const ConversationalWorkflowComposerPanel = lazy(() =>
  import('./ConversationalWorkflowComposerPanel').then((module) => ({ default: module.ConversationalWorkflowComposerPanel })),
)
const VisualWorkflowBuilderPanel = lazy(() =>
  import('./VisualWorkflowBuilderPanel').then((module) => ({ default: module.VisualWorkflowBuilderPanel })),
)

export type WorkflowsSubtab = 'run' | 'studio'

interface Props {
  readonly connection: ConnectionState
  readonly events: readonly CoreEvent[]
  readonly workspace: string | null
  readonly draftId: string
  readonly ownerScope: string | null
  readonly onOpenDraft: (draftId: string, ownerScope: string) => void
}

export function WorkflowsWorkspace({
  connection,
  events,
  workspace,
  draftId,
  ownerScope,
  onOpenDraft,
}: Props): React.JSX.Element {
  const [subtab, setSubtab] = useState<WorkflowsSubtab>('run')

  const handleOpenDraft = useCallback(
    (nextDraftId: string, nextOwnerScope: string) => {
      onOpenDraft(nextDraftId, nextOwnerScope)
      setSubtab('studio')
    },
    [onOpenDraft],
  )

  return (
    <div className="workflows-hub">
      <header className="workflows-hub__header">
        <div className="workflows-hub__tabs" role="tablist" aria-label={translate('Разделы составных задач')}>
          <button
            type="button"
            role="tab"
            id="workflows-tab-run"
            aria-selected={subtab === 'run'}
            aria-controls="workflows-pane-run"
            onClick={() => setSubtab('run')}
          >
            {translate('Запуск и сценарии')}
          </button>
          <button
            type="button"
            role="tab"
            id="workflows-tab-studio"
            aria-selected={subtab === 'studio'}
            aria-controls="workflows-pane-studio"
            onClick={() => setSubtab('studio')}
          >
            {translate('Создание и отладка')}
          </button>
        </div>
      </header>

      <div
        id="workflows-pane-run"
        role="tabpanel"
        aria-labelledby="workflows-tab-run"
        hidden={subtab !== 'run'}
        className={subtab === 'run' ? 'workflows-hub__pane' : 'workflows-hub__pane workflows-hub__pane--hidden'}
      >
        <Suspense fallback={null}>
          <WorkflowPanel connection={connection} events={events} workspace={workspace} />
          <CapabilityRecipePanel connection={connection} events={events} workspace={workspace} onOpenDraft={handleOpenDraft} />
        </Suspense>
      </div>

      <div
        id="workflows-pane-studio"
        role="tabpanel"
        aria-labelledby="workflows-tab-studio"
        hidden={subtab !== 'studio'}
        className={subtab === 'studio' ? 'workflows-hub__pane' : 'workflows-hub__pane workflows-hub__pane--hidden'}
      >
        <Suspense fallback={null}>
          <ConversationalWorkflowComposerPanel connection={connection} events={events} workspace={workspace} />
          <VisualWorkflowBuilderPanel
            connection={connection}
            events={events}
            workspace={workspace}
            draftId={draftId}
            ownerScope={ownerScope}
          />
        </Suspense>
      </div>
    </div>
  )
}
