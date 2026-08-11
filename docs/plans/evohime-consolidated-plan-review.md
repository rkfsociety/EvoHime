# Ревью плана EvoHime

Этот файл содержит накопленные замечания к [сводному плану EvoHime](evohime-consolidated-plan.md).

## Замечания

<!-- Новые ревью добавляются ниже по мере поступления. -->

## Ревью 1 — критические риски, проектные замечания и пересмотр этапов

### Критические риски

1. **Нет раннего пользовательского результата.** Этапы 0–2 — чистая инфраструктура (миграции, IPC, lifecycle, context assembler). Пользователь не получает ничего нового, пока не пройдены три самых тяжёлых этапа. Нет определения MVP и момента, когда можно собрать обратную связь.

2. **Этап 0 перегружен.** Миграции, protobuf, idempotency, replay, backup, rollback, durable run, cancellation, timeout, bounded output, supervisor recovery, typed envelope и compatibility fixtures — по сути весь фундамент платформы в одном этапе. Высокий риск, что этап станет бесконечным.

3. **Approval fatigue.** Одобрение требуется на запись, опасный tool, scope change, delivery, security research, запись в память и внешние мутации. Без пакетирования approvals или уровней доверия инструмент станет утомительным.

### Проектные замечания

4. **«Одна bounded-итерация» не определена численно.** Нужны конкретные лимиты шагов, токенов и вызовов, иначе невозможно написать тесты и определить момент остановки loop.

5. **Memory (этап 6) переусложнён для v1.** Append-only provenance, derived views, hybrid search, entity/temporal signals, TTL, privacy labels, export/delete и redaction — полноценная knowledge-base система. Для первой версии достаточно простого журнала решений с поиском.

6. **Research pipeline (этап 3) имеет широкую поверхность атаки.** HTTP-запросы, извлечение, цитирование, prompt-injection defense и freshness — каждый элемент сам по себе нетривиален. Не описано поведение offline-режима для задач, требующих research.

7. **Role/Skill contracts (этап 4) избыточны для примерно 10 встроенных ролей.** Versioned contracts, deterministic matcher, lifecycle snapshots, manifest/hash/signature, staged updates и rollback — инфраструктура публичного marketplace, а не встроенного каталога.

8. **WinUI 3 для сложного UI.** Граф зависимостей, workflow editor, timeline и catalog — нетривиальные компоненты. Не указано, какие контролы или библиотеки используются; WinUI 3 ограничен в готовых сложных компонентах.

### Отсутствует

9. **Производительность.** Нет целевых метрик для latency IPC, времени восстановления, лимита размера SQLite и поведения при 1000+ задачах.

10. **UX ошибок и восстановления.** Указано «corrupted state → diagnosable blocked state», но не описано, что пользователь видит и делает в каждом сценарии отказа.

11. **Стратегия миграции между этапами.** Схема SQLite будет меняться от этапа к этапу. Упомянут backup перед миграцией, но нет forward compatibility strategy.

12. **План отката инициативы.** Последовательность жёсткая. Если этап 3 или 4 затягивается, не определён fallback-путь.

13. **Onboarding.** Система сложная. Не описано, как новый пользователь понимает, что делать, и какие дефолты выставлены.

### Рекомендация

Разбить этап 0 на два:

- **0a** — минимальные миграции и восстановление после рестарта;
- **0b** — replay, idempotency и supervisor recovery.

Между этапами 1 и 2 встроить milestone: пользователь видит и редактирует граф задач вручную, без lifecycle. Это даст ранний результат и снизит риск.

## Ревью 2 — структурные зависимости, контракты и порядок этапов

Оценка: план сильный, консолидация 11 источников выполнена чисто, без дублирования доменов.

### Критичные и структурные замечания

1. **Этап 0 перегружен.** Миграции task graph, run checkpoints, memory provenance, workflow entities, protobuf-команды, idempotency/replay, durable run state, cancellation/timeout, supervisor recovery и typed envelope слишком велики для одного P0-этапа. Предложено явно разделить его на **0a** (storage, migrations, basic CRUD/events) и **0b** (checkpoint/resume, replay, recovery).

