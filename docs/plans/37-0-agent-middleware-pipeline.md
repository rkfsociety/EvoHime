# План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution

Статус: предложено по [issue #17](https://github.com/rkfsociety/EvoHime/issues/17). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime versioned **Agent Middleware Pipeline**: Core-owned слой перехватчиков вокруг основных фаз agent loop, чтобы routing, redaction, retries, context shaping, metrics и другие cross-cutting правила не были размазаны по gateway/tools/workflow коду.

Middleware не создаёт новый agent runtime и не получает собственные полномочия. Он работает только внутри уже разрешённого run и может **сужать/преобразовывать** запрос, наблюдать результат либо остановить выполнение по policy.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/agent-middleware-pipeline.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./37-1-agent-middleware-pipeline.md)
- [Этап 2 — runtime-интеграция и recovery](./37-2-agent-middleware-pipeline.md)
- [Этап 3 — IPC, client projection и UI](./37-3-agent-middleware-pipeline.md)
- [Этап 4 — verification, release-evidence и закрытие](./37-4-agent-middleware-pipeline.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Минимально поддержать:

```text
before_agent
after_agent
before_model
after_model
wrap_model_call
before_tool
wrap_tool_call
after_tool
```

`before_*` / `after_*` применяют ограниченные typed state/request/result updates.

`wrap_*` получает request + `next()` handler и может:

- вызвать следующий слой один раз;
- выполнить bounded retry, если его тип middleware это разрешает;
- заменить request через immutable override;
- short-circuit только в явно разрешённых сценариях, например policy block или test simulation.

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

- [ ] Есть versioned middleware contract.
- [ ] Есть hooks вокруг agent/model/tool phases.
- [ ] Requests изменяются через typed immutable override.
- [ ] Ordering deterministic и snapshot-ится на run.
- [ ] Middleware state имеет private/checkpoint/public classification.
- [ ] Middleware не может расширять grants/capabilities.
- [ ] Есть trace/failure policy.
- [ ] Built-in policies можно реализовывать поверх pipeline без специальных веток agent loop.

## Ограничения и non-goals

- сторонний executable middleware marketplace;
- выполнение middleware-кода из SKILL.md;
- arbitrary graph jumps;
- расширение capabilities через middleware;
- hot mutation уже идущего run;
- перенос security authority из Core в extension code.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#17 Agent Middleware Pipeline: typed hooks вокруг model/tool execution](https://github.com/rkfsociety/EvoHime/issues/17)
