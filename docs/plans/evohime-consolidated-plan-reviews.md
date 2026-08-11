# Свежие ревью плана EvoHime

Этот файл содержит новую серию замечаний к [сводному плану EvoHime](evohime-consolidated-plan.md).

## Ревью

<!-- Новые ревью добавляются ниже по мере поступления. -->

## Ревью 1 — объём MVP, recovery и последовательность поставки

### Основные замечания

1. **Минимальный MVP всё ещё велик.** 0a + 0b + 1 + минимальная часть 2 остаются инфраструктурой уровня распределённой системы: IPC/replay, reconciliation effects, supervisor recovery, task graph и UI. Разделение этапа 0 делает объём видимым, но не уменьшает его. Рекомендуется рассмотреть ещё более ранний пользовательский сценарий: пустой task workspace с ручным запуском.

2. **Unknown effects требуют алгоритма.** Контракт `prepared → executing → unknown → reconciliation` правильный, но для file/network/shell effects нужно определить, проверяет ли Core idempotency key, спрашивает пользователя, переводит run в blocked или использует type-specific verifier. Blind retry запрещён.

3. **Очередь команд и latency.** Нужно явно указать, обрабатывает ли единая Core command queue команды строго последовательно или допускает параллельные read-only запросы. Это важно для UI, runner и supervisor health-ping при цели p95 IPC ≤ 100 мс.

4. **Approval fatigue.** Пакетирование writes в scope недостаточно, если drift/sensitive file/unexpected diff вызывают 5–10 диалогов. Нужен batch approval для серии однотипных операций или ограниченный механизм `trust this pattern` с явными сроком, scope и risk boundaries.

5. **Изменение статического workflow.** Если план на середине run оказался неверным, UX должен явно описывать путь: pause/cancel текущего run, сохранить evidence, создать новую версию graph и запустить новый run; редактирование immutable graph на месте запрещено.

6. **Миграция между версиями продукта.** Forward-compatible schema rules не описывают переход MVP → этапы 3–9. Нужна политика: автоматические backward-compatible migrations где возможно, иначе migration wizard с backup, preview, progress и recovery.

7. **Слишком линейные зависимости.** UI task workspace после базового IPC может разрабатываться параллельно с recovery backend; research UI можно проектировать параллельно с backend этапа 2. Жёсткая цепочка увеличивает time-to-feedback.

8. **Personality permissions.** Негативный behavioral test не исчерпывающий. Лучше structural guarantee: permission resolver получает только `SkillDefinition.allowed_tools` и `PolicySnapshot`; personality вообще не передаётся в resolver.

9. **Verbosity/diagnostic level.** Нужны user-configurable уровни диагностики для Core Doctor и пользовательской отладки при сохранении redaction и безопасных audit trail.

10. **Benchmark.** 10 000 задач оставить как stress test, но основную целевую метрику `next_ready` измерять на реалистичных 500 задачах; типичный диапазон пользователя оценивается в 50–300.

### Итог

Архитектура sound, границы и safety model продуманы. Главные риски — объём и порядок поставки. Рекомендуется проверить, можно ли дать пользователю пустой task workspace с ручным запуском до полного replay/recovery.

## Ревью 2 — MVP-границы, конфликты UI и точность контрактов

### Критичные и высокоприоритетные замечания

1. **0a/0b всё ещё могут расползтись.** Exit criteria широкие, особенно для 0b: `unknown` reconciliation, lease, gap/resync и Job Object уже образуют recovery platform. Для MVP milestone следует оставить только минимальный recovery, а сложные reconciliation-сценарии, generation и full snapshot пометить как `0b+` после первого feedback.

2. **Optimistic locking и UX конфликта.** При конфликте UI должен получить `expected_version`, `current_version` и diff или last event. Нужно определить действия пользователя: `reload and retry`, ручной merge или явно ограниченный `force overwrite`; абстрактный diagnosable conflict недостаточен.

3. **`next_ready` и неготовые зависимости.** Явно перечислить, что зависимости в `blocked`, `waiting_approval`, `failed`, `cancelled` и иных незавершённых статусах блокируют выбор; готовой считается только зависимость в `done`.

4. **Checkpoint и external effects в UX.** В интерфейсе должно быть очень заметно, что workspace rollback не откатывает SQLite и внешние side effects. Иначе пользователь может ошибочно считать, что Rollback вернул всё состояние.

### Средний приоритет

5. **Research stub.** Plan/Build должен работать полностью offline с пустым research; context assembler не должен иметь обязательной зависимости от research.

6. **Role/Skill refs в 0a.** Ограничить их immutable snapshot + hash; реальный registry и matcher остаются этапом 4 и не входят в 0a.

