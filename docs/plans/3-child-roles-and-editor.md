# Подплан 3 — child roles и native workflow editor

Статус: высокая сложность; два связанных трека реализации
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
  новым id. UUIDv4 генерирует Core при создании request; SQLite unique
  constraint и проверка при старте Core гарантируют отсутствие повторного id
  после рестарта или восстановления базы.

## Объём

Работа разделяется на два трека с независимыми gate-ами:

- **Track A — Core child runtime/security/acceptance:** policy preflight,
  dispatcher, sandbox boundary, adapters, lifecycle, report gate, provenance,
  durable events и replay. Этот трек обязателен и должен быть завершён до
  запуска production child.
- **Track B — WinUI catalog/editor/inspector:** Core-driven catalog, minimal
  policy-safe descriptor form, approval surface, timeline, evidence и error
  states. UI не расширяет Track A и может быть смонтирован на MockChildAdapter
  до подключения реальных adapters.

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
- ограничение действует на двух runtime-уровнях: schema filtering вырезает из
  child system context/tool schema все mutation tools и передаёт только
  allow-list read-only capabilities; затем Core/adapter runtime повторно
  проверяет capability перед каждой операцией. UI, prompt и model output не
  считаются security boundary;
- запрет write, shell, commit, install, network mutation, elevation и nested
  child проверяется на request, dispatcher, adapter и tool layers. OS-level
  syscall/process sandbox не является частью текущего logical-child MVP;
  добавление AppContainer/worker process требует отдельного ADR, threat-model
  review и compatibility decision;
- filesystem boundary нормализует absolute path, запрещает traversal и
  symlink/reparse-point escape, и проверяет принадлежность итогового пути
  разрешённому workspace root перед каждой операцией;
- policy preflight до запуска проверяет non-empty allow-list workspace roots и
  normalized path scopes. Logical-child MVP не создаёт OS mounts; adapter
  получает read-only path capability object, а любая запись/rename/delete,
  traversal, symlink/reparse escape или path вне allow-list отклоняется.
  Environment/credential access запрещён: child не получает process environment,
  Credential Manager/DPAPI handles, inherited secrets или provider tokens;
  context и adapter output проходят secret/PII redaction до journal/logging;
- reduced context строится Core из явного scope descriptor: для
  `code_search` — только найденные совпадения и запрошенные диапазоны строк,
  для `workspace.read` — только перечисленные normalized paths/ranges, для
  `git.diff/status` — только выбранный diff/status scope. Полный prompt,
  соседние файлы, secrets и неуказанные project data не передаются; secret
  redaction выполняется до сериализации context.
- Core сохраняет только redacted context manifest: scope descriptor hash,
  перечисленные path/range ids, item count/bytes, redaction count, policy
  snapshot id и timestamp. Полный context и secrets в audit не пишутся.
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

Короткий индекс контрактов для реализации: `ChildTaskKind` — закрытый enum
`onboarding | code_search | threat_model_review | test_plan_review |
documentation`; неизвестный kind отклоняется на protobuf/Core decode и ещё раз
в dispatcher, до создания adapter job. `ChildTaskInput` и `ChildTaskReport` —
versioned JSON внутри Core domain, protobuf envelope на UI/Core IPC; полная
схема и bounds должны быть закреплены compatibility fixtures до runtime
implementation. Child events используют `event_id`, `child_sequence`,
глобальный Core `sequence_id`, `event_type`, timestamp и bounded redacted
payload. Lifecycle contract описан ниже; UI не является его владельцем.

## Policy defaults для MVP

Значения должны попасть в Core-owned immutable policy snapshot, чтобы UI не мог
ослабить их после запуска. Это стартовые defaults, а не обещание навсегда:

- timeout: 5 минут на child; hard maximum — 15 минут;
- max wall-clock parent budget: 15 минут на child и 30 минут на parent child
  batch; child не может продлить budget через report или новый descriptor;
- `max_output_bytes`: default 16 KiB, hard maximum текущего контракта — 32 KiB;
- reduced context: hard maximum текущего контракта — 16 KiB;
- tool budget: максимум 32 read-only tool calls на child; каждая операция
  дополнительно ограничивается своим filesystem/search/git лимитом;
- max concurrent children: 2 на parent и 4 на Core process; max automatic
  retries: 0 для того же request/report и не более 1 нового child retry по
  explicit parent/human decision. Если OS-level memory/CPU quotas недоступны
  для logical-child MVP, Core обязан ограничивать concurrency/output/time и
  фиксировать это как non-guarantee, а не выдавать ложный resource isolation;
- composite budget считается по wall-clock, числу tool calls и output bytes;
  превышение любого компонента немедленно переводит child в terminal
  `budget_exceeded`. Model-token budget учитывается только если provider
  возвращает измерение; его отсутствие не снимает wall-clock/tool/output
  limits;
