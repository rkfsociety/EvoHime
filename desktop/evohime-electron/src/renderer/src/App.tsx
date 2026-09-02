import { useEffect, useRef, useState } from 'react'

import type {
  ConnectionState,
  CoreEvent,
  ListeningReason,
  ListeningState,
  RepairStatus,
  ShellState,
  UserIdentity
} from '@shared/api'
import type { UpdateStatus } from '@shared/update'
import { shortCommit } from '@shared/update'

import { useShellApi } from './shell-api'
import { UpdateIndicator } from './UpdateIndicator'
import { UpdateGate } from './UpdateGate'
import { ProjectSidebar } from './ProjectSidebar'
import { TaskTimeline } from './TaskTimeline'
import { SettingsModal } from './SettingsModal'
import { OperationsPanel } from './OperationsPanel'
import { PlanReviewPanel } from './PlanReviewPanel'
import { WorkflowPanel } from './WorkflowPanel'
import { OverviewPanel } from './OverviewPanel'
import { ListeningPanel, REASON_TEXTS, STATE_TITLES } from './ListeningPanel'
import { TracePanel } from './TracePanel'
import { RecoveryBanner } from './RecoveryBanner'
import { ContinuationPanel } from './ContinuationPanel'
import { AnalysisKernelPanel } from './AnalysisKernelPanel'
import { WorkflowPackagePanel } from './WorkflowPackagePanel'
import { VisualWorkflowBuilderPanel } from './VisualWorkflowBuilderPanel'
import { ConversationalWorkflowComposerPanel } from './ConversationalWorkflowComposerPanel'
import { AgentBenchmarkMatrixPanel } from './AgentBenchmarkMatrixPanel'
import { AgentMiddlewarePipelinePanel } from './AgentMiddlewarePipelinePanel'
import { StructuredResponseContractPanel } from './StructuredResponseContractPanel'
import { SensitiveDataGuardrailsPanel } from './SensitiveDataGuardrailsPanel'
import { ExecutionPolicyProfilesPanel } from './ExecutionPolicyProfilesPanel'
import { ModelResiliencePolicyPanel } from './ModelResiliencePolicyPanel'
import { ExecutionBackendRegistryPanel } from './ExecutionBackendRegistryPanel'
import { ToolSimulationRuntimePanel } from './ToolSimulationRuntimePanel'
import { AgentRoleProfilesPanel } from './AgentRoleProfilesPanel'
import { ArtifactHandoffRegistryPanel } from './ArtifactHandoffRegistryPanel'
import { TeamSopProtocolsPanel } from './TeamSopProtocolsPanel'
import { CausalCollaborationBusPanel } from './CausalCollaborationBusPanel'
import { HumanWorkItemsPanel } from './HumanWorkItemsPanel'
import { PlanArtifactPanel } from './PlanArtifactPanel'
import { WorkspaceStateCheckpointsPanel } from './WorkspaceStateCheckpointsPanel'
import { IncrementalChangeProtocolPanel } from './IncrementalChangeProtocolPanel'
import { RevisionSafeWorkspaceFilesPanel } from './RevisionSafeWorkspaceFilesPanel'
import { TaskWorktreeIsolationPanel } from './TaskWorktreeIsolationPanel'
import { TeamResourceBudgetPanel } from './TeamResourceBudgetPanel'
import { ComposableTerminationConditionsPanel } from './ComposableTerminationConditionsPanel'
import { WorkspaceBootstrapManifestPanel } from './WorkspaceBootstrapManifestPanel'
import { TeamCoordinationPoliciesPanel } from './TeamCoordinationPoliciesPanel'
import { TypedAgentHandoffContractPanel } from './TypedAgentHandoffContractPanel'
import { SchemaDrivenAgentConfigurationPanel } from './SchemaDrivenAgentConfigurationPanel'
import { ExperienceReplayLibraryPanel } from './ExperienceReplayLibraryPanel'
import { RuntimeInterventionPipelinePanel } from './RuntimeInterventionPipelinePanel'
import { CodeDiagnosticsFeedbackLoopPanel } from './CodeDiagnosticsFeedbackLoopPanel'
import { WorkflowOptimizationLabPanel } from './WorkflowOptimizationLabPanel'
import { CoreTopicSubscriptionEventBusPanel } from './CoreTopicSubscriptionEventBusPanel'
import { DependencyAwareTaskGraphPanel } from './DependencyAwareTaskGraphPanel'
import { DeclarativeAgentComponentRegistryPanel } from './DeclarativeAgentComponentRegistryPanel'
import { WorkbenchPanel } from './WorkbenchPanel'
import { AgenticBrowserSessionPanel } from './AgenticBrowserSessionPanel'

