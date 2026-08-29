# План 23.0 — TaskCheckpoint для compaction и recovery

Статус: предложено по [issue #7](https://github.com/rkfsociety/EvoHime/issues/7).
Это обзорный план направления; реализация начинается только после отдельного
evidence review и уточнения текущей schema revision.

## Цель

Добавить Core-owned immutable `TaskCheckpoint`, который отвечает на вопрос,
что в задаче уже подтверждено, что осталось, какие решения и blockers активны,
какие проверки прошли и какой следующий безопасный шаг допустим. Checkpoint
должен переживать compaction, перезапуск Core и длительную цепочку workflow
операций, не превращаясь в свободный model summary или backup workspace.

## Текущее состояние и граница изменения

В checkout уже есть SQLite-backed state machine context compaction, bounded
scratchpad, immutable context ledger, content-addressed ArtifactStore и отдельный
`CoordinatorCheckpoint` для child workflow. Они остаются источниками своих
контрактов. Новый checkpoint объединяет continuity state задачи и ссылки на эти
источники, но не заменяет event journal, child checkpoint, workflow runtime,
ledger или ArtifactStore.

Предполагаемые точки интеграции:

- `crates/evohime-core/src/context_budget.rs` — границы compaction и projection;
- новый `crates/evohime-core/src/task_checkpoint.rs` — typed contract, validation,
  projection и replay rules;
- новый `crates/evohime-local-storage/src/task_checkpoint_store.rs` — immutable
  chain, hash, retention и transactional writes;
- `crates/evohime-core/src/lib.rs` и
  `crates/desktop-ipc/proto/evohime.desktop.proto` — Core commands/events;
- Electron main/preload и `OverviewPanel`/`OperationsPanel` — bounded read-only
  projection без raw prompt, secrets или полного transcript.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./23-1-task-checkpoint.md)
- [Этап 2 — runtime-интеграция и recovery](./23-2-task-checkpoint.md)
- [Этап 3 — IPC, client projection и UI](./23-3-task-checkpoint.md)
- [Этап 4 — verification, release-evidence и закрытие](./23-4-task-checkpoint.md)

## Зависимости

### Блокирующие

- текущие Core-owned SQLite migrations, transaction helper и event journal;
- текущая context compaction state machine и `context_ledger`;
- текущие workflow/child durable states, leases и `unknown_outcome` semantics;
- существующий ArtifactStore с hash, locator, sensitivity и parent-scope checks;
- authenticated versioned desktop IPC.

### Опциональные

  для обычной task без Goal;
- планы 27–28: retained child и kernel refs могут быть добавлены в массивы
  typed refs после их появления.

## Контракт и provenance

Ввести versioned `TaskCheckpointV1` с полями `id`, `version`, `workspace_id`,
`chat_id?`, `goal_id?`, `parent_checkpoint_id?`, `objective`, `status`,
`completed_items`, `remaining_items`, `decisions`, `blockers`, `files_read`,
`files_changed`, `tests_passed`, `tests_failed`, `gates`, `pending_approvals`,
`workflow_refs`, `child_refs`, `artifact_refs`, `open_questions`, `next_action?`,
`narrative_summary?`, `source_event_seq`, `created_at` и `content_hash`.

Каждый элемент должен иметь типизированный источник либо быть явно помечен как
`core_derived` или `model_proposed`. Core-derived данные включают workflow/child
states, tests/gates, approval state, artifact refs, file metadata, budgets,
event sequence и recovery state. Модель может предложить только narrative,
remaining items, open questions, next action и semantic decisions; эти поля
проходят bounded schema/policy validation и не способны подтвердить эффект,
approval, тест или завершение.

Serialization должна быть канонической и детерминированной. Hash покрывает
версию контракта, нормализованные typed fields, refs и порядок коллекций.
Сохранённая запись immutable; новая projection создаёт новый checkpoint и
ссылается на предыдущий через `parent_checkpoint_id`.

## Жизненный цикл

Checkpoint создаётся на значимых границах: перед compaction, после workflow
stage или принятия child report, перед pause/approval/detach/shutdown, после
существенной смены плана и по явной команде пользователя. Нужна coalescing и
throttling policy, чтобы не создавать snapshot после каждого token/tool event.

Поток compaction:

1. Core фиксирует relevant event sequence и блокирует конкурирующую projection.
2. Core собирает authoritative state и валидирует model-proposed поля.
3. Checkpoint записывается транзакционно до удаления старого контекста.
4. При необходимости создаётся bounded narrative projection.
5. В следующий context передаётся typed checkpoint catalog и ссылки, а не весь
   предыдущий transcript.

Поток recovery: загрузить последний валидный checkpoint, replay/project durable
events после `source_event_seq`, сверить leases и unknown outcomes, отметить
stale/conflicted workspace и только после этого публиковать актуальную
projection. Никакой uncertain side effect не повторяется автоматически.

## Размер, приватность и stale state

Полные файлы, raw provider output, секреты и необезличенные sensitive payload не
хранятся в checkpoint. Большие данные уходят только в существующий ArtifactStore
с sensitivity policy и hash. Для файлов сохраняются normalized path, before/after
hash, change kind и evidence ref. Изменение workspace вне задачи должно давать
`stale`/`conflicted`, а не молчаливое продолжение.

Ограничить число элементов по категориям, длину summary/excerpts и общий
serialized size. Повреждённый или несовместимый последний snapshot откатывается
к предыдущему валидному и запускает redacted diagnostic event.

## IPC и UI

Добавить команды чтения последнего checkpoint, bounded history и explicit save;
mutation принимает idempotency key и пишет audit. Event projection должна
содержать только progress, blockers, failed/passed gates, pending approvals,
next action, timestamp, revision и recovery warning. Renderer не получает raw
JSON, prompt, полный transcript или секретные refs.

## Этапы реализации

1. Зафиксировать contract/version, provenance matrix, canonical hash и limits.
2. Добавить additive SQLite schema, immutable store, backup-before-migration и
   deterministic fallback при повреждённой цепочке.
3. Подключить compaction boundaries, event replay, workspace freshness и
   recovery без повторения uncertain effects.
4. Добавить IPC projection, Operations/Overview UI и audit/idempotency.
5. Добавить unit, storage, recovery, IPC, renderer и security-eval fixtures.

## Критерии готовности

- versioned `TaskCheckpoint` durable contract и immutable parent chain;
- Core-derived evidence отделён от model-proposed summary;
- checkpoint создаётся до compaction и используется после replay recovery;
- stale workspace, unknown outcome, pending approval и failed gate не теряются;
- большие и sensitive данные не копируются в checkpoint/renderer;
- corrupted latest snapshot безопасно заменяется предыдущим + replay;
- IPC/UI показывают bounded typed projection;
- проходят storage, compaction, restart, deterministic hash, redaction и
  `git diff --check`/проектные документационные gates.

## Не входит

Полный transcript в каждом snapshot, копирование workspace, backup файлов,
замена event journal или автоматический повтор незавершённых эффектов.
