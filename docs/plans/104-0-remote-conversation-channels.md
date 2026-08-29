# План 104.0 — Remote Conversation Channels: безопасное управление Евой через Telegram, Slack и другие мессенджеры

Статус: предложено по [issue #84](https://github.com/rkfsociety/EvoHime/issues/84). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Remote Conversation Channels**: Core-owned слой, позволяющий владельцу безопасно привязать внешний чат/бот-канал к своей локальной EvoHime и продолжать обычную conversation/run из Telegram, Slack, Discord или другого поддерживаемого транспорта.

Это не Event Trigger Runtime (#14). Trigger запускает automation по событию. Conversation Channel является **интерактивной пользовательской поверхностью** с identity binding, входящими сообщениями, streaming/final replies, файлами и ограниченным human-input/approval UX.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/remote_conversation_channels.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./104-1-remote-conversation-channels.md)
- [Этап 2 — runtime-интеграция и recovery](./104-2-remote-conversation-channels.md)
- [Этап 3 — IPC, client projection и UI](./104-3-remote-conversation-channels.md)
- [Этап 4 — verification, release-evidence и закрытие](./104-4-remote-conversation-channels.md)

## Зависимости

### Блокирующие

- План 94.0 — Conversation Bridge Adapters: безопасное управление EvoHime conversations из внешних chat threads.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 34.0 — Event Trigger Runtime: безопасный запуск workflow по внешним событиям.
- План 40.0 — Sensitive Data Guardrails: PII/secret detection и streaming redaction на model/tool boundaries.
- План 98.0 — Durable Remote Task Bridge: submit/status/cancel protocol для долгих tool и MCP операций.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Provider Bot/Transport
  -> Channel Adapter
  -> bounded admission queue
  -> Identity Binding
  -> Core Conversation Channel Service
  -> existing Conversation / Goal / Workflow runtime
  -> safe outbound projection
  -> Provider
```

Channel adapter не имеет прямого доступа к model runtime, DB, workspace или credentials других providers.

### Безопасность

- external identity всегда bound Core-side;
- pairing code TTL + single-use;
- per-message ownership recheck;
- inbound dedup;
- bounded queue/rate limits;
- attachment size/path/network controls;
- external text является untrusted user content, не system instruction;
- channel не расширяет workspace/tool capabilities;
- high-risk approval desktop-only по default;
- provider bot token не хранится в config plaintext;
- outbound projection redaction-aware;
- revoke немедленно запрещает дальнейшие messages;
- channel adapter не получает direct Core DB access.

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

- [ ] Есть versioned ChannelProvider/ChannelConnection contracts.
- [ ] Подключение использует short-lived single-use pairing.
- [ ] External identity однозначно привязан к owner.
- [ ] Inbound queue/rate/attachment limits bounded.
- [ ] Conversation routing и dedup Core-owned.
- [ ] Есть streaming/final reply abstraction.
- [ ] Human Work Item можно безопасно закрыть из поддерживаемого channel.
- [ ] High-risk remote approvals deny-by-default.
- [ ] Provider credentials и outbound data проходят обычные sensitive-data boundaries.
- [ ] Connections переживают restart и могут быть немедленно revoked.

## Ограничения и non-goals

- публичная multi-tenant bot SaaS платформа;
- обязательный cloud relay;
- remote shell console;
- отправка каждого internal event в мессенджер;
- unrestricted file transfer прямо в workspace;
- автоматическое high-risk approval по сообщению «да»;
- замена desktop UI;
- поддержка всех мессенджеров одновременно: начать с 1–2 adapters и общего contract.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#84 Remote Conversation Channels: безопасное управление Евой через Telegram, Slack и другие мессенджеры](https://github.com/rkfsociety/EvoHime/issues/84)