/**
 * Stage 0 shell surface: it only renders the connection state owned by the main
 * process. No business logic lives here, and the renderer never touches the
 * workspace, the pipe or a shell (plan 0, rule 2 of AGENTS.md).
 */

// Keep enough of the replayed Core journal for a useful diagnostic export.
// Core still bounds replay and redacts sensitive payloads before IPC.
const MAX_VISIBLE_EVENTS = 2_000

const STATE_LABELS: Record<ConnectionState, string> = {
  starting: 'Запуск',
  connecting: 'Подключение к Core',
  connected: 'Подключено',
  reconnecting: 'Переподключение',
  replaying: 'Восстановление событий',
  resyncing: 'Синхронизация состояния',
  'state-gap': 'Пробел в состоянии — нужна пересинхронизация',
  'version-mismatch': 'Несовместимая версия IPC',
  degraded: 'Ограниченный режим',
  fatal: 'Критическая ошибка'
}

type ViewId = 'chat' | 'overview' | 'reviews' | 'operations' | 'workflows' | 'packages' | 'continuations' | 'kernels' | 'listening' | 'benchmarks' | 'middleware' | 'structured-response' | 'sensitive-data' | 'execution-policy' | 'model-resilience' | 'execution-backends' | 'tool-simulation' | 'agent-role-profiles' | 'artifact-handoff-registry' | 'team-sop' | 'causal-collaboration' | 'human-work-items' | 'plan-artifacts' | 'workspace-checkpoints' | 'incremental-change' | 'revision-safe-files' | 'task-worktree-isolation' | 'team-resource-budget' | 'composable-termination-conditions' | 'workspace-bootstrap-manifest' | 'team-coordination-policies' | 'typed-agent-handoff-contract' | 'schema-driven-agent-configuration' | 'experience-replay-library' | 'runtime-intervention-pipeline' | 'code-diagnostics-feedback-loop' | 'workflow-optimization-lab' | 'core-topic-subscription-event-bus' | 'dependency-aware-task-graph' | 'declarative-agent-component-registry'

interface ViewDescriptor {
  readonly id: ViewId
  readonly label: string
  readonly icon: string
}

/**
 * Tool sections only. The conversation is not a nav row: it is reached by
 * opening a chat, which is where the user already looks for it.
 */
const USER_VIEWS: readonly ViewDescriptor[] = [
  { id: 'overview', label: 'Обзор', icon: '◉' },
  { id: 'reviews', label: 'Ревью планов', icon: '✓' },
  { id: 'operations', label: 'Память и Pulse', icon: '◌' },
  { id: 'workflows', label: 'Составные задачи', icon: '⛓' },
  { id: 'continuations', label: 'Продолжения', icon: '↻' },
  { id: 'kernels', label: 'Анализ', icon: '⌘' },
  { id: 'listening', label: 'Слух', icon: '🎙' },
  { id: 'human-work-items', label: 'Задачи для человека', icon: '☑' },
]