- Defaults принадлежат Core-owned policy snapshot и могут иметь только более
  строгие per-kind значения; parent/UI не могут повысить лимиты. Parent может
  уменьшить budget до запуска через descriptor, но не отменить hard maximum.
  Cancellation инициируется parent command, WinUI operator command или Core
  watchdog (timeout/budget/restart); все пути сходятся в один idempotent
  dispatcher cancellation token.
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
- Approval и acceptance — разные контуры: human approval в WinUI разрешает
  сам запуск/передачу конкретного bounded descriptor или report; parent
  acceptance gate автоматически проверяет schema, request match, bounds,
  provenance и policy независимо от решения человека. Human approval не
  превращает невалидный report в accepted, а acceptance не обходит требуемое
  human approval.
- `completed` — это значение `ChildReport.status`, а не финальное состояние
  child до parent acceptance. После валидного report child находится в
  `waiting_parent_acceptance`; только gate переводит его в `accepted` или
  `rejected`.
- Cancel и terminal completion используют атомарный compare-and-set по
  текущему state. Побеждает первая зафиксированная terminal transition;
  поздний report/cancel получает idempotent result и не изменяет состояние.
- `validating` охватывает schema, provenance, bounds и policy validation; до
  его завершения report не виден как evidence.
- Только Core dispatcher/runtime может переводить child из `created` в
  `queued`, `running`, `validating` и terminal states. Adapter может вернуть
  только result/error, parent gate — только `accepted` или `rejected` после
  `waiting_parent_acceptance`, а UI может лишь отправить command через IPC и
  отобразить результат. Parent/UI/system не могут напрямую записать state.

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

Child failure не является автоматически фатальным для parent pipeline. Core
классифицирует child kind как `required` или `advisory` в immutable workflow
configuration: failure required-child блокирует зависимую ветку и требует
нового child либо human decision; failure advisory-child переводит parent в
`degraded`, помечает отсутствующий результат как `unverified`, сохраняет
terminal reason и позволяет продолжить только операции, не требующие этого
evidence. Parent может запросить ручной ввод/approval или завершить задачу с
явным degraded outcome; silent fallback и выдача unverified report за
accepted запрещены.

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
- `confidence_percent < 70` помечает report как `needs_review` и запрещает
  автоматическое evidence acceptance; это metadata threshold, не security
  proof. Значение `0` допустимо как metadata, но не может пройти automatic
  acceptance;
- непустой summary, уникальные bounded sources, допустимый source format и
  provenance, который указывает на реально прочитанные Core операции;
- отсутствие secret-like content, mutation result или неразрешённого
  capability claim;
- prompt-injection markers и instructions внутри summary/findings/sources не
  исполняются и не меняют policy. Они сохраняются только как untrusted report
  text; явные попытки выдать себя за system/parent instruction, попросить
  secrets, расширить capabilities или обойти gate дают `rejected` с reason
  `untrusted_report_instruction`;
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
Invalid schema/source/provenance отправляется в quarantine (не в evidence),
пишется redacted rejection reason и требует нового report или human review.
Low confidence остаётся валидным `needs_review` report, но не принимается
автоматически; parent может принять его только отдельной explicit decision с
audit reason.

## Replay и reconnect

Каждое child event содержит `event_id`, `child_task_id`, child-local
monotonic `child_sequence`, глобальный Core `sequence_id`, `event_type`,
timestamp и bounded redacted payload. При reconnect UI передаёт последний
полученный Core `sequence_id`; Core возвращает snapshot child lifecycle и
последующий journal replay. Snapshot закрывает старую историю, а replay
достраивает её после snapshot sequence.

События принимаются только из authenticated current-user named pipe session
Core; UI не может публиковать child events обратно в journal. Core проверяет
parent/child ids против durable request, schema version, sequence ownership,
event type, payload bounds и policy snapshot. Malformed, forged, unknown-kind,
wrong-parent или out-of-order event отклоняется и получает redacted audit
reason; он не попадает в timeline/evidence.

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

Must-have этого подплана: Core-driven timeline, evidence panel и truthful
`blocked`/`failed`/`cancelled`/`timed_out`/`accepted`/`rejected`/`degraded`
states, reconnect/replay и минимальная форма запуска одного bounded child.
Catalog здесь ограничен read-only справочником kinds/capabilities/limits.
Полноценный visual DAG editor, свободное связывание цепочек, drag-and-drop,
массовое редактирование и advanced catalog UX — deferred scope подплана 4+;
они не должны задерживать security boundary и execution inspector.

- Catalog показывает только разрешённые kinds, их read-only capabilities,
  limits, expected report shape и доступные роли.
