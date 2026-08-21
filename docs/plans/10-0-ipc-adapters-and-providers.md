# План 10 — IPC, version negotiation и provider boundary

## Цель

Закрепить границы между renderer, Electron main, authenticated Core IPC и
внешними provider/worker adapters без ad-hoc transport calls и скрытого
расхождения версий.

## Что уже есть в checkout

- versioned protobuf over authenticated named pipe;
- Electron main IPC adapter и generated protocol types;
- Core-owned policy, capabilities, workspace scope и secret references;
- Core session/revision и bounded replay из планов 08–09;
- provider routing и supervisor-owned provider configuration.

План 10 упорядочивает границы и состояния, не возвращая HTTP control plane и
не давая renderer прямой доступ к Core, SQLite или provider.

## Границы

Входит: typed `CoreInfo`, version negotiation, adapter-only transport,
versioned provider/worker settings, capability discovery, workspace target,
secret references и stale projection cleanup.

Не входит: прямой renderer IPC/HTTP, provider access к SQLite или workspace,
обход supervisor secrets, неограниченный внешний runtime или новая transport
major-версия без compatibility review.

## Зависимости

### Блокирующие

- план 08 для event sequence/replay и Core revision;
- план 09 для capability snapshot, policy и secret scope;
- текущие authenticated desktop IPC, Electron main adapter и provider routing.

### Опциональные

- MCP/browser/voice/vision adapters подключаются после своих release gates;
  до этого `ProviderAdapterInfo` возвращает typed `adapter_unavailable`;
- catalog metadata из плана 07-2 необязательна: adapter использует manifest
  identity/hash.

## Этапы

- [10-1 — CoreInfo и version negotiation](10-1-core-info-negotiation.md)
- [10-2 — adapter boundary и provider settings](10-2-adapter-provider-boundary.md)
- [10-3 — target scope и stale projection](10-3-target-projection-lifecycle.md)
- [10-4 — acceptance и compatibility closure](10-4-ipc-provider-acceptance.md)

Порядок: 10-1 → 10-2 → 10-3 → 10-4.

## Готово, когда

Совместимость проверяется до запуска сессии, transport calls централизованы в
adapters, ошибки версии типизированы, provider/worker получает только
capability-scoped session, а смена target/provider/backend атомарно очищает
старую projection и не отправляет команду в старый runtime.