7. **Состав approval hash.** Зафиксировать, что `intent_hash` включает как минимум immutable diff, command/action, scope, `risk_class` и effective policy/permissions hash.

8. **WinUI reconnect.** UI не должен быть чисто stateless: reducer кэширует `last known good snapshot` и sequence для reconnect, но не становится владельцем state.

### Мелкие и редакционные замечания

9. SQL-пример должен либо включать `priority`, `estimate`, `complexity`, `attempt_count` и другие перечисленные поля, либо явно называться минимальным каркасом.

10. Нужно устранить терминологическое расхождение `define` vs `/spec` в lifecycle и UI-поставке.

11. В risk matrix явно добавить `memory_write` и `research_write` или указать, к какому классу они относятся.

12. Добавить target размера SQLite, например для 10 000 tasks и 1 000 runs без vacuum.

### Итоговая рекомендация

План готов к работе. Главный риск — объём 0a/0b. Для быстрого MVP рекомендуется ограничить 0b сценарием: после kill восстанавливаются graph и последний durable checkpoint, а `unknown` effects сразу переходят в `blocked/approval`; расширенное reconciliation отложить до обратной связи.

## Ревью 3 — сужение MVP, UI approval и эксплуатационные контракты

### Основные замечания

1. **MVP слишком широк.** 0b с recovery effects, idempotency и reconciliation может задержать первый feedback на месяцы. Предлагается MVP ограничить 0a + этапом 1, оставив полноценный recovery отдельным релизом.

2. **Research в этапе 2.** Пустой stub может сделать Plan/Build слишком слабым. Следует либо разрешить локальные workspace-файлы как минимальный offline context, либо явно указать, что ранний Plan строится только по PRD и ручным подсказкам.

3. **UI approval и policy.** Нужно описать настройку `allowed_paths`, budgets и timeout, способ отображения diff, approval action и immutable intent hash. Эскиз approval/policy UI нужен уже в этапах 2–5.

4. **Слишком много бюджетов и stop conditions.** На ранних этапах лимиты должны быть щадящими/расширенными, а остановки — объяснимыми, иначе MVP будет часто завершаться cryptic errors.

5. **Long-running IPC.** Нужны промежуточные progress/log events или heartbeat-статусы для research, build и workflow, чтобы UI не выглядел зависшим.

6. **Дизайн 0b.** Перед реализацией recovery рекомендуется отдельный protocol design review, при возможности моделирование/TLA+, и kill-9 тесты во всех точках effect protocol.

7. **Workspace scope.** Определить, что входит в workspace: например, текстовые файлы репозитория; задать предел snapshot и стратегию для больших бинарных файлов. Уточнить, используется ли Git-compatible механизм для diff/rollback.

8. **Git integration.** Явно описать Core workspace tools: встроенный Git-клиент или ограниченные `git` shell calls. Команды должны вписываться в `dangerous` policy и approval; auto-commit/push остаются отдельными действиями.

9. **Research и skills.** Они могут быть взаимозависимы. Нужны независимые интерфейсы и общий context contract, чтобы research мог работать с любым skill и разработка могла идти параллельно.

10. **Измеримые критерии этапов.** Для каждого этапа нужны acceptance metrics: recovery time/success rate, максимальный граф, latency, snapshot size, UI reconnect и другие конкретные thresholds.

11. **Обновление Core и миграции.** Описать, как новая версия Core запускает миграции, сохраняет старые данные и откатывается; нужен понятный update/migration flow для пользователя.

12. **Force mode.** Для опытных пользователей можно добавить явный force/override для scope drift или ambiguous criteria, но только с сильным подтверждением, ограничением scope и audit record.

13. **Lifecycle Core.** Добавить graceful shutdown/update protocol: IPC `shutdown`, flush/checkpoint перед завершением и handoff состояния следующему экземпляру.

14. **Слишком много статусов для 0a.** В минимальном storage foundation можно оставить `backlog`, `ready`, `in_progress`, `done`, а `blocked`, `waiting_approval`, `failed`, `cancelled` вводить вместе с соответствующими этапами.

15. **Production logging/monitoring.** Помимо audit trail нужен пользовательский экспорт логов и метрик, возможно JSONL-файл и Windows Event Log integration в рамках Core Doctor/этапа 7.

### Итоговая рекомендация

Сузить MVP до 0a + 1, добавить UI approval/policy, уточнить workspace/Git, включить измеримые exit criteria для каждого этапа, описать обновление Core и graceful shutdown. Замечания не блокируют архитектуру, но уменьшают риск неожиданной сложности.

