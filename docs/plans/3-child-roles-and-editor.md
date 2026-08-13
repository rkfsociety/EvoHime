# Подплан 3 — child roles и native workflow editor

Статус: средняя сложность
Порядок: 3 из 5
Источник: бывший единый мастер-план; актуальная детализация находится в этом подплане.

## Цель

Подключить существующие bounded child-role/handoff contracts к реальному
read-only выполнению и сделать его наблюдаемым в WinUI. Контрактный слой уже
умеет валидировать handoff, урезанный context, read-only capabilities и report,
а storage/IPC уже принимают и сохраняют описатели заявок и отчёты. Этот
подплан добавляет отсутствующий runtime dispatcher, выполнение и UI; он не
считает сохранённую заявку выполненной задачей.

## Фактическая база и границы

- `child_roles` уже содержит роли `coordinator`, `researcher`, `planner`,
  `implementer`, `reviewer`, `tester` и bounded `custom`, а также immutable
  redacted handoff contracts.
- `child_runtime` уже содержит `ChildTaskKind`: `onboarding`, `code_search`,
  `threat_model_review`, `test_plan_review`, `documentation`. В MVP этот
  allow-list закрыт. `research`, `validation`, `security_audit` и `code_review`
  не добавляются автоматически: каждый новый kind требует отдельного
  capability/profile, adapter и тестов.
- Разрешённые capabilities текущего контракта: `workspace.read`,
  `workspace.search`, `git.diff`, `git.status`. Write, shell, commit, install,
  network mutation, elevation и nested child запрещены на Core boundary.
- Текущие hard limits контракта: reduced context — до 32 элементов и 16 KiB,
  один элемент — до 2 048 символов; serialized report/output — до 32 KiB,
  report sources — до 32 элементов по 512 символов. `child_task_id` и
  `parent_task_id` должны быть уникальными в рамках родительского запуска и
  не переиспользоваться после terminal state.
- Реальный execution, timeout, cancellation, tool-count budget, dispatcher и
  child event stream пока отсутствуют. Сохранение `ChildTaskRequest` или
  `ChildReport` само по себе не является execution.
- После перезапуска `evohime-core.exe` child execution в MVP не
  восстанавливается: незавершённый child получает immutable terminal state
  `aborted` с причиной `core_restart`. Durable request/report/event journal
  сохраняются для диагностики и replay, но продолжение требует нового child с
  новым id.

## Объём

- Core dispatcher только для текущего `ChildTaskKind` allow-list и только из
  non-child parent context;
- выдача урезанного context и уникального `child_task_id` из Core, без передачи
  полного parent prompt, секретов или неразрешённых capabilities;
- read-only execution adapters для `workspace.read`, `workspace.search`,
  `git.diff` и `git.status` с общей policy snapshot;
- отдельные filesystem/network sandbox и запрет write, shell, commit, install,
  network mutation, elevation и nested child на request, adapter и tool layers;
- network policy MVP — `deny all`: у текущего child runtime нет network
  capability. Read-only HTTP, если он понадобится позже, вводится отдельным
  capability/profile с host/port/redirect/private-range/credential policy и
  не считается разрешённым по умолчанию;
- единые timeout/cancellation/output limits и composite budget;
- проверка родителем report, confidence и sources до включения evidence в
  plan/build. Непринятый report не становится evidence и не меняет parent
  state;
- durable child lifecycle и redacted provenance: request, state transitions,
  terminal reason, report hash, sources и acceptance decision;
- versioned IPC events/replay/reconnect для child timeline;
- WinUI catalog, descriptor editor, timeline, evidence panel и
  blocked/error states.

## Policy defaults для MVP

Значения должны попасть в Core-owned immutable policy snapshot, чтобы UI не мог
ослабить их после запуска. Это стартовые defaults, а не обещание навсегда:

- timeout: 5 минут на child; hard maximum — 15 минут;
- `max_output_bytes`: default 16 KiB, hard maximum текущего контракта — 32 KiB;
- reduced context: hard maximum текущего контракта — 16 KiB;
- tool budget: максимум 32 read-only tool calls на child; каждая операция
  дополнительно ограничивается своим filesystem/search/git лимитом;
- composite budget считается по wall-clock, числу tool calls и output bytes;
  превышение любого компонента немедленно переводит child в terminal
  `budget_exceeded`. Model-token budget учитывается только если provider
  возвращает измерение; его отсутствие не снимает wall-clock/tool/output
  limits;
