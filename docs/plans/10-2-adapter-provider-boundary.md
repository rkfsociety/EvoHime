# 10-2 — Adapter boundary и provider settings

## Цель

Сделать каждый transport и provider/worker вызов проходящим через проверяемый
adapter, сохранив Core владельцем доступа и capability decision.

## Изменения

1. Централизовать renderer → Electron main и main → authenticated Core IPC
   calls в adapter interfaces; запретить ad-hoc pipe/HTTP calls из renderer.
2. Ввести versioned `ProviderAdapterInfo` с provider identity, protocol,
   capabilities, limits, health и typed availability state.
3. Передавать provider/worker только `CapabilitySnapshot`, workspace scope и
   secret references; исключить прямой SQLite, supervisor secret и arbitrary
   workspace access.
4. Валидировать provider/worker settings и capability discovery до dispatch;
   неизвестную версию или missing capability возвращать typed error.
5. Сохранять fallback без отправки команды в неактивный или stale target.

## Проверки

- provider unavailable и deterministic fallback;
- unsupported adapter version и capability mismatch;
- secret/path scope на worker boundary;
- запрет прямого IPC/HTTP вызова из renderer;
- adapter calls с bounded timeout/cancellation.

## Готово, когда

Ни один provider или worker не получает доступ за пределами выданной session,
а Core является единственным источником решения о доступности инструмента,
provider и workspace.
