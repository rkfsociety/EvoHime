# План 101.0 — Knowledge Source Registry: project/role RAG, source provenance и indexed reference context

Статус: предложено по [issue #81](https://github.com/rkfsociety/EvoHime/issues/81). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный **Knowledge Source Registry** для управляемого подключения reference-контента к проектам, ролям и workflow: документации, спецификаций, PDF, Markdown/Text, JSON/CSV и других индексируемых источников.

Knowledge должен быть отдельным слоем от Memory.

```text
Memory
  = накопленные факты, решения, предпочтения, lessons

Knowledge
  = явно подключённый reference corpus с известным источником,
    revision/fingerprint и retrieval provenance
```

Это позволит агенту опираться на большие локальные наборы материалов без помещения всех файлов в prompt и без превращения каждой страницы документации в долговременную memory.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/knowledge-source-registry-project-role.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 50.0 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy.
- План 68.0 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection.
- План 70.0 — Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits.
- План 86.0 — Semantic Repository Map: symbol graph и token-budgeted контекст большого репозитория.
- План 109.0 — Knowledge Source Registry: versioned RAG corpora, ingestion lineage и role-scoped retrieval.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Source identity, ingestion state, permissions и provenance принадлежат Core.

Модель может запросить retrieval или предложить подключить источник, но не должна сама объявлять произвольный внешний URL/файл trusted knowledge без обычной validation/policy.

### Безопасность

- source registration Core-owned;
- knowledge binding не расширяет grants;
- path canonicalization;
- index isolation по project/sensitivity;
- Secret data не индексируется в lower-sensitivity collection;
- model cannot choose unauthorized source IDs;
- web ingestion не bypass-ит network policy;
- parser не выполняет document scripts/macros;
- prompt content не становится policy instruction;
- retrieval provenance сохраняется end-to-end.

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

- [ ] Knowledge и Memory являются отдельными подсистемами.
- [ ] Есть versioned KnowledgeSource/Binding/View contracts.
- [ ] Поддерживается минимум несколько локальных source types.
- [ ] Retrieval работает только по authorized KnowledgeView.
- [ ] Hits содержат source revision + locator provenance.
- [ ] Source freshness/reindex определены явно.
- [ ] Backend retrieval provider-neutral.
- [ ] Role/project/workflow bindings поддерживаются.
- [ ] UI позволяет управлять sources и инспектировать retrieval.

## Ограничения и non-goals

- unrestricted web crawling;
- обязательная cloud vector DB;
- автоматическое индексирование всего диска;
- превращение любого knowledge hit в Memory;
- выполнение кода/macros из документов;
- публичный shared knowledge SaaS;
- использование vector similarity как authority/trust signal.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#81 Knowledge Source Registry: project/role RAG, source provenance и indexed reference context](https://github.com/rkfsociety/EvoHime/issues/81)
