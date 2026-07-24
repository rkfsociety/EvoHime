# Cloud sync: remote pull

## Цель

Третья волна `7.99`: замкнуть round-trip cloud sync через API — owner скачивает `BackupDump` с настроенного remote endpoint и идемпотентно восстанавливает его в локальную базу, без ручного переноса файлов и CLI.

## Выбранный подход

`POST /api/sync/pull` (owner-only, тот же feature gate `EVOHIME_FEATURE_CLOUD_SYNC`) выполняет `GET` на `EVOHIME_SYNC_URL` с тем же bearer из `EVOHIME_SYNC_TOKEN`, валидирует заголовок формата и вызывает существующий `restore_backup` (`7.99` wave 2) под identity текущего оператора. Новых сущностей восстановления не появляется: pull — это транспорт поверх готового restore-движка.

Приёмник симметричен push: то, что wave 1 положила `PUT`-ом, wave 3 забирает `GET`-ом с того же URL. Выбор remote остаётся конфигурацией развёртывания, не пользовательским вводом.

## Модель данных

`sync_runs` получает колонку `direction` (`push` / `pull`, default `push` для существующих строк). История pull-попыток пишется тем же механизмом: `running` → `success`/`failed`, `bytes_total` — размер скачанного дампа, `checksum` — его SHA-256. `GET /api/sync/status` начинает отдавать direction в runs без изменения формы остальных полей.

## Безопасность и ошибки

- Тот же HTTP-клиент, что в push: только `http`/`https`, redirect запрещён, таймаут.
- Размер ответа ограничен жёстким потолком (64 MiB); превышение — failed run без частичного восстановления.
- Ответ парсится строго как `BackupDump`; неизвестный `format`/`version` отклоняется до транзакции.
- Restore выполняется в одной транзакции (гарантия wave 2): сбой не оставляет частичных данных.
- Одновременно допускается один активный run на оператора независимо от направления: pull во время push (и наоборот) получает `409`.
- Токен и тело ответа не попадают в логи; ошибки remote усечены, как в push.
- Checksum-заголовок от remote, если присутствует (`X-EvoHime-Backup-Checksum`), сверяется с фактическим SHA-256 тела; расхождение — failed run.

## API

`POST /api/sync/pull` → `{ run, report }`, где `report` — счётчики `RestoreReport` (sessions/messages/tasks/steps/events/memory inserted/skipped). `503` без конфигурации, `409` при активном run, `403` для member.

## Реализационные границы

Вне волны: авто-sync по расписанию, инкрементальные дампы, выбор remote через UI, merge-стратегии конфликтов (по-прежнему skip by id). Авто-sync ляжет поверх pull/push как отдельная малая волна через существующий scheduler.

## Проверка

- unit-тесты: лимит размера, сверка checksum, direction-константы;
- storage-тесты: direction в lifecycle runs;
- полный Rust workspace test, Clippy, регенерация OpenAPI, frontend build.

## Критерий готовности

Два экземпляра EvoHime с общим приёмником переносят данные без файлов: A выполняет push, B выполняет pull и получает сессии и memory items оператора A под своим оператором; повторный pull ничего не дублирует; обе попытки видны в `GET /api/sync/status` со своим direction.