2. **Нет явного контракта для `next_ready` и priority.** Нужно определить tie-breaking, aging, workstream-веса и поведение при одновременной готовности нескольких веток, чтобы UI, runner и тесты использовали одинаковые правила.

3. **Lifecycle stages и work_item statuses.** Существуют две параллельные state-машины (`backlog/ready/in_progress/...` и `defined/planned/building/...`). Нужны явный mapping и единый source of truth, особенно для restart и child-handoff.

4. **Budget и stop conditions.** `max iterations`, wall-clock, tool/token budget должны появиться уже в этапе 0/2 как часть `run` и `run_policy`; иначе checkpoint будет неполным, а replay — недетерминированным.

### Важные, но не блокирующие замечания

5. **Research pipeline слишком рано относительно skills/roles.** Предложено либо поднять минимальный skill/role skeleton в этапы 2–3, либо до этапа 4 ограничить research ручным запуском пользователя.

6. **Memory v1 и confidence.** При extraction только по policy + user confirm не определено, кто принимает решение о confidence и когда факт становится текущим view.

7. **Child roles.** Следует явно запретить child создавать нового child и передавать elevated permissions через handoff.

8. **UI sequence.** Пользователь должен видеть выбранные role/skill/route уже на этапах 1–2, даже если полноценные role/skill catalog и research/memory inspector появятся позже.

### Мелкие и consistency-замечания

- Жёсткое правило текущей ветки `main` и task-only commit может конфликтовать с feature-branch + PR при появлении параллельных contributors; стоит оставить окно для `feature/stage-N`.
- Нужно тестово зафиксировать, что personality не влияет на `allowed_tools` и `approval_policy`.
- В readiness criteria упомянут `draft-plan` с non-goals, но non-goals отсутствуют в доменной модели; следует добавить поле или убрать критерий.
- Не описана concurrency-модель: два runner на одном workspace, два UI и supervisor restart во время approval. Нужны правила победителя и формат conflict.

### Сильные стороны

- Чёткое разделение ownership: Core — единственный владелец state/tools/permissions.
- Versioned named-pipe, request IDs и sequence replay.
- Отказ от скрытой магии памяти и graph DB на старте.
- Жёсткий quality gate: реализация считается готовой только при наличии теста и evidence.
- Сознательный список того, что не переносится, защищающий от scope creep.

### Рекомендация по порядку

Сохранить смысловую последовательность 0 → 1, но разрезать этап 0. После этапа 1 сразу сделать минимальный Plan/Build + context assembler как часть этапа 2, чтобы task workspace не оставался «мёртвым» UI. Research и skills сблизить (3+4), остальное оставить по плану.

## Ревью 3 — масштаб MVP, техническая сложность и эксплуатационные риски

### Общая оценка

План впечатляет глубиной проработки, строгостью архитектурных границ и вниманием к безопасности, отказоустойчивости и тестируемости. Консолидация 11 планов выполнена аккуратно; дублирования практически нет, а этапы выстроены в логичной последовательности: фундамент → задачи → жизненный цикл → исследования → навыки → цикл → память → эвалы → дочерние роли → расписания.

Особенно сильны:

- чёткое разделение ответственности между `Core`, `UI` и `Supervisor`;
- требования идемпотентности, replay и восстановления после краша;
- принцип «одна bounded-итерация за раз» и явные стоп-условия;
- запрет скрытого доступа UI к данным и секретам.

### 1. Масштаб и приоритизация этапов

План включает 10 этапов (0–9), каждый со своей проверочной базой, что может затянуть релиз MVP. Рекомендуется:

- явно выделить MVP: этапы 0–2 (фундамент, задачи, Plan/Build с ручным утверждением) плюс базовый UI;
- для этапов 3–9 указать, что можно отложить без потери базовой ценности; расписания и внешние каналы явно P2, но этап 8 с дочерними ролями также может подождать.

### 2. Техническая сложность отдельных этапов

