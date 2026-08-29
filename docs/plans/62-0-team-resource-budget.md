# План 62.0 — Team Resource Budget: shared cost envelope, per-role allocations и reserved verification budget

Статус: предложено по [issue #42](https://github.com/rkfsociety/EvoHime/issues/42). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Team Resource Budget**: общий Core-owned ресурсный envelope для multi-agent/team execution с per-role/per-phase allocations, hard limits, reserve и детальной attribution фактического расхода.

Persistent Goal уже может иметь общий token/cost budget. Новый слой отвечает на следующий уровень:

> Как этот бюджет безопасно распределить между несколькими ролями и не позволить одной ветке съесть всё до reviewer/tester стадии?

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/team-resource-budget.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./62-1-team-resource-budget.md)
- [Этап 2 — runtime-интеграция и recovery](./62-2-team-resource-budget.md)
- [Этап 3 — IPC, client projection и UI](./62-3-team-resource-budget.md)
- [Этап 4 — verification, release-evidence и закрытие](./62-4-team-resource-budget.md)

## Зависимости

### Блокирующие

- План 25.0 — Persistent Goals: durable цели для долгих задач.
- План 26.0 — Continuation Policy: bounded autonomous loops и quality gates.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

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

- [ ] Есть versioned TeamBudgetPolicy.
- [ ] Есть shared TeamBudgetState и per-role/per-phase allocations.
- [ ] Поддерживаются soft/hard limits.
- [ ] Есть protected verification/recovery reserve.
- [ ] Reallocation Core-controlled и auditable.
- [ ] Все model/tool/internal calls дают usage attribution.
- [ ] Budget hierarchy не позволяет child/team превысить parent envelope.
- [ ] Budget state durable/recoverable.

## Ограничения и non-goals

- бухгалтерия/инвойсинг провайдеров;
- точное предсказание стоимости до вызова;
- автоматическая покупка credits;
- изменение security grants ради экономии;
- скрытый provider fallback вопреки policy;
- использование среднего team score как оправдание превышения hard budget.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#42 Team Resource Budget: shared cost envelope, per-role allocations и reserved verification budget](https://github.com/rkfsociety/EvoHime/issues/42)
