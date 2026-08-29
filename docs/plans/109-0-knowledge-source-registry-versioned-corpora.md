# План 109.0 — Knowledge Source Registry: versioned RAG corpora, ingestion lineage и role-scoped retrieval

Статус: предложено по [issue #89](https://github.com/rkfsociety/EvoHime/issues/89). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный Core-owned **Knowledge Source Registry** для подключаемых справочных корпусов: документов, текстов, таблиц, JSON, web-derived artifacts и других явно добавленных источников, которые индексируются и доступны агентам через bounded retrieval.

Knowledge должен быть отделён от Memory.

```text
Knowledge
  = curated/imported source material

Memory
  = факты, решения, опыт и выводы, накопленные во время работы
```

Это позволит давать Еве и отдельным ролям устойчивую предметную базу без постоянного помещения целых документов в model context.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/knowledge-source-registry-versioned-corpora.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 101.0 — Knowledge Source Registry: project/role RAG, source provenance и indexed reference context.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 68.0 — Experience Replay Library: episodic trajectories, success/failure retrieval и context injection.
- План 70.0 — Code Diagnostics Feedback Loop: LSP/compiler evidence и regression delta после agent edits.
- План 86.0 — Semantic Repository Map: symbol graph и token-budgeted контекст большого репозитория.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
KnowledgeSource {
  id,
  version,
  name,
  kind,
  source_ref,
  source_revision,
  scope,
  sensitivity,
  ingestion_policy,
  parser_profile,
  embedding_profile?,
  status,
  content_hash,
  created_at,
  updated_at
}
```

Виды первого этапа:

```text
Text
Markdown
PdfArtifact
CsvArtifact
JsonArtifact
DirectorySnapshot
WebSnapshotArtifact
GenericArtifact
```

Не привязывать публичный contract к конкретной vector DB или parser library.

### Безопасность

- source content считается untrusted data, не instructions;
- retrieval не расширяет capabilities;
- role/team knowledge является subset разрешённого view;
- Secret/Sensitive content фильтруется до external embedding/model boundaries;
- external URLs сначала materialize-ятся как snapshot/artifact через разрешённый fetch layer;
- parsers не получают произвольные execution privileges;
- source path canonicalized и scoped к разрешённым roots;
- raw knowledge не попадает автоматически в telemetry/support bundle;
- model не может зарегистрировать новый source/provider самостоятельно.

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

- [ ] Есть отдельные KnowledgeSource/Collection contracts.
- [ ] Ingestion создаёт versioned provenance-aware index.
- [ ] Retrieval bounded по results/token budget.
- [ ] Knowledge можно привязать к project/role/team/run scope.
- [ ] Source revision/freshness фиксируются end-to-end.
- [ ] Semantic backend не является обязательной архитектурной зависимостью.
- [ ] Sensitive knowledge obeys provider/locality policy.
- [ ] Knowledge и Memory остаются разными слоями с explicit evidence links.

## Ограничения и non-goals

- облачная multi-tenant knowledge base;
- бесконтрольный crawl интернета;
- автоматическое выполнение инструкций из retrieved документов;
- помещение всего corpus в prompt;
- использование vector score как security/authority signal;
- автоматическая запись каждого retrieved chunk в Memory;
- обязательная зависимость от конкретной vector DB.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#89 Knowledge Source Registry: versioned RAG corpora, ingestion lineage и role-scoped retrieval](https://github.com/rkfsociety/EvoHime/issues/89)
