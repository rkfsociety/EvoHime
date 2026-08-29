# План 96.0 — Memory Views & Adaptive Recall: hierarchical scopes, read-only slices и composite retrieval

Статус: предложено по [issue #76](https://github.com/rkfsociety/EvoHime/issues/76). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Расширить Memory Governance (#30) отдельным **Memory Views & Adaptive Recall** слоем: организовать память в иерархические logical scopes, выдавать ролям безопасные read/write views на один или несколько scopes и выбирать глубину retrieval в зависимости от сложности запроса.

Memory Governance остаётся authority по тому, **что можно записать и насколько записи доверять**. Новый слой отвечает на другое:

> какую часть памяти конкретный agent/run может видеть, куда ему разрешено писать и насколько глубоко нужно искать релевантный контекст.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/memory_views_and_adaptive_recall.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./96-1-memory-views-and-adaptive-recall.md)
- [Этап 2 — runtime-интеграция и recovery](./96-2-memory-views-and-adaptive-recall.md)
- [Этап 3 — IPC, client projection и UI](./96-3-memory-views-and-adaptive-recall.md)
- [Этап 4 — verification, release-evidence и закрытие](./96-4-memory-views-and-adaptive-recall.md)

## Зависимости

### Блокирующие

- План 50.0 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 68.0 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection.
- План 70.0 — Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- paths не являются ACL;
- MemoryView Core-owned;
- child/role views являются subset parent/project grants;
- read-only shared view нельзя использовать для write;
- model scope inference не расширяет access;
- Secret/Private записи фильтруются до retrieval;
- deep query planner видит только authorized scope metadata;
- retrieval score не меняет authority записи;
- promoted memory повторно проходит governance.

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

- [ ] Есть hierarchical logical memory scopes.
- [ ] Core создаёт scoped/sliced MemoryView с отдельными read/write rights.
- [ ] Shared read-only memory поддерживается.
- [ ] Retrieval использует explainable composite scoring.
- [ ] Есть Shallow/Deep/Auto recall modes.
- [ ] Deep recall не может выйти за MemoryView.
- [ ] Background ingestion имеет определённую consistency/read-barrier semantics.
- [ ] UI позволяет инспектировать scope/view и причины retrieval.

## Ограничения и non-goals

- использовать path naming как security boundary без Core ACL;
- разрешать model самостоятельно создавать privileged scopes;
- глобальная multi-user vector database;
- бесконечный recursive retrieval;
- автоматическая запись каждого model output в shared memory;
- замена Memory Governance (#30).

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#76 Memory Views & Adaptive Recall: hierarchical scopes, read-only slices и composite retrieval](https://github.com/rkfsociety/EvoHime/issues/76)
