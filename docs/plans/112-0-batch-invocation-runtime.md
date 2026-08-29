# План 112.0 — Batch Invocation Runtime: bounded map execution по наборам inputs с per-item isolation и resume

Статус: предложено по [issue #92](https://github.com/rkfsociety/EvoHime/issues/92). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Batch Invocation Runtime**: Core-owned механизм для запуска одного и того же agent/workflow definition по набору независимых входов с bounded concurrency, отдельным состоянием каждого элемента и возможностью безопасно продолжить незавершённую пачку после restart.

Это production-функция, а не benchmark runner.

Примеры:

- проверить 200 файлов одним review workflow;
- прогнать один analysis workflow по списку репозиториев;
- обработать набор документов;
- применить одинаковую migration/check procedure к нескольким targets;
- запустить однотипные research/review задачи по dataset rows.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/batch-invocation-runtime.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 25.0 — Persistent Goals: durable цели для долгих задач.
- План 26.0 — Continuation Policy: bounded autonomous loops и quality gates.
- План 63.0 — Composable Termination Conditions: first-class stop policies for agent and team runs.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 45.0 — External Coding Agent Adapter: подключение Codex/Claude/Gemini-подобных executors через typed protocol.
- План 62.0 — Team Resource Budget: shared cost envelope, per-role allocations и reserved verification budget.
- План 83.0 — Reasoning Operator Library: typed Generate/Review/Revise/Ensemble primitives для agent workflows.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
BatchInvocation {
  id,
  definition_ref,
  definition_version,
  invocation_profile_ref?,
  items[],
  concurrency_policy,
  budget_policy,
  failure_policy,
  status,
  created_at,
  started_at?,
  completed_at?,
  content_hash
}
```

Каждый элемент:

```text
BatchItem {
  item_id,
  ordinal,
  input_payload,
  input_hash,
  status,
  run_id?,
  attempts,
  result_ref?,
  error_class?,
  created_at,
  updated_at
}
```

`item_id` стабилен внутри batch и используется для dedup/recovery.

### Безопасность

- batch не расширяет capabilities/grants;
- каждый item проходит обычную input/capability/approval validation;
- child/item grants являются subset batch parent ceiling;
- credentials только refs и revalidated per run;
- unknown outcome не retry-ится автоматически;
- concurrency ограничена provider/backend/resource policy;
- imported dataset считается untrusted input data;
- batch result export sensitivity-aware;
- модель не может скрыто увеличить batch concurrency/budget сверх policy.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть durable BatchInvocation/BatchItem contracts.
- [ ] Один definition можно запускать по списку validated inputs.
- [ ] Каждый item имеет отдельный run/state/provenance.
- [ ] Concurrency и per-item/global budgets bounded.
- [ ] Batch переживает Core restart и продолжает Pending work без дублей.
- [ ] Partial failures и approvals не теряют прогресс остальных items.
- [ ] Retry учитывает idempotency/unknown-outcome semantics.
- [ ] Есть aggregate result/export и per-item drill-down.

## Ограничения и non-goals

- distributed cluster scheduler;
- бесконечные datasets/stream processing;
- автоматический retry любых side effects;
- один общий mutable agent context для всех items;
- использование batch как способ обойти approvals;
- замена Benchmark Matrix;
- автоматическая генерация batch items моделью без input review/limits.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#92 Batch Invocation Runtime: bounded map execution по наборам inputs с per-item isolation и resume](https://github.com/rkfsociety/EvoHime/issues/92)