- **Этап 3 — Research и workflow graph.** Typed workflow с условиями, ретраями и сабграфами эквивалентен мини-движку BPMN. Нужно заранее решить, будет ли граф статическим или динамическим; рекомендуется начать с фиксированного графа.
- **Этап 4 — Skills, roles и capability registry.** Проверка хеша/подписи требует криптографической инфраструктуры и доверенных корневых центров. На начальном этапе можно ограничиться SHA-256 по известному манифесту, добавив подписи позже.
- **Этап 6 — Память и RAG.** Гибридный поиск требует embedding-модели. Для offline-режима нужна лёгкая модель, но запуск и индексация могут быть ресурсоёмкими; следует предусмотреть отключение векторного поиска на слабом оборудовании.
- **Этап 9 — Расписания и внешние каналы.** Для GitHub, CI и других интеграций потребуются OAuth-потоки и хранение токенов. Нужно добавить протокол авторизации, включая возможный ручной браузерный вход.

### 3. Детализация разделов

- **Пользовательские сценарии.** Добавить несколько concrete user stories, например: разработчик добавляет фичу, Core генерирует план, запускает сборку, показывает diff и запрашивает утверждение.
- **UI-экраны.** Для каждого экрана описать ключевые элементы. Для графа задач — статусы, блокировки, редактирование зависимостей и критерии готовности UI, например просмотр всех `next_ready` задач и запуск одной из них.
- **Метрики и пороги.** Значения по умолчанию для `max iterations`, `wall-clock timeout` и `token budget` вынести в конфигурацию вроде `settings.toml` с возможностью переопределения через UI.
- **IPC.** Явно закрепить политику обратной совместимости: новые поля protobuf опциональны, старые клиенты игнорируют неизвестные поля; unknown-field compatibility fixtures уже создают основу для этого.

### 4. Риски и смягчение

- **Производительность SQLite.** Предусмотреть периодическую vacuum/архивацию старых записей, особенно `work_item_events` и `run_checkpoints`, и автоматическую очистку по политике, например хранение событий за последние 30 дней.
- **Конкуренция и блокировки.** Явно указать сериализацию операций с графом через единый mutex или очередь команд, чтобы избежать race-условий при одновременных изменениях.
- **Supervisor и дочерние процессы.** При падении во время записи checkpoint нужны SQLite WAL, транзакции и атомарный флаг `checkpoint valid` в отдельной таблице, чтобы восстановление не создавало дубликаты.
- **Подмена манифеста skills.** Целостность манифеста должна быть защищена; лучше подписывать манифест вместе с архивом отдельным файлом `.sig`, а не проверять только хеш.

### 5. Взаимосвязи этапов

- **Этап 2 и Research.** Plan/Build требует context assembler, использующий research и память, хотя research появляется на этапе 3. Предлагается в этапе 2 реализовать research-заглушку с пустым результатом, а полноценную интеграцию добавить на этапе 3.
- **Этап 5 и workflow.** Поскольку этап 3 предшествует этапу 5, порядок допустим. При необходимости раннего запуска loop можно использовать в этапе 5 упрощённый линейный исполнитель без графа, а полноценный workflow подключить после этапа 3.

### 6. Тестирование и качество

- Добавить нагрузочные тесты или бенчмарки: например, создание 10 000 задач и проверка скорости `next_ready`.
- Уточнить стратегию автоматизированного UI-тестирования для WinUI; WinAppDriver или аналог можно отложить, но решение следует зафиксировать.

### 7. Документация и обратная связь

- Добавить встроенную справку или onboarding/tutorial; например, первый запуск может предложить импортировать пример PRD.
- Для feedback `useful/not useful` указать локальное хранение, агрегацию и то, что feedback не влияет на поведение по умолчанию и не отправляется через внешнюю телеметрию.

### Итоговая рекомендация

- Уточнить приоритеты и выделить MVP (этапы 0–2).
- Описать сложные компоненты — workflow graph, RAG и установку skills — на уровне архитектурного дизайна до начала кодирования.
- Добавить конкретные пользовательские сценарии для каждого этапа.
- Предусмотреть очистку старых данных и метрики производительности SQLite.
- Чётко определить политику версионирования IPC и миграций БД.

Оценка ревью: план практически готов к реализации; замечания в основном уточняющие и предупреждающие.

## Ревью 4 — durable orchestration, crash/retry/replay invariants

