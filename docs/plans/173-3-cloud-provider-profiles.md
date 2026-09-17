# План 173.3 — Cloud Provider Profiles: IPC, projection и UI

## Зависимости

### Блокирующие

- [Runtime и recovery](./173-2-cloud-provider-profiles.md).
- Канонический desktop IPC proto, `model.catalog` event, `ProviderForm`,
  `ModelPicker` и provider store.

### Опциональные

- Existing diagnostics/model resilience panels для повторного использования
  redacted status components.

## Реализация

- Добавить additive authenticated IPC projection для provider profiles,
  catalog freshness, capabilities, limits, privacy class, health и credential
  status; использовать свободные command/event tags после проверки proto.
- Существующий `model.catalog` остаётся совместимой проекцией единого
  Core-owned catalog. Новые поля добавляются совместимо либо вводится явно
  версионированная typed projection; второй независимый catalog event не
  создаётся.
- Обновить generated Rust/TypeScript bindings и main bridge. Renderer не
  читает SQLite, workspace или provider API и не решает route compatibility.
- В `ProviderForm` и `ModelPicker` показывать provider/model, capability
  filters, catalog state, free metadata как advisory и понятные `Unknown` /
  `NeedsCredential` / `NeedsModelResolution` состояния.
- Ограничить payload size/count, redaction и replay semantics; raw metadata,
  headers, API keys, prompts и response bodies в UI не передавать.

## Критерии

- UI отображает фактическую Core projection и не может подделать provider,
  capability, privacy или cost decision.
- Старый model catalog event и существующие Ollama/Codex flows сохраняют
  совместимость.
- Malformed/stale/incompatible major IPC payloads отвергаются fail-closed.

## Non-goals

Renderer-owned provider clients, secret editing вне существующей credential
surface и новый developer-only navigation route.
