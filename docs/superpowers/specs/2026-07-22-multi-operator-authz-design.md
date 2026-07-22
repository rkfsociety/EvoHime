# Multi-operator authz scopes

## Цель

Перевести EvoHime из single-operator режима в безопасный локальный multi-operator режим без внешнего OAuth-провайдера и без хранения bearer-токенов в открытом виде.

## Выбранный подход

Используется PostgreSQL registry операторов с непрозрачными bearer-токенами. В базе хранится только SHA-256 хеш токена; сам токен показывается один раз при создании и после этого не восстанавливается. JWT и OAuth не вводятся: для локального deployment они добавляют refresh/revocation и внешние зависимости, не нужные текущему продукту.

Существующий `EVOHIME_API_TOKEN` сохраняется как режим совместимости. При его использовании запрос получает identity локального owner без изменения текущего способа запуска. При переходе на registry токены операторов проверяются через тот же HTTP/WS middleware.

## Модель данных

Миграция добавляет:

- `operators`: `id`, уникальные `name` и `token_hash`, `role`, `active`, `created_at`, `updated_at`, `last_seen_at`;
- `operator_id` в таблицы пользовательских данных, включая sessions, memory items, scheduled tasks, sites и permission approval audit.

Существующие строки получают одного автоматически созданного owner. Все новые строки получают текущий `operator_id` из request/task context. Внешние foreign keys и индексы обеспечивают фильтрацию и каскадные проверки на уровне БД.

Роли первой волны:

- `owner` — управляет операторами и имеет доступ к данным своего scope;
- `member` — работает только со своим scope и не может создавать/revoke операторов.

Доступ между операторами запрещён по умолчанию. Общие данные не вводятся в первой волне; глобальная memory остаётся scoped к оператору.

## Аутентификация и request context

После успешной проверки bearer-токена middleware создаёт `OperatorIdentity { id, name, role, source }` и помещает её в Axum request extensions. WS handshake использует тот же identity до создания socket context. Для legacy `EVOHIME_API_TOKEN` identity указывает на owner из bootstrap registry или на совместимый synthetic owner, если registry ещё не активирован.

Сравнение токенов выполняется по хешу с constant-time проверкой. Revoked/inactive оператор получает `401`; member при owner-only endpoint получает `403`. Токены не попадают в логи, ошибки или response body.

## API первой волны

Добавляются owner-only endpoints:

- `GET /api/operators` — список без токенов;
- `POST /api/operators` — создать оператора и вернуть plaintext token один раз;
- `POST /api/operators/:id/rotate` — заменить токен и вернуть новый один раз;
- `POST /api/operators/:id/revoke` — деактивировать оператора.

`/api/auth/status` показывает текущую identity и режим auth без раскрытия секретов. Остальные endpoints получают scope из identity, а не из пользовательского query-параметра.

## Безопасность и ошибки

- Нет fallback к «все данные» при отсутствии identity.
- Owner не может случайно удалить или revoke последнего активного owner.
- Нельзя создать дубликат имени или токена; коллизия хеша обрабатывается как ошибка.
- Scope-фильтры применяются в storage-запросах до сериализации ответа.
- Нельзя передать `operator_id` в теле запроса для обхода identity.
- Audit фиксирует создание, rotation и revoke оператора без токенов.

## Реализационные границы

Первая вертикальная волна включает миграцию, auth identity, owner-only operator API, scope enforcement для sessions/memory/tasks и HTTP/WS тесты. Остальные большие поверхности (sites, scheduled, plugins, backup import/export и полноценный UI управления операторами) подключаются через тот же `OperatorIdentity` отдельными малыми изменениями после базовой вертикали.

## Проверка

- unit-тесты хеша, ролей, revoke и защиты последнего owner;
- storage integration-тесты изоляции строк двух операторов;
- HTTP/WS тесты проверки identity и `401/403`;
- полный Rust workspace test, Clippy, frontend typecheck/build и миграционный smoke.

## Критерий готовности

Два активных оператора могут одновременно работать через HTTP/WS, видят только свои sessions/tasks/memory, owner может создать/rotate/revoke member, revoked token немедленно теряет доступ, а существующий `EVOHIME_API_TOKEN` продолжает работать как owner-compatible режим.
