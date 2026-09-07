# План 127.2 — Remote Client Control Plane: runtime-интеграция и recovery

Статус: этап 2 для [плана 127.0](./127-0-remote-client-control-plane.md); issue: [#108](https://github.com/rkfsociety/EvoHime/issues/108).

## Зависимости

### Блокирующие

- План 127.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Создать PC connector как отдельный ограниченный runtime-компонент. Он держит исходящее TLS-соединение к статичному relay IP, регистрирует device_id и capabilities, переподключается после смены локального IP и передаёт текстовые команды через существующий authenticated Core IPC. Connector не читает SQLite напрямую и не получает произвольный shell authority.

Добавить Core-owned remote request/session state: immutable request id, client identity, selected device, cancellation, timeout, approval/policy decision, stream sequence, duplicate suppression и explicit Unknown/Disconnected result. Restart relay, connector или Core не должен создавать duplicate effect. Для нескольких PC сначала реализовать выбор конкретного устройства; пул независимых запросов — опционально, distributed inference одной модели — вне scope.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.
- [ ] Покрыты reconnect, stale token, replay, duplicate frame, ordering, timeout, cancellation, Core unavailable и supervisor recovery.
- [ ] Connector работает без root и не меняет установленный клиент EvoHime.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных. При ошибке handshake connector остаётся offline/read-only.
