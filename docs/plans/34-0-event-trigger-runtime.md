# План 34.0 — Event Trigger Runtime: безопасный запуск workflow по внешним событиям

Статус: предложено по [issue #14](https://github.com/rkfsociety/EvoHime/issues/14). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Event Trigger Runtime**: durable слой, который запускает заранее разрешённые workflow по внешним или локальным событиям, а не только вручную или по `once/interval` schedule.

Примеры:

- открыт GitHub PR;
- пришёл webhook от внешнего сервиса;
- изменился файл/ветка в выбранном workspace;
- завершился CI run;
- integration provider прислал поддерживаемое событие.

Trigger не является новым способом передать модели произвольный prompt. Он связывает typed event со snapshot конкретного workflow и его входами.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/event_trigger_runtime.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./34-1-event-trigger-runtime.md)
- [Этап 2 — runtime-интеграция и recovery](./34-2-event-trigger-runtime.md)
- [Этап 3 — IPC, client projection и UI](./34-3-event-trigger-runtime.md)
- [Этап 4 — verification, release-evidence и закрытие](./34-4-event-trigger-runtime.md)

## Зависимости

### Блокирующие

- План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.
- Для provider-backed webhook MVP — provider trigger capability, validation strategy, credential reference и subscription adapter из плана 33; локальные источники не могут подменять этот контракт.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
External/local event
  -> Trigger Adapter / Ingress
  -> validation + authentication
  -> normalization
  -> dedup/idempotency
  -> Trigger Registry
  -> input mapping validation
  -> Workflow Runtime
```

Все side effects дальше выполняются обычным workflow runtime с теми же grants, approvals, budgets и recovery semantics.

### Обязательные варианты источников и lifecycle

- Contract должен различать `integration_webhook`, `local_workspace_event` и `system_event`; `custom_local_ingress` допускается только как явно optional и строго bounded вариант.
- Provider-backed trigger хранит subscription state отдельно от portable definition и поддерживает typed lifecycle `Draft → Connecting → Active → Paused/Broken → Revoked/Deleted`; credential revoke/remove переводит trigger в `Broken` или `Paused`.
- Local workspace source ограничивается конкретным workspace root, разрешёнными event kinds, debounce/coalescing, ignore patterns и max event rate.
- Accepted-but-not-dispatched events, bounded dedup journal, rate counters/circuit state и reconnect/error state переживают restart; missed external events не синтезируются.
- Loop prevention обязана использовать origin/correlation marker, max chain depth и fingerprint suppression window; storm protection имеет bounded queue/overflow policy и typed outcomes `Throttled`, `DroppedWithAudit`, `Coalesced`, `PausedByCircuitBreaker`.
- Declarative filters, если входят в MVP, должны быть deterministic, bounded и без I/O; arbitrary expression language и scripts остаются non-goal.

### Безопасность

- inbound payload считается недоверенным;
- строгий size limit до JSON parse;
- content type/schema validation;
- authenticity проверяется до workflow enqueue;
- payload не может задавать tool/provider identity;
- input mapping allowlisted;
- secrets не входят в event projection;
- trigger не расширяет workflow grants;
- imported Workflow Package не активирует trigger автоматически;
- network listener/ingress должен быть opt-in и иметь чёткий binding/firewall story;
- local-only default предпочтителен для desktop продукта.

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

- [ ] Есть versioned TriggerDefinition.
- [ ] Есть normalized EventEnvelope.
- [ ] Webhook authenticity и schema проверяются до enqueue.
- [ ] Есть dedup/replay protection.
- [ ] Workflow version pinned.
- [ ] Input mapping ограничивает payload.
- [ ] Есть rate limits/circuit breaker.
- [ ] State durable/recoverable.
- [ ] Existing workflow approvals/grants сохраняются.

## Ограничения и non-goals

- публичный Internet-facing automation service по умолчанию;
- arbitrary code filters/transforms;
- exactly-once гарантия внешнему миру;
- standing approval для любых будущих действий;
- автоматическое создание trigger из импортированного файла;
- замена scheduler;
- unlimited event queues.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#14 Event Trigger Runtime: безопасный запуск workflow по внешним событиям](https://github.com/rkfsociety/EvoHime/issues/14)
