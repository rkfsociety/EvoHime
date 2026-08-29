# План 86.0 — Semantic Repository Map: symbol graph и token-budgeted контекст большого репозитория

Статус: предложено по [issue #66](https://github.com/rkfsociety/EvoHime/issues/66). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Semantic Repository Map**: компактное представление структуры и взаимосвязей большого кодового репозитория, которое помогает модели ориентироваться в коде без загрузки полного содержимого всех файлов в context window.

Map должна строиться из реального состояния workspace, быть revision-aware, ограничиваться токен-бюджетом и ранжировать наиболее важные symbols/files относительно текущей задачи.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/semantic_repository_map.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./86-1-semantic-repository-map.md)
- [Этап 2 — runtime-интеграция и recovery](./86-2-semantic-repository-map.md)
- [Этап 3 — IPC, client projection и UI](./86-3-semantic-repository-map.md)
- [Этап 4 — verification, release-evidence и закрытие](./86-4-semantic-repository-map.md)

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 60.0 — Revision-Safe Workspace Files: uploads/workspace/outputs namespaces и stale-write protection.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 43.0 — Execution Backend Registry: несколько agent backends, health и capability handshake.
- План 75.0 — Typed Context References: адресные @refs на файлы, diff, diagnostics, terminal и artifacts.
- План 82.0 — Context Mentions: typed @references для files, folders, git, diagnostics и runtime resources.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
RepositoryMapSnapshot {
  id,
  workspace_binding_id,
  workspace_fingerprint,
  parser_registry_version,
  files_indexed,
  symbols_count,
  edges_count,
  generated_at,
  content_hash
}
```

Map является derived index. Авторитетным состоянием остаются реальные workspace files.

### Безопасность

- индексируется только разрешённый workspace scope;
- paths canonicalized Core-side;
- secret/excluded files следуют sensitivity/index policy;
- raw secret literals не должны попадать в signatures/map projection;
- repository content считается untrusted data, не instructions;
- map relevance не расширяет read/write grants;
- parser crash не приводит к arbitrary code execution;
- external parser processes, если появятся, идут через ExecutionPolicy.

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

- [ ] Есть revision-aware RepositoryMapSnapshot.
- [ ] Есть parser/tag registry минимум для основных языков EvoHime use cases.
- [ ] Definitions/references образуют bounded file/symbol graph.
- [ ] Есть relevance ranking с focus/mention signals.
- [ ] Projection соблюдает explicit token budget.
- [ ] Index обновляется incremental по file hash/revision.
- [ ] Map интегрируется с ContextRefs/Impact Analysis без расширения capabilities.
- [ ] Model-call provenance фиксирует map snapshot/query hash.

## Ограничения и non-goals

- полноценный compiler semantic database для всех языков;
- индексирование всего диска;
- замена LSP/IDE;
- автоматическое изменение файлов по graph inference;
- graph database как новый source of truth;
- giant visual dependency explorer;
- доверие stale map вместо чтения актуального кода.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#66 Semantic Repository Map: symbol graph и token-budgeted контекст большого репозитория](https://github.com/rkfsociety/EvoHime/issues/66)