## Ревью 4 — идентичность, command idempotency и recovery state machine

### Критичные замечания

1. **Противоречивая граница MVP.** В начале MVP включает 0a, 0b, 1 и часть 2, а dependency graph и финальный порядок фактически ставят feedback после 0a/0b/1, а Plan/Build — после feedback. Рекомендуется формально разделить **MVP-1 / Feedback build** (`0a + 0b + 1`) и **MVP-2 / Agentic build** (`+ минимальный 2`).

2. **Семантика edges.** `work_items.parent_id` должен обозначать decomposition hierarchy, а dependency graph — отдельные поля `work_item_edges.from_work_item_id`, `to_work_item_id`, `kind`. Нужно явно зафиксировать направление dependency, особенно для правила «все зависимости должны быть done».

3. **Identity contract.** Для persistent domain IDs определить формат и владельца. Предлагается: UUIDv7, генерирует только Core, ID immutable, import не принимает внешний ID как authoritative, внешние IDs хранятся отдельно в `source_ref`; uniqueness и сохранение ID при export/import должны быть явными.

4. **IPC command idempotency.** `request_id` должен иметь durable/bounded deduplication: `(request_id, session/client identity) → command_hash → committed_result`. Повтор того же запроса возвращает тот же результат; тот же ID с другим payload — protocol error. Replay событий сам по себе этого не решает.

5. **Полный PolicySnapshot.** Snapshot должен содержать canonical serialized effective policy, schema version и hash, достаточные для forensic replay, а не только ID текущей конфигурации. То же правило применить к Role/Skill/ModelRoute snapshots.

6. **Recovery vs resume.** Ввести состояния `RECOVERING → RECONCILING → RESUMABLE | BLOCKED | WAITING_APPROVAL | FAILED`; только `RESUMABLE → RUNNING`. UI должен различать восстановление состояния и продолжение выполнения.

7. **Cancellation contract.** Определить cooperative/forcible cancellation, судьбу executing effect, связь timeout с cancellation, terminal-момент `cancelled`, запрет resume cancelled run и связь supervisor kill с reconciliation. Рекомендуемая семантика: cancellation запрещает новые effects, started effects получают cancellation request, неопределённый outcome становится `unknown`, terminal cancelled — только после reconciliation.

### Дополнительные контракты

8. **Workspace snapshot.** Выбрать конкретную модель: Git tree-like object, manifest + hashes + content blobs, copy-on-write или baseline diff. Описать untracked files, renames, deletes, binaries, symlinks/reparse points и внешнее изменение файла во время run. Запись должна использовать `expected_content_hash`; mismatch превращается в workspace conflict, а не overwrite.

9. **Scope model.** Добавить `max_bytes_changed`, `allow_create`, `allow_delete`, `allow_rename`, `allowed_file_types` и `baseline_snapshot_id` к существующим path/operation/output limits.

10. **Verified evidence.** К `Evidence` добавить `producer`, `verification_status`, `verifier` и input/baseline hash. Разделить `claimed` и `verified` evidence: сообщение агента «tests passed» не заменяет реально запущенный command с exit code 0.

11. **Retention contract.** Уже в 0a/0b определить, что immutable навсегда, что compactable, что disposable и что нельзя удалить из-за references. Это относится к events, provenance, checkpoints, evidence, research, audit и diffs.

12. **UI не является источником истины.** Любая UI-команда — request/intention, а не authoritative transition. UI запрашивает `mark task done, expected_version=N`; Core валидирует transition и публикует authoritative state.

### Рекомендуемые точечные изменения

Перед реализацией 0a исправить шесть вещей: разделить MVP Feedback и MVP Agentic; переименовать edges в `from/to`; добавить Identity и IPC command idempotency; ввести recovery state machine; формализовать workspace snapshot/concurrent modification; закрепить canonical immutable representation для policy/role/skill/route snapshots.

После этих правок план следует считать готовым к реализации 0a, не продолжая архитектурное проектирование бесконечно.

## Ревью 5 — rollback, IPC compatibility, audit и operational readiness

### Приоритизация и риски

1. Уточнить exit criteria 0a/0b: отдельно тестировать rollback при сбое миграции и определить поведение partial gap, когда пропущена только часть событий. Добавить hard deadline для каждого подэтапа, чтобы фундамент не затягивался бесконечно.

### Технические детали

2. Для SQLite/WAL описать retention policy и очистку WAL-файлов. Для optimistic locking зафиксировать UI conflict resolution: показать expected/current version и diff, предложить reload/retry или merge.

3. Для versioned named pipe явно гарантировать backward compatibility старых клиентов. Для oversized payload определить отказ с диагностикой или chunking protocol.

