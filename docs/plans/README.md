# Планы реализации

Каталог `docs/plans/` хранит незавершённые планы реализации. Обновлено:
2026-09-04. Реализованный комплект переносится в канонические документы и
удаляется из каталога; его наличие здесь означает, что направление ещё не
закрыто.

## Текущий каталог

В checkout сохранены планы 123–143. Реализованные планы 102, 119, 120, 121, 122 и 144 удалены после
переноса контракта в канонические документы.

| План | Тема | Состояние |
| --- | --- | --- |
| 120 | Grounded research workspace | реализован, контракт перенесён в канонические документы |
| 121 | Local model performance calibration | реализован, контракт перенесён в канонические документы |
| 122 | Verification evidence ledger | реализован, контракт перенесён в канонические документы |
| 123 | Content-aware context compression | реализован, контракт перенесён в канонические документы |
| 124 | Project quality contract | реализован, контракт перенесён в канонические документы |
| 125 | Free provider reliability routing | реализован, контракт перенесён в канонические документы |
| 126 | Design intent review lane | реализован, контракт перенесён в канонические документы |
| 127 | Remote client control plane | реализован MVP-контур; Android/server deployment unavailable |
| 128 | Local inference scheduler | реализован MVP-контур; inference adapter unavailable |
| 129 | Confidence-gated model cascade | реализован MVP-контур; producer/executor unavailable |
| [130](130-0-task-ownership-lease-fencing.md) | Task ownership lease fencing | незавершён |
| [131](131-0-unified-context-namespace.md) | Unified context namespace | незавершён |
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
| 144 | Modular release and component update | реализован, контракт перенесён в канонические документы |

Следующее новое направление получает номер `145`, если отдельное решение не
изменит порядок. План 144 не заменяет текущий installer, а добавляет
совместимый selective-update путь и fallback.

## Формат этапов

Имя файла имеет формат `NN-M-slug.md`:

- `NN` — номер направления;
- `M = 0` — обзор scope, контракта, ограничений и зависимостей;
- `M = 1` — Core-контракт, schema и storage;
- `M = 2` — runtime-интеграция и recovery;
- `M = 3` — IPC, client projection и UI;
- `M = 4` — verification, release evidence и закрытие.

Этапы выполняются строго `NN-1 → NN-2 → NN-3 → NN-4` после принятия overview.
Блокирующая зависимость допускается только от более раннего номера плана или
более раннего этапа того же плана. Зависимость от более позднего номера —
ошибка нумерации и должна быть исправлена до реализации.

Каждый файл этапа обязан явно разделять:

1. блокирующие зависимости;
2. опциональные зависимости и fail-closed fallback;
3. изменяемые контракты, migration/version и recovery;
4. выходные артефакты и focused verification;
5. критерии остановки, rollback и release evidence.

## Правило закрытия

План закрывается только после полного набора `0–4`: код, integration, recovery,
typed IPC/UI, tests, security/release evidence и каноническая документация.
После закрытия:

- контракт переносится в [`../architecture.md`](../architecture.md);
- фактический статус переносится в [`../current-state.md`](../current-state.md);
- проверочные результаты — в [`../release-evidence.md`](../release-evidence.md);
- временные файлы плана удаляются из этого каталога.

Наличие старой ссылки на удалённый plan-файл не является доказательством
незавершённой работы: такую ссылку нужно заменить ссылкой на канонический
документ. Не дублируйте здесь общий статус реализации.

## Граф текущей очереди

Порядок реализации определяется зависимостями, а не номерами файлов:

`139 → 141 → 134 → 130 → 132 → 131 → 137 → 128 → 129 → 133 → 135 →
136 → 140 → 143 → 138 → 127 → 142`.

Номера планов сохраняются как идентификаторы. Внутри каждого плана этапы
выполняются `0 → 1 → 2 → 3 → 4`. План 144 почти реализован отдельным
release-потоком и не входит в эту очередь. Опциональные adapter-направления не
должны становиться обязательными для базового Windows-пакета.

## Правила стыковки направлений

- `122` — общий владелец verification evidence для `124`, `135`, `136` и `140`;
- `134` предоставляет resource-pressure signals для `121`, `128`, `132` и `142`;
- `130` владеет lease/fencing, а `132` использует его для durable execution;
- `131` и `137` не создают параллельные registries: namespace контекста →
  готовый набор контекста; Execution Environment Profiles описаны в
  `docs/architecture.md` и не дублируются следующими планами;
- `121`, `125`, `128` и `129` используют единый gateway/resolver;
- `138` не создаёт второй механизм обновлений и использует контракт `144`.

## Проверка плана

Перед реализацией overview и после каждого этапа сверяйте план с кодом,
`AGENTS.md`, [`architecture.md`](../architecture.md), security policy и
release workflow. Минимальный gate: документационные ссылки разрешаются,
`git diff --check` проходит, а критерии этапа подтверждены свежими тестами.
