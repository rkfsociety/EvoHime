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
  `parent_task_id` должны быть непустыми; `child_task_id` — глобально
  уникальный UUIDv4, проверяемый Core и SQLite unique constraint, и не
  переиспользуемый после terminal state.
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
- выдача урезанного context и глобально уникального UUIDv4 `child_task_id` из Core, без передачи
  полного parent prompt, секретов или неразрешённых capabilities;
- read-only execution adapters для `workspace.read`, `workspace.search`,
  `git.diff` и `git.status` с общей policy snapshot;
- MVP выполняет child как logical bounded job внутри Core, а не как отдельный
  контейнерный процесс. Sandbox обеспечивается Core capability-router-ом:
  adapter получает только проверенный workspace root, read-only operation
  object и cancellation/limit handles; прямой OS shell/process spawn,
  arbitrary path access и неразрешённые IPC commands недоступны. AppContainer,
  отдельный worker process или виртуализация не являются скрытой частью MVP;
  если threat model потребует process isolation, это отдельное ADR и scope.
- запрет write, shell, commit, install, network mutation, elevation и nested
  child проверяется на request, dispatcher, adapter и tool layers;
- filesystem boundary нормализует absolute path, запрещает traversal и
  symlink/reparse-point escape, и проверяет принадлежность итогового пути
  разрешённому workspace root перед каждой операцией;
- network policy MVP — `deny all`: у текущего child runtime нет network
  capability. Read-only HTTP, если он понадобится позже, вводится отдельным
  capability/profile с host/port/redirect/private-range/credential policy и
  не считается разрешённым по умолчанию;
- единые timeout/cancellation/output limits и composite budget;
- передача лимитов в adapter идёт через immutable `ChildPolicySnapshot` и
  cancellation token; output пишется в bounded counting sink, который
  останавливает producer до записи байта сверх лимита, а timeout watchdog
  отменяет token и дожидается фактической остановки операции;
- проверка родителем report, confidence и sources до включения evidence в
  plan/build. Непринятый report не становится evidence и не меняет parent
  state;
- durable child lifecycle и redacted provenance: request, state transitions,
  terminal reason, report hash, sources и acceptance decision;
- versioned IPC events/replay/reconnect для child timeline;
- WinUI catalog, descriptor editor, timeline, evidence panel и
  blocked/error states.

## Схемы входа и отчёта

`ChildTaskInput` — versioned Core-owned envelope с обязательными полями
`schema_version`, глобальным UUIDv4 `child_task_id`, `parent_task_id`,
разрешённым `kind`, bounded `role`, `reduced_context[]`,
`requested_capabilities[]`, `max_output_bytes`, policy snapshot id и
`parent_is_child`. `started_at`/`finished_at`, event identity и provenance не
приходят от UI как доверенные значения: Core заполняет или проверяет их сам.
Неизвестные capability/kind, дополнительные executable payload types и
`parent_is_child=true` отклоняются до dispatch.

`ChildTaskReport` — versioned envelope с обязательными `child_task_id`,
`kind`, `status`, `summary`, `findings[]`, `sources[]`,
`confidence_percent`; bounded optional `limitations[]`, `output_bytes`,
`started_at`, `finished_at`, `event_id` и provenance hash. Все collection
элементы и serialized envelope имеют собственные limits, перечисленные в
contract constants; отсутствие обязательного поля, неизвестный enum или
несовпадение request hash переводят report в validation rejection.

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
- adapter panic, forced producer crash или потеря worker task даёт `failed` с
  machine-readable reason; в MVP это bounded Core task, поэтому отдельный
  child OS process не обещается и не должен быть имитирован в UI;
- если parent acceptance gate не может завершить проверку из-за внутренней
  ошибки Core/storage, report не принимается и не отклоняется как содержательно
  неверный: child получает `blocked` с redacted reason, ошибка пишется в audit,
  а parent получает доступное действие для ручного retry новым UUID;
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