const DEVELOPER_VIEWS: readonly ViewDescriptor[] = [
  { id: 'plan-artifacts', label: 'Plan Artifacts', icon: '◇' },
  { id: 'workspace-checkpoints', label: 'Workspace Checkpoints', icon: '▣' },
  { id: 'incremental-change', label: 'Incremental Change', icon: 'Δ' },
  { id: 'revision-safe-files', label: 'Revision-safe Files', icon: '◈' },
  { id: 'task-worktree-isolation', label: 'Task Worktrees', icon: '⑂' },
  { id: 'team-resource-budget', label: 'Team Budget', icon: '₿' },
  { id: 'composable-termination-conditions', label: 'Termination', icon: '⏹' },
  { id: 'workspace-bootstrap-manifest', label: 'Bootstrap Manifest', icon: '⌂' },
  { id: 'team-coordination-policies', label: 'Team Coordination', icon: '⇄' },
  { id: 'typed-agent-handoff-contract', label: 'Typed Handoff', icon: '⇢' },
  { id: 'schema-driven-agent-configuration', label: 'Schema Configuration', icon: '⚙' },
  { id: 'experience-replay-library', label: 'Experience Replay', icon: '↺' },
  { id: 'runtime-intervention-pipeline', label: 'Runtime Intervention', icon: '⚠' },
  { id: 'code-diagnostics-feedback-loop', label: 'Code Diagnostics', icon: '⌁' },
  { id: 'workflow-optimization-lab', label: 'Workflow Lab', icon: '◌' },
  { id: 'core-topic-subscription-event-bus', label: 'Topic Bus', icon: '⇆' },
  { id: 'dependency-aware-task-graph', label: 'Task Graph', icon: '◇' },
  { id: 'declarative-agent-component-registry', label: 'Components', icon: '▦' },
  { id: 'packages', label: 'Workflow Package', icon: '⇄' },
  { id: 'benchmarks', label: 'Бенчмарки', icon: '▦' },
  { id: 'middleware', label: 'Middleware', icon: '◇' },
  { id: 'structured-response', label: 'Structured response', icon: '▤' },
  { id: 'sensitive-data', label: 'Защита данных', icon: '⌁' },
  { id: 'execution-policy', label: 'Профили выполнения', icon: '▣' },
  { id: 'model-resilience', label: 'Надёжность модели', icon: '↯' },
  { id: 'execution-backends', label: 'Среды выполнения', icon: '⌘' },
  { id: 'tool-simulation', label: 'Симуляция инструментов', icon: '◇' },
  { id: 'agent-role-profiles', label: 'Профили ролей', icon: '◎' },
  { id: 'artifact-handoff-registry', label: 'Артефакты и передачи', icon: '◇' },
  { id: 'team-sop', label: 'Team SOP', icon: '∷' },
  { id: 'causal-collaboration', label: 'Collaboration Bus', icon: '⇆' },
]

const VIEWS: readonly ViewDescriptor[] = [...USER_VIEWS, ...DEVELOPER_VIEWS]

/** Not a nav row: reached through the gear next to the account. */
const SETTINGS_LABEL = 'Настройки'

