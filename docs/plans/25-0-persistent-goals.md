# План 25.0 — Persistent Goals для длительных задач

Статус: предложено по [issue #5](https://github.com/rkfsociety/EvoHime/issues/5).
Goal — durable objective/progress projection, а не новый scheduler и не
автоматическое превращение каждого сообщения в автономную задачу.

## Цель и граница

Добавить явную Core-owned сущность `Goal`, живущую дольше model turn, связанную
с workflow runs, child runs, checkpoints и проверяемыми success criteria. Goal
отвечает «что должно быть достигнуто»; workflow отвечает «какой граф действий»;
Continuation Policy отдельно решает, разрешено ли продолжение.

Текущие workflow runtime, child contracts, event journal, scratchpad и UI
Operations остаются действующими контурами. Goal не получает capabilities и не
хранит credentials.

## Зависимости

### Блокирующие

- план 23 TaskCheckpoint для durable continuity и compaction/recovery;
- существующий immutable workflow definition/runtime и child report contracts;
- Core-owned SQLite transaction/migration, event journal и authenticated IPC;
- существующие budget, approval, evidence и recovery semantics.

### Опциональные

- план 26 Continuation Policy для bounded autonomous next action;
- план 27 retained child contexts для повторных специализированных runs;
- план 29 Continual Refinement для evidence из завершённых goals.

## Контракт и состояния

Ввести versioned `GoalV1`: `id`, `workspace_id`, `chat_id?`, `objective`,
`success_criteria[]`, `status`, `progress_summary`, `completed_criteria[]`,
`remaining_criteria[]`, `blockers[]`, `next_action?`, `workflow_run_ids[]`,
`child_run_ids[]`, `checkpoint_id?`, optional `token_budget`, `cost_budget`,
`continuation_budget`, timestamps и `content_hash/version`.

Состояния: `Active`, `Paused`, `Blocked`, `BudgetLimited`, `Completed`,
`Failed`, `Cancelled`. `Completed` разрешён только после authoritative success
criteria или отдельного разрешённого user decision с audit reason. Обычный
assistant answer, model summary или завершение одного child не закрывает Goal.

Criterion имеет type `manual|gate|workflow_evidence|artifact`, status, evidence
ref, verifier identity/version и `verified_at`. Renderer не может подделать
evidence. Изменение objective/criteria после запуска создаёт immutable revision;
старые workflow runs сохраняют ссылку на прежнюю Goal version.

## Core runtime и persistence

Создание разрешено только явной user/UI командой или validated workflow/template,
если это будет отдельно разрешено контрактом. После значимого durable event Core
обновляет projection транзакционно: criterion, blocker, workflow/child terminal
state, gate, budget или checkpoint. Guarded version/sequence не допускает
устаревшему событию откатить новую projection.

Goal registry хранится additive SQLite schema с append-only revisions/events и
bounded projection. Startup recovery загружает Goal, сверяет связанные runs и
checkpoint, не повторяет uncertain effects и помечает unknown/degraded state.
Завершённый Goal не удаляется вместе с чатом; explicit cancel/delete policy
должна быть отдельной и audit-able.

Budget counters используют существующие Core policy limits. Исчерпание токенов,
стоимости, continuations или wall-clock переводит в `BudgetLimited`, а не в
`Completed`/`Failed`.

## UI и IPC

Добавить typed команды create/read/list/pause/resume/cancel/update criteria и
bounded Goal events. В `OverviewPanel`/`OperationsPanel` показывать objective,
progress `N/M`, blockers, current/next action, budgets, pending approvals,
linked workflows/children/evidence и recovery warning. Действия пользователя
идемпотентны и не обходят Core policy.

## Этапы реализации

1. Уточнить schema/version, criterion types, status transition table, revision
   rules и completion proof matrix.
2. Добавить SQLite Goal/revision/event store с transactional projection и
   migration backup.
3. Подключить workflow/child/checkpoint events, budget accounting и recovery.
4. Реализовать additive IPC и bounded UI с pause/resume/cancel/manual criterion.
5. Добавить tests на stale events, restart/compaction, evidence, versioning,
   budget limits и renderer tamper resistance.

## Критерии готовности

- Goal создаётся явно, durable и имеет versioned status machine;
- success criteria/evidence отличают подтверждение от текста модели;
- Goal связывает несколько workflow/child runs и последний checkpoint;
- stale event не ломает новую projection, recovery не повторяет uncertain effect;
- budget exhaustion виден как `BudgetLimited`;
- objective/criteria versioning сохраняет историю;
- UI позволяет pause/resume/cancel и показывает blockers/next action;
- Goal не расширяет capabilities и не хранит credentials;
- проходят storage, recovery, IPC, UI и deterministic transition tests.

## Не входит

Автосоздание Goal для каждого сообщения, новый scheduler, замена workflow,
бесконечная автономность или скрытые неудаляемые цели.