### Общая оценка

План логично собран по цепочке «фундамент → task graph → lifecycle → research/workflow → skills → runner → memory → observability → child roles → schedules». Архитектурные границы не размыты: WinUI остаётся thin client, Core владеет логикой и SQLite, Supervisor отвечает за lifecycle/recovery.

Главный вывод ревью: план хорошо отвечает на вопрос «что строим и в каком порядке», но слабее фиксирует инварианты, которые не позволят durable orchestration ошибиться после crash/retry/replay. Это следует усилить до начала этапа 0.

### Критичные контракты

1. **Семантика checkpoint.** Нужно определить, создаётся ли checkpoint до или после внешнего side effect; сделать `checkpoint_id` монотонным внутри `run_id`; хранить `stage`, `node_id`, `attempt`, `input_hash`, `state`, `pending_effects`; считать checkpoint durable только после commit SQLite transaction. Внешней операции нужен отдельный `operation_id/idempotency_key`. После crash Core должен сначала проводить reconciliation незавершённых effects, а не вслепую продолжать с checkpoint.

2. **Отдельная модель side effects.** SQLite-транзакция не охватывает filesystem, shell, GitHub, HTTP и model provider. Нужна сущность `run_effect` с полями `effect_id`, `run_id`, `node_id`, `kind`, `idempotency_key`, `intent`, `state`, timestamps, `result_hash` и `reconciliation_state`. Состояния: `prepared → executing → succeeded | failed | unknown`. `unknown` после убийства процесса должен вести к reconciliation или approval, а не к retry по умолчанию.

3. **Две lifecycle-модели.** `work_item` и `run` следует явно объявить ортогональными state machines. Рекомендуется разделить `WorkItemStatus`, `RunStatus`, `LifecycleStage`, `StopReason` и `ApprovalState`. Например, `RunStatus`: `queued`, `running`, `paused`, `waiting_approval`, `completed`, `failed`, `cancelled`; `LifecycleStage`: `define`, `plan`, `build`, `verify`, `review`, `ship`. `building` не должен быть статусом run.

4. **Lease/ownership runner.** Для races двух runners нужен контракт `run_owner`, `lease_id`, `lease_expires_at`, `heartbeat_at`, `generation` и атомарный переход `READY → CLAIMED → RUNNING`, чтобы после restart различать умерший runner и временно медленный.

5. **Детерминированный `next_ready`.** Нужен стабильный tie-break, например `priority DESC`, dependency readiness, `created_at ASC`, `work_item_id ASC`, а также сохранение `selection_reason`. Это требуется для replay, тестов, одинакового поведения UI/Core и объяснения выбора пользователю.

6. **Approval как доменная сущность.** Нужен `ApprovalRequest` с `approval_id`, `run_id`, `effect_id`, `requested_action`, `risk_class`, `scope`, `reason`, `preview`, timestamps, `decision`, `decided_at`, `decided_by` и hash одобренного intent. Approval должен разрешать конкретный immutable intent; иначе diff может измениться после нажатия Approve.

7. **Границы snapshot/diff/rollback.** Явно определить покрытие snapshot: workspace-файлы, Git state, untracked files, SQLite, generated artifacts и внешние effects. Зафиксировать: workspace rollback не означает rollback внешних side effects. Snapshot не должен зависеть от чистого Git tree; подойдут workspace journal/CAS или Git-compatible content snapshot.

8. **Пакетирование approval для UX.** Read-only выполняется без approval; writes внутри заранее одобренного bounded scope — пакетно; approval требуется перед первым mutation set, при scope drift, sensitive file, unexpected diff и перед delivery/commit. Это сохраняет безопасность без серии одинаковых диалогов.

### Контракты, которые стоит поднять раньше

9. **Role/Skill refs.** Полный registry можно оставить этапу 4, но `RoleRef`, `SkillRef`, `PolicySnapshot`, `ModelRouteSnapshot` нужно ввести уже в этапе 0 в минимальном/stubbed виде, чтобы не мигрировать позднее schema run/checkpoint.