- report confidence хранится как integer `0..100` и означает bounded
  self-assessment child, а не доказанную достоверность и не security signal.
  Он показывается родителю и может использоваться для сортировки/запроса
  дополнительной проверки, но сам по себе не разрешает и не запрещает
  acceptance. Порог `70` может быть policy hint для `needs_review`, но не
  автоматическим gate.

## Child lifecycle и state machine

Core является единственным владельцем этой state machine. Нормальный путь:

`created → queued → running → validating → waiting_parent_acceptance → accepted`

Из `waiting_parent_acceptance` report переходит в `accepted` или `rejected`.
Отдельные terminal states: `cancelled`, `timed_out`, `blocked`, `failed`,
`budget_exceeded`, `output_exceeded`, `aborted`. `accepted`, `rejected` и все
эти terminal states immutable.

- `waiting_approval` не является child lifecycle state: это состояние parent
  workflow/approval overlay. Если approval нужен до запуска, child остаётся
  `queued`; после запуска approval может приостановить parent без подмены
  child state.
- `completed` — это значение `ChildReport.status`, а не финальное состояние
  child до parent acceptance. После валидного report child находится в
  `waiting_parent_acceptance`; только gate переводит его в `accepted` или
  `rejected`.
- Cancel и terminal completion используют атомарный compare-and-set по
  текущему state. Побеждает первая зафиксированная terminal transition;
  поздний report/cancel получает idempotent result и не изменяет состояние.
- `validating` охватывает schema, provenance, bounds и policy validation; до
  его завершения report не виден как evidence.

## Lifecycle и ошибки

- превышение output останавливает adapter, не обрезает молча report и не
  публикует частичный evidence;
- timeout отменяет активную операцию и фиксирует `timed_out`; поздний report
  отклоняется;
- cancellation идемпотентна: повторная отмена не меняет terminal state;
- forbidden capability, nested child, invalid descriptor или invalid report
  дают `blocked`/`rejected` с machine-readable reason и redacted detail;
- runtime/provider/filesystem failure даёт `failed`, без blind retry. Retry
  возможен только как новый child с новым id и новым policy snapshot;
- parent acceptance gate атомарно проверяет request/report pair, task id,
  status, bounds, confidence, sources и provenance до записи acceptance.

## ChildReport и parent acceptance contract

Базовые поля существующего `ChildReport` сохраняются: `child_task_id`,
`status`, `summary`, `findings[]`, `sources[]` и `confidence_percent`. Runtime
envelope дополняет их полями `schema_version`, разрешённым `kind`,
`limitations[]`, `output_bytes`, `started_at`, `finished_at`, provenance hash и
`event_id`. Если эти поля ещё не входят в IPC message, добавление выполняется
совместимо с major/minor правилами IPC и покрывается compatibility tests.

До acceptance Core проверяет:

- schema version и закрытые enum для `kind`/`status`, без неизвестных опасных
  payload types;
- совпадение `child_task_id`, parent task, kind и immutable request hash;
- все collection/item/serialized size limits, включая отдельные bounds для
  findings, sources и limitations;
- `confidence_percent` строго в `0..100`;
- непустой summary, уникальные bounded sources, допустимый source format и
  provenance, который указывает на реально прочитанные Core операции;
- отсутствие secret-like content, mutation result или неразрешённого
  capability claim;
- `output_bytes` в пределах фактического output budget и корректный порядок
  `started_at <= finished_at`.

Confidence не заменяет source validation, acceptance review или approval. При
неполном, но валидном report родитель может принять его как `partial`; при
ошибке schema/provenance/bounds acceptance отклоняется независимо от confidence.

## Replay и reconnect

Каждое child event содержит `event_id`, `child_task_id`, child-local
monotonic `child_sequence`, глобальный Core `sequence_id`, `event_type`,
timestamp и bounded redacted payload. При reconnect UI передаёт последний
полученный Core `sequence_id`; Core возвращает snapshot child lifecycle и
последующий journal replay. Snapshot закрывает старую историю, а replay
достраивает её после snapshot sequence.

Transport может доставить event повторно. Consumer обязан быть idempotent по
`event_id` (и проверять child sequence); exactly-once delivery не обещается.
Core обнаруживает gap в child sequence и запрашивает/возвращает snapshot
вместо попытки тихо продолжить неполную timeline. Terminal state хранится
durable и является конечным: поздний report/cancel не воскрешает child и не
создаёт второй timeline item.

