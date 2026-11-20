# План 198.0 — Независимые EvaPulse и Улей

Статус: active implementation contract. План исправляет границу владения между
внешним EvaPulse и Ульем. Улей остаётся домом Королевы Евы (матери-пчелы), её
агентов, задач, approvals, рабочих пространств и истории. EvaPulse становится
отдельным runtime, который может обмениваться с Ульем только через
аутентифицированный typed-контракт и не владеет внутренними задачами Улья.

## Цель

Разделить две независимые системы:

```text
EvaPulse  ── typed signal/request ──>  Hive boundary
EvaPulse  <─ typed result/event   ───  Hive boundary

Hive
├── Queen Eva — мать-пчела
├── agents and departments
├── tasks and approvals
├── workspaces and permissions
└── durable state and history
```

EvaPulse может сообщить Королеве о внешнем сигнале или попросить её рассмотреть
действие. Только Улей и Королева решают, создавать ли внутреннюю задачу,
какому агенту её поручить и какое approval требуется. Все approvals,
возникающие из работы Улья, должны отображаться в Улье.

## Основание checkout

Текущая реализация нарушает границу: `EvaPulseDaemon::start` получает
`TaskCoordinator`, читает `HiveSettings` и напрямую вызывает
`dispatch_recovery`/`dispatch_evolution`. IPC-команда `togglePulse` фактически
управляет daemon из панели Улья, а snapshot смешивает состояние Улья со
статусом EvaPulse. Renderer уже пытается показывать approval в Улье, но
approval, не попавший в conversation projection Улья, теряется из
`hiveEvents`.

Это создаёт четыре проблемы:

1. остановка или сбой EvaPulse может влиять на жизненный цикл задач Улья;
2. EvaPulse получает полномочия владельца внутреннего планировщика Улья;
3. пользователь видит технический `EvaPulse` вместо состояния Улья;
4. запрос подтверждения от автономной работы не гарантированно виден рядом с
   Королевой.

## Граница

- Улей и EvaPulse — независимые Core-owned подсистемы с отдельными состоянием,
  cancellation, recovery и health status.
- EvaPulse не получает `TaskCoordinator`, `HiveSettings`, Hive snapshot или
  внутренние task/approval registries.
- Улей не управляет расписанием, включением, выключением или recovery EvaPulse.
- Связь разрешена только через версионированный typed message boundary с
  correlation ID, bounded payload, source identity, TTL и idempotency key.
- Внешний сигнал EvaPulse не является задачей Улья. Королева может принять,
  отклонить, отложить или преобразовать его во внутреннюю задачу.
- Работа Королевы, агентов и дочерних задач всегда принадлежит Улью; её
  approvals, включая вызванные внешним сигналом, проецируются в UI Улья.
- Сбой EvaPulse не отменяет и не изменяет уже принятые задачи Улья.
- Сбой Улья не изменяет durable состояние EvaPulse; непринятые сообщения
  остаются в его bounded outbox/dead-letter состоянии.
- Один процесс Core и supervisor сохраняются; отдельный процесс не требуется,
  если независимость владельцев состояния и контрактов сохраняется.
- Техническое имя `EvaPulse` не является пользовательским режимом Улья.

## Изменяемые контракты

- новый typed `EvaPulse → Hive` envelope: `source`, `message_id`, `kind`,
  `correlation_id`, `created_at`, `expires_at`, bounded `payload`;
- typed Hive response/event envelope с результатом `accepted`, `rejected`,
  `deferred` или `failed` и redacted reason;
- отдельные lifecycle/status projections для EvaPulse и Улья;
- Hive task binding, который хранит `origin = hive_internal | eva_pulse` без
  передачи EvaPulse внутренних полномочий;
- approval projection с `origin`, `task_id`, `approval_id` и связью с
  `hive-system-chat`, чтобы approval всегда был виден в Улье;
- аддитивная миграция SQLite/EventJournal и обратная совместимость старых
  событий без origin как `hive_internal` только для уже существующих задач;
- IPC/UI убирает `EvaPulse` из пользовательского переключателя и показывает
  состояние автономности Улья отдельно от статуса EvaPulse.

## Блокирующие зависимости

- `EventJournal` и существующие authenticated IPC roles/sequence replay;
- `TaskCoordinator`, approval registry и conversation log Улья;
- `crates/evohime-core/src/eva_pulse.rs`, `hive_state.rs`,
  `hive_settings.rs`, `ipc_bridge_hive_state.rs`;
- `desktop/evohime-electron/src/renderer/src/OperationsPanel.tsx`,
  `App.tsx`, conversation projection и shared IPC types;
- канонический proto в `crates/desktop-ipc/proto/evohime.desktop.proto`.

## Опциональные зависимости

- отдельный OS-процесс EvaPulse — только если это понадобится для deployment
  isolation; план не требует его заранее;
- durable delivery/retry для внешних EvaPulse-сигналов сверх bounded outbox;
- отдельный экран диагностики EvaPulse; базовый статус достаточно показать в
  Operations/diagnostics, не смешивая его с Ульем.

## Recovery и rollback

- до включения нового контракта старые direct-dispatch paths работают только в
  compatibility adapter и пишут диагностическое событие о legacy ownership;
- повторная доставка сигнала дедуплицируется по `message_id` и
  `idempotency_key`;
- истёкший или превышающий размер envelope переводится в rejected/dead-letter
  без создания задачи Улья;
- перезапуск EvaPulse восстанавливает только его outbox/status, но не вызывает
  replay внутренних задач Улья;
- перезапуск Улья восстанавливает Hive tasks/approvals из журнала и не запускает
  EvaPulse cycle;
- rollback версии сохраняет чтение старых Hive/EvaPulse событий и может
  отключить приём новых сообщений, не удаляя durable history.

## Этапы

- [Этап 1 — Core boundary, schema и storage](./198-1-independent-eva-pulse-and-hive.md)
- [Этап 2 — Runtime isolation, delivery и recovery](./198-2-independent-eva-pulse-and-hive.md)
- [Этап 3 — IPC, projections и UI Улья](./198-3-independent-eva-pulse-and-hive.md)
- [Этап 4 — Verification, документация и release evidence](./198-4-independent-eva-pulse-and-hive.md)

## Критерии готовности

- [ ] EvaPulse больше не получает `TaskCoordinator` и не создаёт Hive tasks
  напрямую.
- [ ] Улей и EvaPulse имеют независимые status/lifecycle/recovery owners.
- [ ] Внешний сигнал EvaPulse может быть принят только через typed boundary и
  после принятия получает Hive-owned task binding.
- [ ] Любое approval для Hive task, включая task с `origin = eva_pulse`, видно
  в панели Улья и разрешается только через `core.resolveApproval`.
- [ ] Остановка EvaPulse не останавливает Королеву или уже принятые задачи.
- [ ] Остановка Улья не изменяет состояние EvaPulse.
- [ ] UI не представляет EvaPulse как режим или замену Улья.
- [ ] Есть focused Rust/Electron tests, recovery tests, protocol checks и
  redacted release evidence.
- [ ] `architecture/hive.md`, `architecture.md`, `current-state.md` и
  `release-evidence.md` согласованы с реализованной границей.
