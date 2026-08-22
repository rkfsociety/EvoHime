# План 10 — IPC, version negotiation и provider boundary

## Цель

Закрепить одну проверяемую границу между renderer, Electron main,
authenticated Core IPC и Core-owned provider/worker adapters. План расширяет
существующий `desktop-ipc-v1`, а не создаёт второй transport или второй
provider contract.

## Что уже есть в checkout

- `Handshake`, `AuthChallenge`, `ProtocolOffer`, `Ready`, `ResyncRequest`,
  `ReplayGap` и `FullSnapshot` в
  `crates/desktop-ipc/proto/evohime.desktop.proto`; `core_instance_id` и
  `session_epoch` лежат в `EventEnvelope` (поля 6 и 7), а не в `Ready`;
- Rust negotiation и bounded limits в `crates/desktop-ipc/src/lib.rs`:
  major compatibility, minor downgrade, capability intersection, 4 MiB
  frame, bounded replay/resync;
- Electron `CorePipeClient`, `protocol-version.ts`, `command-queue.ts` и
  `frame-codec.ts`, которые уже владеют handshake, reconnect, sequence,
  epoch-change и очередью команд;
- provider contract в `crates/model-gateway/src/provider_contract.rs`:
  `CapabilityMetadata`, `RoutePolicySnapshot`, `CandidateHealthSnapshot` и
  `RunHealthOverlay`;
- Core-owned adapters: `NodeAdapter` в
  `crates/evohime-core/src/workflow_runtime.rs`, `CoreNodeAdapter` в
  `crates/evohime-core/src/workflow_adapters.rs` и `LocalAdapterProcess` в
  `crates/evohime-supervisor/src/local_provider.rs`;
- `provider.*` в Electron main: это локальная оболочка над `provider.json`,
  DPAPI/safeStorage и supervisor restart. Эти команды не являются
  provider/worker transport и не передают ключ renderer'у.

## Решения, зафиксированные ревью

1. `CoreInfo` добавляется аддитивно в `Ready`; существующие поля envelope
   остаются совместимыми для WinUI compatibility runtime.
2. `core_instance_id + session_epoch` — идентичность Core generation,
   `sequence_id` — ревизия event journal, а `target_generation` — отдельная
   ревизия выбранного workspace/route/backend. Их нельзя заменять одним
   общим `revision`.
3. Provider model gateway и worker adapters — внутренние Core boundaries.
   Renderer не получает их descriptors и не вызывает их напрямую.
4. Существующие `CapabilityMetadata` и routing snapshot остаются источником
   истины для model providers. `ProviderAdapterInfo` допускается только как
   bounded transport/view над ними, но не как второй независимый каталог.
5. Изменение provider credentials по-прежнему делает supervisor/Core
   restart (`provider.save`/`provider.clearKey` в
   `desktop/evohime-electron/src/main/shell-bridge.ts` возвращают
   `{ summary, restarted }`). Внутри живой сессии можно менять только
   разрешённые route/model hints: `SelectModelRequest` по контракту proto
   применяется без рестарта Core и не является hot-swap credentials.

6. Новые внутренние contracts (`adapter/v1`, `ActiveTarget`) живут в Rust и
   не создают второй wire transport. В desktop proto попадают только те
   поля, которые реально пересекают границу main ↔ Core, аддитивно и без
   переиспользования уже занятых field numbers.

## Границы

Входит: additive `CoreInfo`, negotiation и effective limits, adapter-only
transport, versioned adapter descriptors, capability-scoped sessions,
workspace target identity, stale projection cleanup и deterministic acceptance.

Не входит: прямой renderer pipe/HTTP, provider access к SQLite или raw
workspace, передача ключей через IPC, новый transport major, hot-swap
provider credentials, внешний runtime без supervisor и реальный сетевой
provider в deterministic tests.

## Зависимости

### Блокирующие

- контракты планов 08–09 после их принятия: event sequence/replay, Core
  generation, execution linkage, capability snapshot, policy и approval;
- текущие authenticated desktop IPC, Electron main adapter и model gateway
  provider contract.

### Опциональные

- внешние adapters подключаются только после собственных release gates. До
  этого generic adapter возвращает typed `adapter_unavailable` и не пытается
  запускать внешний runtime;
- catalog metadata плана 07-2. Без неё descriptor использует identity/hash
  уже зарегистрированного Core manifest или routing entry.

## Этапы

- [10-1 — CoreInfo и version negotiation](10-1-core-info-negotiation.md)
- [10-2 — adapter boundary и provider settings](10-2-adapter-provider-boundary.md)
- [10-3 — target scope и stale projection](10-3-target-projection-lifecycle.md)
- [10-4 — acceptance и compatibility closure](10-4-ipc-provider-acceptance.md)

Порядок: 10-1 → 10-2 → 10-3 → 10-4.

## Готово, когда

Совместимость проверяется до рабочей команды, transport calls проходят через
один main/Core adapter, provider и worker получают только разрешённый
session context, fallback остаётся внутри активного target, а смена target
или Core generation не применяет старую projection и не повторяет внешний
эффект вслепую.