10. **Model routing.** С этапа 0 сохранять `requested_route`, `resolved_provider`, `resolved_model`, `route_policy_version` и `fallback_chain`, даже если пока есть один provider.

11. **Версия policy.** Для каждого run нужен immutable `policy_snapshot_id`, `policy_version` и `effective_permissions_hash`; тот же snapshot-подход следует использовать для role, skill и capability definitions.

12. **Ранний provenance.** На этапе 0 заложить storage для `provenance/events/decisions`; на этапе 6 оставить extraction, retrieval, ranking, lexical/vector search и memory UX.

### IPC, события и scope

13. **Research и обычный HTTP tool.** Research не должен получать обходной privileged network access. Все fetch/search должны проходить через общий capability/policy/effect layer с allowlist, audit, cancellation и budget.

14. **IPC evolution.** Добавить правила: enum поддерживает `UNKNOWN`; reader tolerates unknown fields; новая semantics требует negotiated capability; breaking semantic changes получают новую message/command version; handshake сообщает `protocol_version + capabilities`; replay buffer привязан к `core_instance_id/session_epoch`.

15. **Event ordering.** Для reducer-state UI определить `core_instance_id`, `event_sequence`, `aggregate_version`, `event_id`. Дубликат игнорируется, gap вызывает replay/resync, stale update игнорируется, потеря replay window приводит к full snapshot.

16. **Scope drift.** Формализовать scope через `allowed_paths`, `allowed_operations`, `expected_outputs`, `protected_paths`, `max_files_changed` и `acceptance_criteria`.

17. **Evidence как сущность.** Добавить `Evidence` с `evidence_id`, `run_id`, `work_item_id`, `kind`, `source`, optional `command`, `exit_code`, `artifact_hash`, `summary`, `captured_at`. Типы: `test_result`, `diff`, `build`, `lint`, `screenshot`, `citation`, `manual_review`.

18. **Task-only commit.** Вынести правило task-only commit из продуктовых и security invariants в отдельный раздел `Development execution rules`; это правило разработки EvoHime, а не архитектурный инвариант продукта.

### Семь точечных изменений

1. Разделить `4.2` на `RunStatus`, `LifecycleStage`, `StopReason`, `ApprovalState`.
2. Добавить `RunEffect` и crash-reconciliation.
3. Добавить runner lease/ownership.
4. Описать event ordering/replay/resync.
5. Сделать `ApprovalRequest` immutable и привязанным к hash intent/diff.
6. Добавить в этап 0 lightweight snapshots role/skill/policy/model route.
7. Добавить структурированный `Evidence`.

Дополнительные положительные стороны: schedules и внешние каналы правильно оставлены после стабилизации локального контура, а UI не получает прямых полномочий над workspace/SQLite.

## Ревью 5 — технические примеры, UX и документация

### Общая оценка

План чётко описывает цели, границы, доменную модель, этапы поставки и критерии готовности. Объединение 11 исходных планов логично и без дублирования, приоритизация P0 → P2 и зависимости между этапами проработаны.

### Замечания и предложения

1. **Ясность и детализация.** В разделе 4.1 нужны примеры SQLite-таблиц, например `work_items` и `run_checkpoints`. В разделе 5 полезна Mermaid-диаграмма зависимостей между этапами.

2. **Безопасность и отказоустойчивость.** Для `risk_class` и `allowlist` нужны конкретные правила вычисления и применения, желательно таблица рисков и ограничений. Для Supervisor recovery следует описать конфликты при восстановлении, включая ситуацию, когда два процесса пытаются восстановить один checkpoint.

3. **Технические примеры.** Для IPC нужны примеры protobuf-команд `TaskCRUD` и `CheckpointResume`. Для workflow graph — JSON/YAML-пример node с typed inputs/outputs, retry и approval. Для Memory/RAG — решение по embedding-моделям и conflict resolution между старыми и новыми фактами.

4. **UX.** Нужно описать минимальный набор экранов MVP и доступные зоны `three-zone frame` на этапе 1, а также дать mockup или текстовое описание task detail/graph и run timeline. Для состояний `Degraded`, `Error`, `Blocked` нужны конкретные пользовательские сообщения, например сообщение при недостатке permissions.

