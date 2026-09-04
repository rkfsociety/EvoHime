# Планы реализации

Каталог `docs/plans/` хранит незавершённые планы реализации. Обновлено:
2026-09-04. Реализованный комплект переносится в канонические документы и
удаляется из каталога; его наличие здесь означает, что направление ещё не
закрыто.

## Текущий каталог

В checkout сохранены планы 119–143. Реализованный план 144 удалён после
переноса контракта в канонические документы.

| План | Тема | Состояние |
| --- | --- | --- |
| [119](119-0-execution-environment-profiles.md) | Execution environment profiles | незавершён |
| [120](120-0-grounded-research-workspace.md) | Grounded research workspace | незавершён |
| [121](121-0-local-model-performance-calibration.md) | Local model performance calibration | незавершён |
| [122](122-0-verification-evidence-ledger.md) | Verification evidence ledger | незавершён |
| [123](123-0-content-aware-context-compression.md) | Content-aware context compression | незавершён |
| [124](124-0-project-quality-contract.md) | Project quality contract | незавершён |
| [125](125-0-free-provider-reliability-routing.md) | Free provider reliability routing | незавершён |
| [126](126-0-design-intent-review-lane.md) | Design intent review lane | незавершён |
| [127](127-0-remote-client-control-plane.md) | Remote client control plane | незавершён |
| [128](128-0-local-inference-scheduler.md) | Local inference scheduler | незавершён |
| [129](129-0-confidence-gated-model-cascade.md) | Confidence-gated model cascade | незавершён |
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

`139 → 141 → 122 → 124 → 134 → 130 → 132 → 119 → 131 → 137 → 121 → 128 →
125 → 129 → 120 → 123 → 133 → 135 → 136 → 140 → 143 → 138 → 127 → 142`.

Номера планов сохраняются как идентификаторы. Внутри каждого плана этапы
выполняются `0 → 1 → 2 → 3 → 4`. План 144 почти реализован отдельным
release-потоком и не входит в эту очередь. Опциональные adapter-направления не
должны становиться обязательными для базового Windows-пакета.

## Правила стыковки направлений

- `122` — общий владелец verification evidence для `124`, `135`, `136` и `140`;
- `134` предоставляет resource-pressure signals для `121`, `128`, `132` и `142`;
- `130` владеет lease/fencing, а `132` использует его для durable execution;
- `119`, `131` и `137` не создают параллельные registries: окружение → namespace
  контекста → готовый набор контекста;
- `121`, `125`, `128` и `129` используют единый gateway/resolver;
- `138` не создаёт второй механизм обновлений и использует контракт `144`.

## Проверка плана

Перед реализацией overview и после каждого этапа сверяйте план с кодом,
`AGENTS.md`, [`architecture.md`](../architecture.md), security policy и
release workflow. Минимальный gate: документационные ссылки разрешаются,
`git diff --check` проходит, а критерии этапа подтверждены свежими тестами.