4. Reconciliation должен логироваться в audit trail с effect id, idempotency key, verifier и решением. Idempotency keys должны быть уникальны глобально или в явно определённом domain scope, а не только внутри run.

### Безопасность

5. Добавить audit trail для всех external actions, включая GitHub/network requests, с redaction. Для research/network ввести rate limiting. Для child roles явно ограничить filesystem и network sandbox.

6. Описать rotation ключей в Windows Credential Manager/DPAPI и поведение при смене пользовательских credentials.

### UI/UX

7. Проверить responsive behavior three-zone frame на разных размерах окна. Для Empty/Loading/Ready/Running/Degraded/Error/Blocked добавить user-friendly сообщения с конкретным path, причиной и следующим действием; не показывать секреты.

### Тестирование и quality gate

8. Для benchmark на 10 000 задач определить latency, memory usage и throughput, а основную target-метрику сопоставить с реальным размером графа. Добавить concurrent graph editing tests.

9. В CI добавить integration tests взаимодействия Core, Supervisor и WinUI/IPC, включая restart, zombie child process, health-ping, replay и recovery.

### Документация и операционная политика

10. Вынести отдельный glossary с определениями `bounded loop`, `provenance`, `capability registry`, `reconciliation`, checkpoint и evidence; добавить примеры acceptance criteria для каждого этапа.

11. Для non-goals добавить краткое обоснование и fallback/альтернативу: почему SQLite вместо graph DB, чем заменяются external calendars и т.п.

12. Описать crash supervisor: restart/notification policy, диагностику и cleanup zombie processes, включая зависшие дочерние процессы.

13. Для MVP milestone определить сбор feedback через UI и локальные logs, а также success metrics: время выполнения типовых задач, доля успешных ручных запусков, reconnect/recovery UX и число необъяснимых blocked states.

### Итог

План качественный и готов к реализации 0a/0b при жёстком контроле exit criteria, backward compatibility, external audit, rate limiting и integration tests.

## Ревью 6 — разделение графов, упрощение MVP и минимальные контракты

### Стратегические замечания

1. **0a/0b слишком насыщены для P0.** Replay/gap/resync, unknown effect reconciliation, lease/generation, Job Object cleanup и protocol negotiation могут потребовать 3–4 подэтапа. Предлагается вынести protocol negotiation и effect reconciliation в отдельный 0c, чтобы MVP не зависел от полного recovery pipeline.

2. **Task graph и workflow graph — разные графы.** Нужно описать их связь: может ли workflow node ссылаться на `work_item`, имеет ли work item свой workflow subgraph, где находится mapping и ownership.

3. **Plan/Build слишком сложен для MVP.** Для первого релиза можно оставить read-only Plan, Build с ограниченным списком разрешённых файлов и один approval на весь Build; mutation matrix, protected paths, compaction, rollback и сложный approval hash отложить.

4. **Skills/roles для MVP.** Полный matcher, lifecycle snapshots, manifest/signature и capability governance оставить этапу 4. В MVP достаточно `RoleRef`, `SkillRef`, `allowed_tools` и `risk_class`.

### Архитектурные замечания

5. **RunEffect можно упростить.** Для первой реализации использовать `prepared → executing → completed`, где completed содержит success/failure; `unknown` возникает только после crash, а сложный `reconciliation_state` отложить до 0b/0c.

6. **Минимальный snapshot contract.** Нужен явный формат, связанный с run:

```text
snapshot { id, run_id, workspace_hash, diff[], created_at }
```

Также определить atomicity snapshot + diff и формат хранения.

7. **Memory v1 слишком амбициозна.** Для этапа 6 достаточно append-only provenance, lexical search и derived facts без confidence; entity/temporal signals, TTL, compression и vector search можно перенести в memory v2.

### Исполнимость и тестирование

8. Добавить таблицу MVP acceptance tests с минимальными Core и UI smoke-критериями для milestone после этапа 1.

9. Зафиксировать минимальный IPC surface MVP: `CreateTask`, `UpdateTask`, `AddEdge`, `RemoveEdge`, `GetGraph`, `StartRun`, `StopRun`, `ResumeRun` и соответствующие events/acks.

### Безопасность

10. К risk matrix добавить default tool policy: конкретные примеры инструментов классов `read`, `write`, `dangerous`, `external` и их default approval/allowlist rules.

### Итоговая рекомендация

Разделить 0a/0b при необходимости на 0c, отделить task graph от workflow graph, упростить MVP Plan/Build, RunEffect, skills/roles и memory, а также добавить MVP acceptance tests, IPC command list и default risk policy.
