# План 127.0 — Remote Client Control Plane

Статус: предложено; это implementation contract, функционал этим документом не считается реализованным.

## Цель

Добавить удалённый текстовый доступ к локальной Еве через лёгкий relay-сервис:

```text
Android-клиенты ── исходящие TLS-соединения ──┐
                                               ├── relay-сервер
Ева на ПК ─────── исходящее TLS-соединение ────┘
                         │
                         └── локальный authenticated Core IPC
```

Relay не запускает модели, инструменты и бизнес-логику. Он только проверяет
учётные данные, держит bounded-сессии и пересылает текстовые сообщения и
служебные события. Адреса телефонов и ПК могут меняться: клиенты сами
переподключаются к статичному IP relay-сервера. Домен не является обязательным.

## Порядок реализации

1. Отдельный relay-модуль и одноразовый безопасный installer/bootstrap для сервера.
2. Android-приложение как thin client с выбором зарегистрированного устройства.
3. PC connector и интеграция с существующим Core/desktop IPC Евы.
4. Общий device registry, pairing, отзыв токенов, offline/reconnect и release evidence.

## Архитектурная граница

```text
relay-installer -> relay service -> authenticated text sessions
                                      ├── Android client
                                      └── PC connector -> Core -> existing tools/models
```

Core остаётся единственным владельцем состояния, памяти, моделей, инструментов,
approval/policy и локальных эффектов. Android и relay не получают доступ к
SQLite, workspace, provider secrets или произвольному shell. Relay не является
model gateway и не участвует в distributed inference: общий пул устройств может
маршрутизировать независимые запросы, но разделение одной модели между узлами не
входит в базовый scope.

## Блокирующие зависимости

- Существующие Core policy/capability/approval, cancellation, event/replay,
  SQLite migration/backup, provenance и authenticated desktop IPC.
- Проверка текущего checkout на evidence freeze перед выбором новых IPC tags,
  schema revision и конкретных module paths.
- Поддерживаемая стратегия упаковки Android и server artifact; она фиксируется
  до реализации, но не должна переносить root-доступ в runtime.

## Опциональные зависимости

- Verification Evidence Ledger (#102), Project Quality Contract (#104),
  Diagnostics Bundle и Agent Benchmark Matrix.
- Прямое LAN-подключение без relay как оптимизация; при отказе relay клиент
  должен явно показать offline, а не незаметно обходить policy.
- Автоматический пул/маршрутизация нескольких PC-узлов; базовый режим выбора
  конкретного устройства должен работать без него.

## Общие security-инварианты

- TLS обязателен для удалённого режима; сертификат проверяется pinning/системным
  trust-профилем, а IP-адрес не является идентификатором или правом доступа.
- Сопряжение выполняется одноразовым короткоживущим кодом/QR; каждый Android и
  каждый PC connector получает отдельный отзывной токен.
- Root/SSH-секрет используется только во время явного bootstrap, не передаётся
  модели и не сохраняется relay. Runtime работает под отдельным пользователем с
  минимальными правами.
- Все команды на границе relay являются allow-listed текстовым протоколом с
  size/rate/time limits, correlation id, idempotency и redacted errors.
- Core повторно проверяет identity, scope, policy, approval и cancellation;
  relay не может повысить полномочия клиента.

## Не входит

Облачный inference, хранение пользовательской памяти на relay, передача файлов и
изображений в первой версии, arbitrary shell/network execution, постоянный
root-доступ, открытие входящих портов на домашних ПК, silent failover,
распределённый запуск одной модели и обязательная покупка домена.

## Критерии готовности всего направления

- [ ] Relay устанавливается повторно безопасно и идемпотентно, имеет health,
  bounded resources, service restart и rollback/uninstall procedure.
- [ ] Android поддерживает pairing, список online/offline устройств, выбор Евы,
  поток текстового ответа, reconnect, logout и отзыв собственного токена.
- [ ] PC connector использует существующий authenticated Core IPC и не открывает
  новый путь к runtime state.
- [ ] Несколько Android и PC-клиентов имеют независимые identities/tokens;
  отключение одного не ломает остальные.
- [ ] Проверены unauthorized/replay/expired/stale/oversize/rate-limit,
  disconnect/reconnect, restart, duplicate delivery и partial failure.
- [ ] Производительность relay измерена отдельно по CPU, памяти, числу сессий,
  p50/p95 задержки и сетевому трафику; значения не объявляются целями без baseline.
- [ ] После реализации подтверждённый контракт переносится в `docs/architecture.md`,
  состояние — в `docs/current-state.md`, evidence — в `docs/release-evidence.md`;
  комплект плана удаляется только после полного закрытия этапов 1–4.

## Этапы

- [Этап 1 — relay-контракт, installer, registry и storage](./127-1-remote-client-control-plane.md)
- [Этап 2 — runtime, PC connector и recovery](./127-2-remote-client-control-plane.md)
- [Этап 3 — Android, IPC projection и пользовательский UI](./127-3-remote-client-control-plane.md)
- [Этап 4 — verification, security и release evidence](./127-4-remote-client-control-plane.md)

## Связанный issue

- [#108 Remote Client Control Plane](https://github.com/rkfsociety/EvoHime/issues/108)