export function App(): React.JSX.Element {
  const [state, setState] = useState<ShellState | null>(null)
  const [events, setEvents] = useState<readonly CoreEvent[]>([])
  const [apiMissing, setApiMissing] = useState(false)
  const [view, setView] = useState<ViewId>('chat')
  const [workspace, setWorkspace] = useState<string | null>(null)
  const [chatId, setChatId] = useState<string | null>(null)
  // Bumped when a chat is renamed or reordered so the sidebar reloads its list.
  const [chatRevision, setChatRevision] = useState(0)
  const [identity, setIdentity] = useState<UserIdentity | null>(null)
  const [update, setUpdate] = useState<UpdateStatus | null>(null)
  const [repair, setRepair] = useState<RepairStatus | null>(null)
  const [traceOpen, setTraceOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [accountMenuOpen, setAccountMenuOpen] = useState(false)
  const [developerMenuOpen, setDeveloperMenuOpen] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  // Дополнительные панели не должны конкурировать с чатом при первом запуске.
  // Пользователь открывает их явно из верхней панели.
  const [workbenchVisible, setWorkbenchVisible] = useState(false)
  const [browserVisible, setBrowserVisible] = useState(false)
  const accountMenuRef = useRef<HTMLDivElement | null>(null)

  const api = useShellApi()

  useEffect(() => {
    if (!api) {
      setApiMissing(true)
      return
    }

    const unsubscribe = api.subscribe((event) => {
      if (event.kind === 'state') {
        setState(event.state)
        return
      }
      if (event.kind === 'update') {
        setUpdate(event.status)
        return
      }
      if (event.kind === 'repair') {
        setRepair(event.status)
        return
      }
      // Состояние речевого рантайма слушает только его собственный экран:
      // в общую ленту событий оно не попадает.
      if (event.kind !== 'core-event') {
        return
      }
      setEvents((current) => [event.event, ...current].slice(0, MAX_VISIBLE_EVENTS))
    })

    void api.invoke('shell.getState', {}).then((outcome) => {
      if (outcome.ok) {
        setState(outcome.value)
      }
    })

    void api.invoke('update.getStatus', {}).then((outcome) => {
      if (outcome.ok) setUpdate(outcome.value)
    })
    void api.invoke('repair.getStatus', {}).then((outcome) => {
      if (outcome.ok) setRepair(outcome.value)
    })

    return unsubscribe
  }, [api])

  useEffect(() => {
    if (!api) return
    void api.invoke('identity.get', {}).then((outcome) => {
      if (outcome.ok) setIdentity(outcome.value)
    })
  }, [api])

  useEffect(() => {
    if (!accountMenuOpen) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setAccountMenuOpen(false)
    }
    const onPointerDown = (event: PointerEvent) => {
      if (event.target instanceof Node && !accountMenuRef.current?.contains(event.target)) {
        setAccountMenuOpen(false)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    window.addEventListener('pointerdown', onPointerDown)
    return () => {
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('pointerdown', onPointerDown)
    }
  }, [accountMenuOpen])

  if (apiMissing) {
    return (
      <main className="shell shell--recovery">
        <h1>Оболочка недоступна</h1>
        <p>Мост preload не загрузился. Перезапусти приложение.</p>
      </main>
    )
  }

  const connection = state?.connection ?? 'starting'
  const title = view === 'chat' ? 'Диалог' : (VIEWS.find((item) => item.id === view)?.label ?? 'Диалог')

  return (
    <div className={`shell${traceOpen ? ' shell--trace-open' : ''}${sidebarCollapsed ? ' shell--sidebar-collapsed' : ''}`}>
      <nav className="sidebar" aria-label="Разделы">
        <div className="sidebar__brand">
          <button
            type="button"
            className="sidebar__brand-toggle"
            aria-label={sidebarCollapsed ? 'Развернуть боковую панель' : 'Свернуть боковую панель'}
            title={sidebarCollapsed ? 'Развернуть боковую панель' : 'Свернуть боковую панель'}
            onClick={() => setSidebarCollapsed((value) => !value)}
          >
            <span className="sidebar__logo" aria-hidden="true">E</span>
          </button>
          <h1 className="sidebar__title">EvoHime</h1>
        </div>

        <div className="sidebar__projects">
          <ProjectSidebar
            connection={connection}
            workspace={workspace}
            chatId={chatId}
            onWorkspaceChange={setWorkspace}
            onChatChange={(id) => {
              setChatId(id)
              // Picking a chat means going back to the conversation.
              if (id !== null) setView('chat')
            }}
            revision={chatRevision}
          />
        </div>

        <div className="account" ref={accountMenuRef}>
          <button
            type="button"
            className="account__user"
            aria-expanded={accountMenuOpen}
            aria-haspopup="menu"
            onClick={() => setAccountMenuOpen((value) => !value)}
          >
            <span className="account__avatar" aria-hidden="true">
              {(identity?.name ?? '?').slice(0, 1).toUpperCase()}
            </span>
            <span className="account__copy">
              <span className="account__name" title={identityTitle(identity)}>
                {identity?.name ?? '…'}
              </span>
              <small>Разделы и настройки</small>
            </span>
            <span className="account__chevron" aria-hidden="true">⌃</span>
          </button>
          <UpdateIndicator status={update} />
          {accountMenuOpen ? (
            <div className="account__menu" role="menu" aria-label="Разделы и настройки">
              {USER_VIEWS.map((item) => (
                <NavItem
                  key={item.id}
                  view={item}
                  active={item.id === view}
                  onSelect={(id) => {
                    setView(id)
                    setAccountMenuOpen(false)
                  }}
                />
              ))}
              <button
                type="button"
                className="account__menu-item"
                role="menuitem"
                aria-expanded={developerMenuOpen}
                aria-haspopup="menu"
                onClick={() => setDeveloperMenuOpen((value) => !value)}
              >
                <span aria-hidden="true">⌘</span>
                Интерфейс разработчика
                <span className="account__menu-chevron" aria-hidden="true">
                  {developerMenuOpen ? '⌃' : '⌄'}
                </span>
              </button>
              {developerMenuOpen ? (
                <div className="account__developer-menu" role="menu" aria-label="Интерфейс разработчика">
                  {DEVELOPER_VIEWS.map((item) => (
                    <NavItem
                      key={item.id}
                      view={item}
                      active={item.id === view}
                      onSelect={(id) => {
                        setView(id)
                        setAccountMenuOpen(false)
                      }}
                    />
                  ))}
                </div>
              ) : null}
              <button
                type="button"
                className="account__menu-item"
                role="menuitem"
                aria-current={settingsOpen ? 'page' : undefined}
                onClick={() => {
                  setSettingsOpen(true)
                  setAccountMenuOpen(false)
                }}
              >
                <span aria-hidden="true">⚙</span>
                {SETTINGS_LABEL}
              </button>
            </div>
          ) : null}
        </div>
      </nav>

      <main className="main">
        <header className="topbar">
          <h2 className="topbar__title">{title}</h2>
          <span className="topbar__path">{workspace ?? 'папка не выбрана'}</span>
          <span className="topbar__spacer" />
          {!workbenchVisible ? (
            <button type="button" className="topbar__panel-toggle" onClick={() => setWorkbenchVisible(true)}>
              Рабочая панель
            </button>
          ) : null}
          {!browserVisible ? (
            <button type="button" className="topbar__panel-toggle" onClick={() => setBrowserVisible(true)}>
              Открыть браузер
            </button>
          ) : null}
          <button
            type="button"
            className={`topbar__trace${traceOpen ? ' topbar__trace--active' : ''}`}
            aria-label={traceOpen ? 'Закрыть трейс' : 'Открыть трейс'}
            aria-pressed={traceOpen}
            onClick={() => setTraceOpen((value) => !value)}
          >
            Трейс
          </button>
          <ListeningIndicator events={events} />
          <span className={`status-pill status-pill--${connection}`}>{STATE_LABELS[connection]}</span>
        </header>

        <RecoveryBanner
          connection={connection}
          events={events}
          onOpenTask={() => setView('chat')}
          taskScoped={false}
        />

        <div className="main__body">
          {view === 'chat' ? (
            <div className={`conversation-layout${workbenchVisible || browserVisible ? '' : ' conversation-layout--chat-only'}`}>
              <TaskTimeline
                connection={connection}
                events={events}
                workspace={workspace}
                chatId={chatId}
                onChatTouched={() => setChatRevision((value) => value + 1)}
                onChatOpened={(id) => {
                  setChatId(id)
                  setChatRevision((value) => value + 1)
                }}
                identityName={identity?.name ?? null}
                chatRevision={chatRevision}
              />
              {workbenchVisible || browserVisible ? (
                <aside className="conversation-side-panels" aria-label="Дополнительные панели">
                  {workbenchVisible ? (
                    <WorkbenchPanel
                      connection={connection}
                      chatId={chatId}
                      workspace={workspace}
                      events={events}
                      onClose={() => setWorkbenchVisible(false)}
                    />
                  ) : null}
                  {browserVisible ? <AgenticBrowserSessionPanel onClose={() => setBrowserVisible(false)} /> : null}
                </aside>
              ) : null}
            </div>
          ) : (
            <div className="main__scroll">
              {view === 'overview' ? <OverviewPanel connection={connection} events={events} workspace={workspace} /> : null}
              {view === 'reviews' ? <PlanReviewPanel connection={connection} events={events} /> : null}
              {view === 'operations' ? <OperationsPanel connection={connection} events={events} repair={repair} /> : null}
              {view === 'workflows' ? (
                <>
                  <WorkflowPanel connection={connection} events={events} workspace={workspace} />
                  <ConversationalWorkflowComposerPanel connection={connection} events={events} workspace={workspace} />
                  <VisualWorkflowBuilderPanel connection={connection} events={events} workspace={workspace} />
                </>
              ) : null}
              {view === 'packages' ? <WorkflowPackagePanel /> : null}
              {view === 'continuations' ? <ContinuationPanel connection={connection} events={events} /> : null}
              {view === 'kernels' ? <AnalysisKernelPanel connection={connection} events={events} /> : null}
              {view === 'listening' ? <ListeningPanel connection={connection} events={events} /> : null}
              {view === 'benchmarks' ? <AgentBenchmarkMatrixPanel /> : null}
              {view === 'middleware' ? <AgentMiddlewarePipelinePanel /> : null}
              {view === 'structured-response' ? <StructuredResponseContractPanel /> : null}
              {view === 'sensitive-data' ? <SensitiveDataGuardrailsPanel /> : null}
              {view === 'execution-policy' ? <ExecutionPolicyProfilesPanel /> : null}
              {view === 'model-resilience' ? <ModelResiliencePolicyPanel /> : null}
              {view === 'execution-backends' ? <ExecutionBackendRegistryPanel /> : null}
              {view === 'tool-simulation' ? <ToolSimulationRuntimePanel /> : null}
              {view === 'agent-role-profiles' ? <AgentRoleProfilesPanel /> : null}
              {view === 'artifact-handoff-registry' ? <ArtifactHandoffRegistryPanel /> : null}
              {view === 'team-sop' ? <TeamSopProtocolsPanel /> : null}
              {view === 'causal-collaboration' ? <CausalCollaborationBusPanel /> : null}
              {view === 'human-work-items' ? <HumanWorkItemsPanel /> : null}
              {view === 'plan-artifacts' ? <PlanArtifactPanel connection={connection} events={events} /> : null}
              {view === 'workspace-checkpoints' ? <WorkspaceStateCheckpointsPanel connection={connection} events={events} /> : null}
              {view === 'incremental-change' ? <IncrementalChangeProtocolPanel connection={connection} events={events} /> : null}
              {view === 'revision-safe-files' ? <RevisionSafeWorkspaceFilesPanel connection={connection} events={events} /> : null}
              {view === 'task-worktree-isolation' ? <TaskWorktreeIsolationPanel connection={connection} events={events} /> : null}
              {view === 'team-resource-budget' ? <TeamResourceBudgetPanel connection={connection} events={events} /> : null}
              {view === 'composable-termination-conditions' ? <ComposableTerminationConditionsPanel connection={connection} events={events} /> : null}
              {view === 'workspace-bootstrap-manifest' ? <WorkspaceBootstrapManifestPanel connection={connection} events={events} /> : null}
              {view === 'team-coordination-policies' ? <TeamCoordinationPoliciesPanel connection={connection} events={events} /> : null}
              {view === 'typed-agent-handoff-contract' ? <TypedAgentHandoffContractPanel connection={connection} events={events} /> : null}
              {view === 'schema-driven-agent-configuration' ? <SchemaDrivenAgentConfigurationPanel connection={connection} events={events} /> : null}
              {view === 'experience-replay-library' ? <ExperienceReplayLibraryPanel connection={connection} events={events} /> : null}
              {view === 'runtime-intervention-pipeline' ? <RuntimeInterventionPipelinePanel connection={connection} events={events} /> : null}
              {view === 'code-diagnostics-feedback-loop' ? <CodeDiagnosticsFeedbackLoopPanel connection={connection} /> : null}
              {view === 'workflow-optimization-lab' ? <WorkflowOptimizationLabPanel connection={connection} /> : null}
              {view === 'core-topic-subscription-event-bus' ? <CoreTopicSubscriptionEventBusPanel connection={connection} /> : null}
              {view === 'dependency-aware-task-graph' ? <DependencyAwareTaskGraphPanel connection={connection} /> : null}
              {view === 'declarative-agent-component-registry' ? <DeclarativeAgentComponentRegistryPanel connection={connection} /> : null}
            </div>
          )}
        </div>
      </main>

      {settingsOpen ? (
        <SettingsModal
          workspace={workspace}
          connection={connection}
          events={events}
          onClose={() => setSettingsOpen(false)}
        />
      ) : null}

      {traceOpen ? (
        <TracePanel
          chatId={chatId}
          chatRevision={chatRevision}
          events={events}
          state={state}
          workspace={workspace}
          onClose={() => setTraceOpen(false)}
        />
      ) : null}

      <footer className="statusbar">
        <span>Протокол {state?.protocol ? `v${state.protocol.major}.${state.protocol.minor}` : '—'}</span>
        <span>Core {state?.coreVersion ?? '—'}</span>
        {/* Сборка опознаётся коммитом: релизный номер — только ярлык установщика. */}
        {update && update.phase !== 'disabled' ? (
          <span title={`Ветка ${update.branch}`}>сборка {shortCommit(update.installedCommit)}</span>
        ) : null}
        <span>seq {state?.lastSequence ?? 0}</span>
        {(state?.reconnectAttempts ?? 0) > 0 ? <span>переподключений: {state?.reconnectAttempts}</span> : null}
        <span className="statusbar__spacer" />
        {state?.reason ? <span className="statusbar__reason">{state.reason}</span> : null}
      </footer>

      {update ? <UpdateGate status={update} /> : null}
    </div>
  )
}

/**
 * Индикатор записи в шапке. Виден на любой вкладке: узнать, слушают ли тебя,
 * нельзя ставить в зависимость от того, какой раздел открыт.
 *
 * Fail-visible: пока состояние не пришло, показывается «проверка состояния» с
 * предупреждением, а не «выключено». Утверждать, что микрофон выключен, можно
 * только зная это.
 */
export function ListeningIndicator({
  events
}: {
  readonly events: readonly CoreEvent[]
}): React.JSX.Element {
  // `events` holds the newest event first (prepended on receipt below), so
  // the latest match is the FIRST one found — not the last.
  const payload = events.find(
    (event) => event.eventType === 'ambient.state' || event.eventType === 'ambient.status'
  )
  let state: ListeningState | null = null
  let reason: ListeningReason | null = null
  if (payload) {
    try {
      const value = JSON.parse(payload.payload) as { state?: ListeningState; reason?: ListeningReason }
      state = value.state ?? null
      reason = value.reason ?? null
    } catch {
      state = null
    }
  }
  const unknown = state === null || state === 'engine_unavailable'
  const live = state === 'listening' || state === 'starting'
  const title = state === null ? 'Слушание: проверка состояния…' : STATE_TITLES[state]
  const tooltip = reason === null ? title : `${title} — ${REASON_TEXTS[reason]}`
  return (
    <span
      className={`listening-pill${live ? ' listening-pill--live' : ''}${unknown ? ' listening-pill--unknown' : ''}`}
      role="status"
      title={tooltip}
    >
      <span aria-hidden="true">{live ? '🎙' : unknown ? '⚠️' : '⏸'}</span>
      {title}
    </span>
  )
}

/** Says where the name came from, so an unexpected one is explainable. */
function identityTitle(identity: UserIdentity | null): string {
  if (!identity) return ''
  const source = {
    github: 'GitHub CLI',
    git: 'git config user.name',
    os: 'учётная запись Windows'
  }[identity.source]
  return `${identity.name} · ${source}`
}

interface NavItemProps {
  readonly view: ViewDescriptor
  readonly active: boolean
  readonly onSelect: (id: ViewId) => void
}

function NavItem({ view, active, onSelect }: NavItemProps): React.JSX.Element {
  return (
    <button
      type="button"
      className="nav-item"
      role="menuitem"
      aria-current={active ? 'page' : undefined}
      onClick={() => onSelect(view.id)}
    >
      <span className="nav-item__icon" aria-hidden="true">{view.icon}</span>
      {view.label}
    </button>
  )
}
