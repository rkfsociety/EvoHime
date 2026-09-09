# Планы реализации

Каталог `docs/plans/` содержит только незавершённые implementation contracts.
Обновлено: 2026-09-09. Наличие комплекта `NN-0` ... `NN-4` означает, что
направление ещё не закрыто; статус реализации не выводится из одного файла
плана.

## Источник статуса

Подтверждённое состояние checkout находится в [`../current-state.md`](../current-state.md),
архитектурные контракты — в [`../architecture.md`](../architecture.md), а
проверки поставки — в [`../release-evidence.md`](../release-evidence.md).
Эта страница является каталогом незавершённых направлений и не дублирует
таблицы реализации.

## Закрытые направления

Временные plan-файлы закрытых направлений удалены после переноса их контракта
и evidence в канонические документы. К закрытым относятся планы `01–131` и
`144`; планы `127–130` закрыты как MVP-контуры с явно сохранёнными
`unavailable` deployment/adapter gates. Их отсутствие из каталога не означает
отсутствие контракта: он находится в `architecture.md` и `current-state.md`.

## Активный каталог

Все перечисленные ниже направления имеют этапы `0–4`. Ссылки ведут на overview
этапа `0`; его блокирующие и опциональные зависимости являются источником
порядка реализации.

| План | Тема | Состояние |
| --- | --- | --- |
| [132](132-0-durable-background-execution-plane.md) | Durable background execution plane | незавершён |
| [133](133-0-built-in-deterministic-developer-utilities.md) | Built-in deterministic developer utilities | незавершён |
| [134](134-0-host-resource-telemetry-pressure-guard.md) | Host resource telemetry pressure guard | незавершён |
| [135](135-0-code-review-lane.md) | Code review lane | незавершён |
| [136](136-0-evidence-preserving-static-analysis-packs.md) | Evidence-preserving static analysis packs | незавершён |
| [137](137-0-agent-context-loadouts.md) | Agent context loadouts | незавершён |
| [138](138-0-skill-source-update-lifecycle.md) | Skill source update lifecycle | незавершён |
| [139](139-0-kernel-capability-facade.md) | Kernel capability facade | незавершён |
| [140](140-0-authorized-security-assessment-lane.md) | Authorized security assessment lane | незавершён |
| [141](141-0-runtime-service-graph.md) | Runtime service graph | незавершён |
| [142](142-0-agent-program-optimizer.md) | Agent program optimizer | незавершён |
| [143](143-0-project-knowledge-notebook.md) | Project knowledge notebook | незавершён |
| [145](145-0-git-remote-publication-protocol.md) | Git Remote Publication Protocol | незавершён |
| [146](146-0-voice-input-dictation.md) | Voice Input & Dictation | незавершён |
| [147](147-0-offline-experience-consolidation-cycle.md) | Offline Experience Consolidation Cycle | незавершён |
| [148](148-0-deterministic-review-execution-plan.md) | Deterministic Review Execution Plan | незавершён |
| [149](149-0-interactive-model-compare-workbench.md) | Interactive Model Compare Workbench | незавершён |
| [150](150-0-minimal-change-policy.md) | Minimal Change Policy | незавершён |
| [151](151-0-contextual-next-step-suggestions.md) | Contextual Next-Step Suggestions | незавершён |
| [152](152-0-autonomous-metric-experiment-runtime.md) | Autonomous Metric Experiment Runtime | незавершён |
| [153](153-0-native-computer-use-runtime.md) | Native Computer-Use Runtime | незавершён |
| [154](154-0-project-execution-board.md) | Project Execution Board | незавершён |
| [155](155-0-mobile-device-automation-runtime.md) | Mobile Device Automation Runtime | незавершён |
| [156](156-0-cross-modal-ui-grounding.md) | Cross-Modal UI Grounding | незавершён |
| [157](157-0-external-source-acquisition-runtime.md) | External Source Acquisition Runtime | незавершён |
| [158](158-0-temporal-signal-intelligence.md) | Temporal Signal Intelligence | незавершён |
| [159](159-0-temporal-memory-facts.md) | Temporal Memory Facts | незавершён |
| [160](160-0-ide-companion-bridge.md) | IDE Companion Bridge | незавершён |
| [161](161-0-verified-git-checkpoints.md) | Verified Git Checkpoints | незавершён |
| [162](162-0-verified-technical-diagram-artifacts.md) | Verified Technical Diagram Artifacts | незавершён |
| [163](163-0-local-model-compatibility-gateway.md) | Local Model Compatibility Gateway | незавершён |
| [164](164-0-semantic-activity-motion-system.md) | Semantic Activity Motion System | незавершён |
| [165](165-0-domain-workflow-recipes.md) | Domain Workflow Recipes | незавершён |
| [166](166-0-optional-voice-output-adapter.md) | Optional Voice Output Adapter | незавершён |
| [167](167-0-command-center.md) | Command Center | незавершён |

Номера `131–143` и `145–167` являются текущими идентификаторами активной
очереди. Пропуск `144` намеренный: это закрытый план модульного обновления.
Новая работа получает следующий свободный номер только после проверки
дубликатов и зависимостей.

## Формат этапов

Имя файла имеет формат `NN-M-slug.md`:

- `NN` — номер направления;
- `M = 0` — scope, контракт, ограничения и зависимости;
- `M = 1` — Core-контракт, schema и storage;
- `M = 2` — runtime-интеграция и recovery;
- `M = 3` — IPC, client projection и UI;
- `M = 4` — verification, release evidence и закрытие.

Этапы одного плана выполняются `0 → 1 → 2 → 3 → 4` после принятия overview.
Блокирующая зависимость от более позднего номера запрещена. Каждый этап
обязан разделять blocking и optional dependencies, изменяемые контракты,
recovery, verification, rollback и release evidence.

## Правило закрытия

План закрывается только после кода, integration, recovery, typed IPC/UI при
наличии, тестов, security/release evidence и обновления канонической
документации. После закрытия:

- контракт переносится в [`../architecture.md`](../architecture.md);
- фактический статус — в [`../current-state.md`](../current-state.md);
- проверки — в [`../release-evidence.md`](../release-evidence.md);
- временные файлы плана удаляются из этого каталога.

Старая ссылка на удалённый plan-файл не является доказательством незавершённой
работы: её нужно заменить ссылкой на канонический документ.

## Стыковка направлений

- `122` остаётся владельцем verification evidence для `124`, `135`, `136` и
  `140`;
- `134` предоставляет resource-pressure signals для runtime-направлений;
- `130` владеет lease/fencing, а `132` использует этот уже закрытый контракт;
- `131` и `137` не создают параллельные registries;
- `121`, `125`, `128` и `129` используют единый gateway/resolver;
- `138` не создаёт второй механизм обновлений и использует контракт `144`.

## Проверка

Перед реализацией overview и после каждого этапа сверяйте план с кодом,
`AGENTS.md`, [`../architecture.md`](../architecture.md), security policy и
release workflow. Минимальный gate: ссылки разрешаются, `git diff --check`
проходит, а критерии этапа подтверждены свежими тестами.
