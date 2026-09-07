# План 127.3 — Remote Client Control Plane: IPC, projection и UI

Статус: этап 3 для [плана 127.0](./127-0-remote-client-control-plane.md); issue: [#108](https://github.com/rkfsociety/EvoHime/issues/108).

## Зависимости

### Блокирующие

- План 127.0 и предыдущий этап этого направления.
- Existing Core policy/capability/approval, SQLite, event/replay, provenance и authenticated IPC boundaries.

### Опциональные

- #102 Verification Evidence Ledger, #104 Project Quality Contract и diagnostics; без них результат остаётся explicit Unknown/degraded.

## Реализация

Создать Android thin client: статичный IP/порт relay, TLS trust/pinning, одноразовое QR/код-сопряжение, токен в Android Keystore, device naming, logout/revoke и credentials rotation. Приложение получает список online/offline устройств, показывает last-seen и позволяет выбрать конкретную Еву. Поток текста имеет bounded buffering, sequence/replay, cancel и reconnect.

Добавить additive authenticated IPC commands/events после проверки highest tag, correlation/idempotency, replay/resync и bounded errors. Проецировать только redacted metadata и настройки connector/pairing; renderer не вычисляет verdict, не пишет storage и не получает secrets/raw payloads.

## Критерии выхода

- [ ] Все material transitions типизированы, bounded и проверяются Core.
- [ ] Ошибки, stale/conflict/restart и отсутствие evidence дают безопасный non-success verdict.
- [ ] Нет обхода существующих authority, секретов или raw user data.
- [ ] Есть воспроизводимые tests/evidence для acceptance criteria.
- [ ] Несколько Android работают одновременно с независимыми tokens; IP-only, просроченный или отозванный token не даёт доступа.
- [ ] Смена IP телефона или ПК не требует ручной перенастройки; offline/reconnect не дублирует сообщение.
- [ ] Проверены certificate rotation, malformed frames, accessibility и logout/revoke.

## Не входит

Новая параллельная authority, arbitrary shell/network execution, silent policy relaxation, renderer-owned business logic и автоматическая публикация данных. Файлы, изображения и push notifications — вне базовой версии.