5. **Тестирование.** В раздел 8 добавить шаблон тест-кейсов с колонками «Сценарий», «Ожидаемый результат», «Инструмент проверки» и примерами IPC reconnect и migration rollback. Уточнить automated regression testing и CI/CD pipeline для этапов.

6. **Документация и поддержка.** Добавить глоссарий (`bounded loop`, `provenance`, `capability registry`), FAQ по типичным проблемам вроде разрыва IPC и ссылки на внешнюю документацию WinUI 3, SQLite и protobuf.

7. **Риски и митигации.** Добавить отдельный risk register с вероятностью, влиянием и митигацией. Примеры: высокая вероятность критической задержки этапа 0 и средняя вероятность высокой по влиянию несовместимости IPC Rust/C#.

8. **Форматирование.** Разбить длинные предложения в разделе 2 и добавить якоря для ссылок между разделами, например `#граф-задач`.

### Итоговая рекомендация

Добавить технические примеры (protobuf, SQL, workflow node), конкретизировать security rules (`risk_class`, `allowlist`), описать MVP UX и сообщения об ошибках, включить тестовые шаблоны и глоссарий.

## Ревью 6 — проверяемость, детерминизм и IPC/lifecycle-контракты

### Общая оценка

Документ очень силён: структура, границы, этапы, критерии готовности и non-goals заданы правильно. Основные уточнения нужны для проверяемости, детерминизма, IPC-контрактов, миграций, lifecycle-изоляции и гарантии отсутствия скрытой магии в Core.

### Ключевые замечания

1. **IPC v1 недостаточно формализован.** Нужен отдельный mini-spec с форматом unknown-field compatibility, правилами sequencing при одновременных UI → Core и Core → UI событиях, поведением при partial replay и различением duplicate request и retry.

2. **Task graph и atomicity.** Следует определить уровень изоляции SQLite, правила гонки между IPC-клиентами и стратегию конфликтов обновления `work_item` — optimistic lock или last-write-wins. Иначе тесты races двух runners будут неполными.

3. **Lifecycle mutation matrix.** Для DEFINE → PLAN → BUILD → VERIFY → REVIEW → SHIP нужно описать допустимые изменения полей `work_item`/`run` на каждой стадии и формально определить scope drift: изменение description, acceptance criteria, complexity и т.п.

4. **Context assembler.** Нужен контракт redaction, детерминированный порядок элементов контекста и правила разрешения конфликта между memory и research с учётом свежести источника.

5. **Research evidence.** Формализовать формат excerpt (plain/redacted, максимальный размер), freshness (timestamp, TTL или policy) и доказательство того, что research не попал в prompt без approval.

6. **Skills/roles.** Нужен deterministic resolution order для конфликтов, например когда skill требует запрещённый policy tool или обновился после фиксации run snapshot.

7. **Memory provenance.** Определить события, создающие provenance (tool call, diff, approval, research), различие ephemeral/durable фактов и формат confidence: число, enum или вычисляемое значение.

8. **Bounded loop.** Ввести структурированный stop reason (`enum + details`), правила отображения разных причин в UI и критерий, по которому Core определяет неясность acceptance criteria.

9. **Supervisor health protocol.** Описать health-ping, различение зависшего Core и долгой операции, а также передачу degraded state в UI.

### Дополнительные уточнения по этапам

- В этапе 1 decomposition не должен менять исходный PRD-текст.
- В этапе 2 snapshot/diff должен быть явно ограничен workspace, а не SQLite.
- В этапе 4 skill может только сузить permissions, но не расширить их.
- В этапе 5 fallback должен быть видимым до выполнения, а не постфактум.
- В этапе 6 memory export/delete требует approval.
- В этапе 7 hooks не могут менять порядок элементов context.
- В этапе 8 child tasks должны иметь фиксированный максимальный размер output.
- В этапе 9 нужна dead-letter policy с числом попыток и backoff.

### Итог

До начала этапа 0 рекомендуется формализовать семь минимальных контрактов: IPC v1 spec, lifecycle mutation matrix, context assembler contract, research evidence format, memory provenance format, deterministic skill resolution и supervisor health protocol.