## Workflow editor и UI MVP

- Catalog показывает только разрешённые kinds, их read-only capabilities,
  limits, expected report shape и доступные роли.
- Descriptor editor — форма, а не произвольный script runner: пользователь
  может добавить child из catalog, удалить ещё не запущенный descriptor,
  изменить kind, built-in role, reduced-context items и порядок отображения,
  затем сохранить/загрузить workflow configuration и запустить child через
  immutable preview. Уже запущенные descriptors и runtime events только для
  чтения.
- Editor показывает локальную схему `parent → child`; drag-and-drop и
  свободный граф не обязательны для MVP. Валидация до запуска проверяет kind,
  role, context bounds, duplicate ids, dependencies и policy compatibility.
  Arbitrary dependency graph и reorder semantics переносятся в task graph
  contract подплана 4.
- Workflow configuration и runtime state — разные модели и persistence:
  configuration содержит descriptors и пользовательский порядок, runtime
  state содержит Core-owned lifecycle/events/reports. Конфигурация UI/IPC не
  может добавить capability вне Core child policy и не может изменить policy
  snapshot уже запущенного child.
- Timeline — линейный список переходов с timestamp, sequence, duration,
  terminal reason и reconnect-safe status; graph view не требуется для
  первого vertical slice.
- Evidence panel показывает summary/findings/sources, confidence, policy
  threshold, provenance и решение parent gate. Непринятое evidence визуально
  отделено от принятого и не попадает в plan/build context.
- Для `waiting_approval`, `blocked`, `failed`, `cancelled`, `timed_out`,
  `budget_exceeded` и `output_exceeded` UI показывает понятную причину и
  доступное действие; UI не подменяет Core state локальным успехом.

## Порядок реализации

1. Зафиксировать Core-owned child policy snapshot, allow-list и lifecycle
   state machine поверх существующих contracts/storage.
2. Ввести Core dispatcher и read-only execution adapters с единым
   timeout/cancellation/output/tool budget.
3. Реализовать parent acceptance gate, source validation, confidence policy и
   evidence provenance.
4. Добавить durable child events, IPC compatibility, replay/reconnect и
   terminal-state conflict rules.
5. Добавить native catalog/descriptor editor, timeline/evidence views,
   blocked/error states и visual smoke.
6. Провести focused contract, adapter, cancellation/timeout/budget,
   replay/reconnect, acceptance-gate и WinUI smoke tests.

## Критерии готовности

- child не может получить elevated permissions, создать child или выполнить
  mutation tool; запрет проверяется на Core boundary и повторно перед adapter;
- UI, IPC и workflow configuration не могут выдать capability, отсутствующую
  в Core child runtime policy;
- все текущие пять `ChildTaskKind` либо реально исполняются read-only
  adapter-ом, либо явно показываются как `not_implemented`, но не создают
  ложный `completed`;
- timeout, cancel, forbidden capability, invalid report, oversized output и
  любой budget limit приводят к bounded terminal state с причиной;
- parent принимает только валидный report с policy threshold confidence,
  непустым summary, уникальными валидными sources и matching provenance;
- UI одинаково и truthful показывает created, queued, running, validating,
  waiting parent acceptance, accepted/rejected, blocked, failed, cancelled,
  timed out, budget exceeded, output exceeded и aborted; `waiting approval`
  отображается как parent workflow overlay;
- replay после reconnect не дублирует child events и не воскрешает terminal
  child;
- focused Rust tests, IPC compatibility tests, WinUI smoke и `git diff --check`
  проходят; generated artifacts очищены.

## Зависимости

- `child_roles`, `child_runtime`, child storage и базовые IPC commands уже
  существуют как контрактный/persistence слой, но их execution wiring ещё не
  готов — это основной объём данного подплана;
- task lifecycle, event journal, cancellation, replay/reconnect и bounded
  checkpoint/recovery foundation уже существуют в Core, однако child-specific
  lifecycle/events/lease integration ещё нужно добавить;
- подплан 4 не является обязательной зависимостью для security boundary или
  focused testability. Его `RunPolicy`/runner можно переиспользовать позже,
  но MVP подплана 3 должен иметь самостоятельный Core-owned child policy,
  deterministic adapters и тестовый harness;
- полноценные pause/resume, arbitrary dependency graph и provider/model
  routing остаются в подплане 4 и не должны быть скрыты внутри child editor.
