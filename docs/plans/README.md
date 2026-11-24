# Планы реализации

Каталог `docs/plans/` содержит только незавершённые implementation contracts.
Обновлено: 2026-11-21. Наличие комплекта `NN-0` ... `NN-4` означает, что
направление ещё не закрыто; статус реализации не выводится из одного файла
плана.

## Источник статуса
Подтверждённое состояние checkout находится в [`../current-state.md`](../current-state.md),
архитектурные контракты — в [`../architecture.md`](../architecture.md) и [`../architecture/`](../architecture/), а
проверки поставки — в [`../release-evidence.md`](../release-evidence.md).
Эта страница является каталогом незавершённых направлений и не дублирует
таблицы реализации.

## Закрытые направления
Временные plan-файлы закрытых направлений удалены после переноса их контракта
и evidence в канонические документы. К закрытым относятся планы `01–179`,
`181`, `183`, `189–190` (включая `144`), `192`, `196`, `197` и `198`; планы `127–130` закрыты как MVP-контуры
с явно сохранёнными `unavailable` deployment/adapter gates. Их отсутствие
из каталога не означает отсутствие контракта: он находится в
`architecture.md`, `docs/architecture/*.md` и `current-state.md`.

## Активный каталог
Все перечисленные ниже направления имеют этапы `0–4`. Ссылки ведут на overview
этапа `0`; его блокирующие и опциональные зависимости являются источником
порядка реализации.

| План | Тема | Состояние |
| --- | --- | --- |
| 180 | [Core-owned A2A Bridge](180-0-core-a2a-bridge.md) | active; исторический источник issue #158 |
| 184 | [Core-owned Execution-Trace Evaluation](184-0-core-execution-trace-evaluation.md) | active; trace contract/legacy digest есть, 184.2 capture не подключён |
| 185 | [Core-owned Behavioral Simulation Harness](185-0-core-behavioral-simulation-harness.md) | active; исторический источник issue #161; blocking dependency 184 |
| 186 | [Core-owned Local Typed-Decision Fast Path](186-0-core-local-typed-decision-fast-path.md) | active; исторический источник issue #162 |
| 187 | [Core-owned Windows Native Computer Use](187-0-core-windows-native-computer-use.md) | active; исторический источник issue #163; blocking dependencies 184–185 |
| 188 | [Core-owned Research Experiment Tree](188-0-core-research-experiment-tree.md) | active; исторический источник issue #164 |
| 191 | [Core-owned tool surface hardening и доставка skills агенту](191-0-core-tool-surface-and-skill-delivery.md) | active; выведен из аудита checkout, без исходного issue |
| 193 | [Agent Coordination, Loop Guards и Session Recall](193-0-core-agent-coordination-loop-guards-and-session-recall.md) | active; анализ DeepSeek Harness, audit checkout и trace evidence 2026-11-17 (лимит шагов маскировался как completed) |
| 194 | [Background Jobs Control Center](194-0-core-background-jobs-control-center.md) | active; UX/IPC поверх реализованного background-execution/v1 |
| 195 | [Capability Extensions и Workflow Composition](195-0-core-capability-extension-and-workflow-composition.md) | active; declarative extension поверх существующих owners |
| 199 | [Живой город-архипелаг реальной архитектуры Улья](199-0-hive-living-architecture-archipelago.md) | active; Hive-owned (`evohime-hive-daemon` + прямой Electron UI, без изменений в Core) |
| 200 | [Королева-маршрутизатор Улья и динамические агенты](200-0-hive-queen-router-and-dynamic-agents.md) | active; чистый оркестратор Королевы, ролевые агенты, динамические сектора и 3D-маршруты |

Незавершённые numbered plans: `180`, `184–188`, `191`, `193–195`, `199` и `200`.

Номера `149–172` и `182–183` являются закрытыми идентификаторами очереди.
Пропуск `144` намеренный: это закрытый план модульного обновления. Новая
работа получает следующий свободный номер только после проверки дубликатов и
зависимостей; следующий номер — `201`, текущий active catalog — `180`,
`184–188`, `191`, `193–195`, `199` и `200`. Номера issues в активных планах — исторические
идентификаторы постановок и не являются текущим источником статуса или
критерием закрытия.

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
- контракт переносится в доменный файл [`../architecture/`](../architecture/) (один домен — один файл; новый домен — новый файл), а [`../architecture.md`](../architecture.md) меняется лишь при изменении границ, процессов, инвариантов или карты;
- фактический статус — в [`../current-state.md`](../current-state.md);
- проверки — в [`../release-evidence.md`](../release-evidence.md);
- временные plan-файлы удаляются из этого каталога.

Старая ссылка на удалённый plan-файл не является доказательством незавершённой
работы: её нужно заменить ссылкой на канонический документ.

## Стыковка направлений
- `122` остаётся владельцем verification evidence для `124`, `135`, `136` и `140`;
- `134` предоставляет resource-pressure signals для runtime-направлений;
- `130` владеет lease/fencing, а `132` использует этот уже закрытый контракт;
- `131` и `137` не создают параллельные registries;
- `121`, `125`, `128` и `129` используют единый gateway/resolver;
- `138` не создаёт второй механизм обновлений и использует контракт `144`;
- `193` расширяет existing agent organization, TeamSession, child handoff,
  EventJournal и memory/context owners;
- `194` использует background-execution/v1 и не создаёт второй scheduler;
- `195` использует existing capability, skill, workflow и component updater
  owners; arbitrary executable plugins запрещены;
- `196` задаёт единый канонический лексикон tool id (`^[a-z][a-z0-9_]*$`,
  разделитель `_`); планы `191–195` и `197` используют только канонические id;
- `197` изолирует провайдера, модель, `PermissionMode` и опционально `git worktree`
  на уровне чата (`SessionBinding`) внутри одного Core, проходя через
  `ModelGateway` privacy gate (`181`), общий `task_worktree_isolation` (`188`),
  наследование для `child`/`TeamSession` (`193`) и проекцию `194`.

## Проверка
Перед реализацией overview и после каждого этапа сверяйте план с кодом,
`AGENTS.md`, [`../architecture.md`](../architecture.md), security policy и
release workflow. Минимальный gate: ссылки разрешаются, `git diff --check`
проходит, а критерии этапа подтверждены свежими тестами.
