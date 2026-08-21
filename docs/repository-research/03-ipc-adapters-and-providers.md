# 03. IPC, version negotiation и provider boundary

## Цель

Закрепить границы между renderer, Electron main, authenticated Core IPC и
внешними provider/worker adapters без ad-hoc transport calls и скрытого
расхождения версий.

## Scope

- typed `CoreInfo` с protocol major, build/runtime revision, capabilities,
  feature flags и limits;
- явные состояния `unavailable`, `unsupported`, `unknown`, stale session;
- adapter-only transport на каждом уровне;
- versioned provider/worker settings и capability discovery;
- передача workspace scope и secret references через доверенную сторону;
- stale projection cleanup при смене target/provider/backend.

## Инварианты

- Renderer обращается только к Electron/main adapter.
- Main обращается только к authenticated desktop IPC.
- Core единолично решает, какой инструмент, provider и workspace доступны.
- Provider adapter не получает прямой доступ к SQLite, supervisor secrets или
  произвольному workspace.
- Смена target атомарно меняет query scope и не отправляет команду в старый
  runtime.
- Кэш capability сбрасывается после restart или смены Core revision.

## Возможные контракты

- `CoreInfo`;
- `CapabilitySnapshot`;
- `ProviderAdapterInfo`;
- `WorkspaceTarget`;
- `TransportError`;
- `StaleSession`;
- `RuntimeService`.

## Тестовый контур

- major-version mismatch и unsupported feature;
- reconnect/replay при перезапуске Core;
- provider unavailable и fallback без отправки команды неактивному target;
- запрет прямого IPC/HTTP вызова из renderer;
- path/secret scope на границе worker adapter;
- stale projection после смены provider/workspace.

## Критерии готовности

- совместимость проверяется до запуска сессии;
- transport calls централизованы в adapters;
- ошибки версии и состояния типизированы и видны UI;
- provider/worker получает только capability-scoped session;
- contract tests фиксируют major protocol и sequence replay.

## Зависимости

Требует 01 и 02. Закрытие этого раздела открывает 04–09.
