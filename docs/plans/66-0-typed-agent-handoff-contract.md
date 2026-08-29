# План 66.0 — Typed Agent Handoff Contract: explicit transfer of task ownership and context

Статус: предложено по [issue #46](https://github.com/rkfsociety/EvoHime/issues/46). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Ввести в EvoHime отдельный **typed handoff contract** для явной передачи ownership задачи между агентами/ролями/child contexts.

Сейчас передачу работы легко свести к тексту вроде «передай это reviewer». Для durable multi-agent runtime этого недостаточно: Core должен понимать, кто кому что передал, почему, какие артефакты относятся к передаче и была ли она принята.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/typed-agent-handoff-contract.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 23.0 — Task Checkpoint: структурированное состояние задачи для compaction и recovery.
- План 27.0 — Retained Child Contexts: mailbox и повторное использование child agents.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 47.0 — Skill Trust Pipeline: deterministic scanning, contextual review и quarantine перед активацией.
- План 56.0 — Artifact Handoff Registry: typed deliverables, lineage и freshness для multi-agent работы.
- План 65.0 — Team Coordination Policies: pluggable routing for multi-agent collaboration.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

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

- [ ] Есть versioned `HandoffPacket`
- [ ] Есть state machine и ACK/NACK
- [ ] Context transfer структурирован и budget-aware
- [ ] Capabilities не наследуются автоматически
- [ ] Pending handoff переживает restart
- [ ] UI отображает полный lifecycle
- [ ] Provenance связывает source, transfer, target run и result

## Ограничения и non-goals

- Не заменять child spawn.
- Не передавать весь transcript всегда.
- Не разрешать handoff в произвольный внешний process/address.
- Не использовать handoff как обход approval boundary.
- Не смешивать handoff с обычным informational message: это именно transfer of ownership.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#46 Typed Agent Handoff Contract: explicit transfer of task ownership and context](https://github.com/rkfsociety/EvoHime/issues/46)
