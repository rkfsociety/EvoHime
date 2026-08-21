# План 09 — Policy, capabilities и approval

## Цель

Сделать каждое внешнее действие проверяемым и ограниченным. Модель может
предложить действие, но не получает права самостоятельно менять filesystem,
network, процессы или секреты.

## Что уже есть в checkout

- Core-owned permissions и approval tokens;
- exact-call recheck и signed receipts;
- supervisor-owned secret references и DPAPI boundary;
- execution ledger плана 08 для terminal outcomes;
- bounded tool/runtime limits и cancellation paths.

План 09 усиливает существующие границы и не переносит policy в renderer или
второй execution runtime.

## Границы

Входит: capability snapshot на run, permissions, workspace/network scope,
Core-owned path resolver, approval lifecycle, preflight/postflight hooks,
redaction и bounded input/output.

Не входит: unrestricted desktop control, host-full-access режим, обязательный
внешний egress, модельная оценка риска как authority или обход supervisor/Core.

## Зависимости

### Блокирующие

- план 08 для canonical call, receipt, terminal event и execution linkage;
- существующие Core policy, permissions, supervisor secret boundary и
  authenticated IPC.

### Опциональные

- browser, voice и vision adapters получают capability-scoped session после
  появления своих планов; до этого policy поддерживает builtin tools;
- catalog metadata из плана 07-2 не обязательна: используется manifest hash.

## Этапы

- [09-1 — capability snapshot и typed policy contract](09-1-capability-snapshot.md)
- [09-2 — Core resolver и operation checks](09-2-core-policy-resolver.md)
- [09-3 — approval lifecycle и hooks](09-3-approval-hooks.md)
- [09-4 — acceptance и security closure](09-4-policy-acceptance.md)

Порядок: 09-1 → 09-2 → 09-3 → 09-4.

## Готово, когда

Опасное действие нельзя выполнить без действующего policy/approval; все
проверки находятся в Core; approval нельзя перенести на другой call/scope;
rejection, timeout и cancellation видны в execution ledger; секреты не
попадают в renderer, receipts или логи открытым текстом.
