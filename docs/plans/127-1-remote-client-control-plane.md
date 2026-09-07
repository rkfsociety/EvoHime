# План 127.1 — Relay-контракт, installer, registry и storage

Статус: этап 1 для [плана 127.0](./127-0-remote-client-control-plane.md); реализация не начата.

## Зависимости

### Блокирующие

- План 127.0 и текущие Core/IPC/security boundaries.
- Evidence freeze: проверить workspace topology, storage schema, highest IPC tags и способ упаковки server artifact.

### Опциональные

- #102/#104 и diagnostics; отсутствие этих систем даёт typed degraded evidence.

## Реализация

Спроектировать versioned bounded relay protocol для pair, device_register,
device_list, chat_start, chat_delta, chat_complete, cancel, ping и ошибок.
Зафиксировать размеры, rate limits, TTL, correlation/idempotency, ordering и
reconnect/resume semantics. Разделить owner, device identity, session и
revocable credential; хранить только хеши токенов и metadata, не raw prompts или secrets.

Создать отдельный relay package и idempotent installer/bootstrap. Installer
работает по явному SSH-доступу, создаёт service user с минимальными правами,
конфигурацию и service unit, настраивает только нужный firewall-порт, выдаёт
одноразовый owner pairing secret и выполняет health-check. Root/SSH credential
не попадает в модель, логи или runtime; предусмотрены повторный запуск, rollback
и безопасное удаление. Сервер поддерживает статичный IP без обязательного домена;
сертификатный trust, pinning и rotation описываются до клиента.

## Выходные артефакты

- protocol/schema с compatibility/version policy;
- relay package, конфигурация и installer;
- migration/storage contract для owner/device/token/session metadata;
- redacted operator guide для установки, rotation, revoke и recovery.

## Критерии выхода

- [ ] Installer не выполняет произвольный model-generated shell и повторный запуск не ломает конфигурацию.
- [ ] Relay отказывает без valid token/pairing, не считает IP authentication и не выдаёт секреты в ошибках.
- [ ] Storage транзакционный, secret-safe, bounded и recoverable.
- [ ] Protocol имеет explicit incompatibility, expiry, replay и oversize errors.
- [ ] Есть contract/property tests для parser, token hashing, pairing TTL, rate-limit и idempotent installer fixture.

## Rollback и остановка

При неизвестной ОС, неподтверждённом trust или неполной миграции установка
останавливается до изменения рабочего relay. Неуспешный bootstrap удаляет только
созданные им ресурсы по journal; существующий сервис не заменяет.
