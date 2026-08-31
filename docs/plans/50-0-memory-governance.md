# План 50.0 — Memory Governance: typed memory, evidence gates, reinforcement и retention policy

Статус: предложено по [issue #30](https://github.com/rkfsociety/EvoHime/issues/30). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить поверх существующей памяти EvoHime отдельный **Memory Governance** слой: формальные типы памяти, критерии записи, provenance/evidence, правила противоречий, reinforcement/freshness и управляемое забывание.

Память агента должна быть не просто хранилищем удачных фраз из прошлых диалогов, а контролируемой системой знаний с понятной причиной, почему конкретная запись существует и насколько ей можно доверять.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Memory Governance расширяет существующие `memory_domain.rs`, `memory_api.rs`,
`memory_retrieval.rs`, `memory_extraction.rs` и
`evohime-local-storage/src/memory_store.rs`. Отдельный
`memory_governance.rs` допустим только как policy/gate слой над этими
контрактами: второй `MemoryRecord`, параллельный store или обход действующих
create/update/retrieval entrypoints запрещены. IPC, Electron bridge и inspector
добавляются после унификации существующих in-memory и durable представлений.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./50-1-memory-governance.md)
- [Этап 2 — runtime-интеграция и recovery](./50-2-memory-governance.md)
- [Этап 3 — IPC, client projection и UI](./50-3-memory-governance.md)
- [Этап 4 — verification, release-evidence и закрытие](./50-4-memory-governance.md)

## Зависимости

### Блокирующие

- реализованные Core memory API/domain/store/retrieval contracts и их
  каноническое описание в `../architecture.md` и `../current-state.md`;
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- Continual Refinement v1 может использовать governed memory как evidence, но
  без этой интеграции governance полностью работает, а refinement получает
  typed `unavailable` для новых evidence projections.
- TaskCheckpoint может ссылаться на governed memory refs после отдельной
  интеграции; отсутствие этой связи не блокирует write/read governance.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- model не назначает себе `SystemDefined` authority;
- Secret content хранится только по существующей secret/sensitivity policy;
- project memory не может незаметно стать global/user memory;
- retrieved memory не расширяет capabilities;
- malicious workspace text не становится user preference без достаточного evidence;
- source refs и raw evidence redaction-aware.

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

- [ ] MemoryRecord имеет typed kind/scope/durability/authority/confidence.
- [ ] Долговременные записи проходят MemoryWriteGate.
- [ ] Есть dedup/merge и explicit contradiction semantics.
- [ ] Reinforcement требует независимого evidence.
- [ ] Есть freshness и versioned retention policy.
- [ ] Retrieval сохраняет provenance/authority metadata.
- [ ] Пользователь может инспектировать и исправлять память.

## Ограничения и non-goals

- хранить полный raw chat навсегда;
- считать LLM confidence доказательством истины;
- cloud shared memory между пользователями;
- vector score как единственный критерий authority;
- автоматическое превращение всей памяти в system prompt;
- удаление пользовательских pinned facts без явной policy.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#30 Memory Governance: typed memory, evidence gates, reinforcement и retention policy](https://github.com/rkfsociety/EvoHime/issues/30)

## Результат ревью 2026-09-01

- План сопоставлен с issue #30 и live memory path; устранён риск второго
  `MemoryRecord`/store рядом с уже существующими memory domain/API/storage.
- Continual Refinement и TaskCheckpoint переведены в опциональные consumers;
  блокирующим основанием оставлен действующий memory contract.
