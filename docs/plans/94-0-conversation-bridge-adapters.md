# План 94.0 — Conversation Bridge Adapters: безопасное управление EvoHime conversations из внешних chat threads

Статус: предложено по [issue #74](https://github.com/rkfsociety/EvoHime/issues/74). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Conversation Bridge Adapters**: Core-owned слой, который связывает внешнюю chat thread/conversation с конкретной EvoHime conversation и позволяет продолжать агентную работу через Telegram/Slack/Google Chat/другие поддерживаемые messaging providers без превращения каждого входящего сообщения в независимый webhook workflow.

Это не замена Event Trigger Runtime (#14) и не универсальный bot framework.

Главное различие:

```text
Event Trigger:
external event -> запускает predefined workflow

Conversation Bridge:
external human chat thread <-> существующая EvoHime conversation
```

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/conversation_bridge_adapters.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./94-1-conversation-bridge-adapters.md)
- [Этап 2 — runtime-интеграция и recovery](./94-2-conversation-bridge-adapters.md)
- [Этап 3 — IPC, client projection и UI](./94-3-conversation-bridge-adapters.md)
- [Этап 4 — verification, release-evidence и закрытие](./94-4-conversation-bridge-adapters.md)

## Зависимости

### Блокирующие

- План 67.0 — Schema-Driven Agent Configuration: Core-owned schemas для agent/conversation settings.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 74.0 — Declarative Agent Component Registry: versioned runtime components with schema-safe loading.
- План 76.0 — Safe UI Extension Framework: declarative pages, panels и themes без renderer authority.
- План 85.0 — Customization Inventory: единый каталог Skills, Integrations, Profiles, Workflows и UI Extensions.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- bridge built on Integration Provider credentials, raw tokens не видит renderer/model;
- sender identity authenticated/bound;
- provider retries deduplicated;
- external messages не являются system prompt;
- remote commands typed/allowlisted;
- arbitrary cwd/path/tool/provider selection запрещён;
- Secret outbound deny-by-default;
- approvals correlated с exact pending request;
- bridge не создаёт standing approval;
- workspace/grants не расширяются;
- attachments staged/validated;
- webhook authenticity проверяется Event/Provider ingress layer;
- pairing local-explicit и revoke-able;
- loss of bridge не ломает local conversation.

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

- [ ] Есть durable ConversationBridge + ThreadBinding contracts.
- [ ] Внешний thread может быть однозначно связан с EvoHime conversation.
- [ ] External principal проходит explicit pairing/auth binding.
- [ ] Inbound provider retries idempotent/deduplicated.
- [ ] Outbound messages используют bounded privacy-aware projections.
- [ ] Поддержаны typed attention/approval/Human Work Item replies.
- [ ] Remote commands ограничены allowlisted control surface.
- [ ] Workspace/capability/credential authority не расширяется через chat.
- [ ] Conversation остаётся полноценной локальной сущностью независимо от bridge lifecycle.

## Ограничения и non-goals

- публичный multi-user SaaS bot;
- arbitrary group-chat users с доступом к локальному агенту;
- remote shell;
- unrestricted `/cwd` host paths;
- передача полного event/tool log во внешний чат;
- standing blanket approvals через messaging reply;
- замена Event Trigger Runtime;
- замена desktop UI для сложной настройки и security policy;
- хранение external provider credentials внутри conversation.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#74 Conversation Bridge Adapters: безопасное управление EvoHime conversations из внешних chat threads](https://github.com/rkfsociety/EvoHime/issues/74)