Допустимые source forms в MVP: workspace file reference
`workspace:<normalized-relative-path>#L<start>-L<end>`, git reference
`git:<commit-or-working-tree>:<normalized-relative-path>#L<start>-L<end>` и
внутренний event reference `event:<event_id>`. URL не разрешены при `deny all`
network policy. Core проверяет, что file/git/event reference существует в
разрешённом scope и был доступен через child operation; произвольная строка
или непроверенный внешний URL не считается provenance.

Parent rejection переводит child в immutable `rejected` и не меняет parent
task автоматически. Parent может запросить ручную проверку, создать новый
child с новым UUID и изменённым descriptor либо завершить ветку как
`blocked`. Автоматический retry того же request/report запрещён.

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

Event journal хранится в существующем Core-owned SQLite event journal и
связанном child lifecycle storage; UI не держит его единственную копию. В MVP
timeline хранится по общей retention policy завершённых task, а active,
blocked и rejected child не удаляется до завершения parent task. Если retention
очищает старое событие, Core возвращает snapshot с `replay_floor_sequence`, а
UI показывает `history truncated`, не придумывая пропущенные переходы.

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
- WinUI получает Core events через существующий versioned named-pipe protobuf
  transport. ViewModel использует reducer/state projection и
  `ObservableCollection`/property notifications только как представление;
  Core остаётся владельцем lifecycle, configuration persistence, policy и
  acceptance. Отдельной шины, gRPC или прямого доступа UI к SQLite нет.
- Для blocked/error UI использует Core enum, machine-readable reason, safe
  detail и доступные actions. Локальные переходы допускаются только для
  loading/reconnect decoration и не меняют child state.

## Наблюдаемость и диагностика

Core пишет redacted structured events для `created`, `started`,
`cancel_requested`, `cancelled`, `timed_out`, `budget_exceeded`,
`output_exceeded`, `failed`, `validated`, `accepted`, `rejected` и `aborted`.
Каждая запись содержит `child_task_id`, parent id, kind, policy snapshot id,
duration, tool-call count, output bytes, terminal reason и report/provenance
hash без prompt, secret-like content или полного report.

Публичные метрики MVP: duration, output bytes, tool calls,
cancellation/timeout count, failure/blocked/rejection count и replay gap count.
Метрики агрегируются по kind/terminal reason; raw findings и secret-like
payload в metrics не попадают.

## Порядок реализации

1. Зафиксировать Core-owned child policy snapshot, allow-list и lifecycle
   state machine поверх существующих contracts/storage.
2. Зафиксировать `ChildTaskInput`/`ChildTaskReport` schema, UUID identity,
   source forms, policy/error enums и IPC compatibility fixtures.
3. Ввести Core dispatcher и read-only execution adapters с единым
   timeout/cancellation/output/tool budget и sandbox negative tests.
4. Реализовать parent acceptance gate, source validation, confidence metadata и
   evidence provenance.
5. Добавить durable child events, IPC compatibility, replay/reconnect и
   terminal-state conflict rules.
6. Добавить native catalog/descriptor editor, timeline/evidence views,
   blocked/error states и visual smoke.
7. Провести focused unit tests каждого adapter и acceptance gate, integration
   tests с fake child/producer и forced crash, sandbox bypass tests,
   cancellation/timeout/budget tests, replay/reconnect tests и WinUI smoke.

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
- parent принимает только валидный report с корректными schema/provenance,
  непустым summary, уникальными валидными sources и matching provenance;
- confidence сохраняется в `0..100` как metadata и не может самостоятельно
  перевести report в accepted или расширить permissions;
- UI одинаково и truthful показывает created, queued, running, validating,
  waiting parent acceptance, accepted/rejected, blocked, failed, cancelled,
  timed out, budget exceeded, output exceeded и aborted; `waiting approval`
  отображается как parent workflow overlay;
- replay после reconnect не дублирует child events и не воскрешает terminal
  child;
- child crash даёт `failed` с machine-readable reason, внутренний gate failure
  даёт `blocked` с redacted detail, а sandbox negative tests подтверждают
  denial для write/shell/nested child/network mutation/traversal/symlink escape;
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