- Editor имеет явные режимы `Draft`, `Locked` и `Executing`: в `Draft`
  разрешены добавление/удаление/reorder ещё не запущенных descriptors и
  сохранение configuration; после immutable preview и запуска configuration
  переходит в `Locked`; `Executing` отображает runtime state и timeline только
  для чтения. Изменение требует создать новую draft revision и не меняет уже
  запущенный policy snapshot.
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
- Workflow валидируется дважды: при сохранении draft и непосредственно перед
  execution. Обе проверки выполняются Core, а не только WinUI: закрытый kind,
  role, capabilities, path scope, budgets, report requirements и approval
  mode сверяются с policy snapshot. Невалидный workflow не сохраняется как
  runnable и не получает command id.
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
- `degraded` и `unverified` являются parent workflow/evidence states, а не
  child terminal states: UI показывает, какой advisory child отсутствует и
  какое действие доступно. `waiting human approval` и
  `waiting_parent_acceptance` показываются раздельно.
- Human approval card показывает parent/child ids, kind/role, reduced-context
  manifest, read-only capabilities, path scope, budget/timeout, expected
  output, risk/reason и действия `approve`/`reject`. Решение, actor, timestamp
  и reason сохраняются в audit; reject переводит parent overlay в blocked и
  child остаётся queued либо получает cancelled по Core policy. Evidence
  доступно отдельной read-only панелью до и после acceptance, с явной
  маркировкой `untrusted`, `needs_review`, `accepted` или `rejected`.

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
   Добавить `MockChildAdapter` с быстрым, долгим, отменяемым, timeout,
   oversized-output, rejected-report и crash сценариями для UI/timeline tests;
   Mock adapter не получает production capabilities и не используется как
   security proof.
8. Провести отдельный security/contract suite: report schema, invalid
   evidence/source, prompt-injection report, malformed/forged IPC event,
   duplicate/gap replay, path traversal, symlink/reparse escape, environment
   secret access, mutation/shell/install/commit/network/nested-child attempts.

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
- editor корректно проводит `Draft → Locked → Executing`, не позволяет
  менять permissions policy после запуска и отдельно показывает human
  approval, parent acceptance, `degraded` и `unverified`;
- replay после reconnect не дублирует child events и не воскрешает terminal
  child;
- child crash даёт `failed` с machine-readable reason, внутренний gate failure
  даёт `blocked` с redacted detail, а sandbox negative tests подтверждают
  denial для write/shell/nested child/network mutation/traversal/symlink escape;
- 100% negative attempts в security suite получают denial/terminal bounded
  result; oversized output никогда не публикуется как evidence;
- каждый published child event содержит parent_task_id, child_task_id, kind,
  timestamp, event_id, sequence и policy/budget provenance;
- reconnect suite показывает 0 duplicate timeline events по event_id при
  повторной at-least-once доставке и корректно обрабатывает checkpoint gap;
- acceptance suite принимает только schema-valid report с разрешёнными
  sources и matching request/provenance, а invalid/low-confidence/injection
  report остаётся в quarantine или `needs_review`;
- focused Rust tests, IPC compatibility tests, WinUI smoke и `git diff --check`
  проходят; generated artifacts очищены.

## Зависимости

- `child_roles`, `child_runtime`, child storage и базовые IPC commands уже
  существуют как контрактный/persistence слой, но их execution wiring ещё не
  готов — это основной объём данного подплана;
- До начала adapters обязательна enforcement-аудитка: `ChildTaskRequest::validate`,
  dispatcher и tool runtime должны независимо отвергать nested child,
  mutation capability, shell/process spawn, network capability и path escape.
  Unit-тесты контрактов недостаточны; требуются end-to-end negative IPC tests,
  доказывающие отказ на реальном Core command path без участия UI.
- Execution gate закрыт, пока не утверждён Core child runtime policy snapshot
  со всеми полями: разрешённые `ChildTaskKind`, capabilities и path scopes,
  forbidden actions, concurrency/retry/wall-clock/output/tool budgets,
  timeout/cancellation rules, report/event schemas и evidence requirements.
  Документация без enforceable code/tests не считается завершённой policy.
- task lifecycle, event journal, cancellation, replay/reconnect и bounded
  checkpoint/recovery foundation уже существуют в Core, однако child-specific
  lifecycle/events/lease integration ещё нужно добавить;
- подплан 4 не является обязательной зависимостью для security boundary или
  focused testability. Его `RunPolicy`/runner можно переиспользовать позже,
  но MVP подплана 3 должен иметь самостоятельный Core-owned child policy,
  deterministic adapters и тестовый harness;
- полноценные pause/resume, arbitrary dependency graph и provider/model
  routing остаются в подплане 4 и не должны быть скрыты внутри child editor.
