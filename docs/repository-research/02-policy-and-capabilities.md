# 02. Policy, capabilities и approval

## Цель

Сделать каждое внешнее действие проверяемым и ограниченным. Модель предлагает
действие, но не получает права самостоятельно менять filesystem, network,
процессы или секреты.

## Scope

- `CapabilitySnapshot` на каждый run;
- permissions для инструментов, workspace, network routes, browser sessions и
  лимитов;
- единый Core-owned path resolver;
- path, scope, network, timeout и cancellation checks;
- approval request, risk signal, explicit rejection и terminal receipt;
- preflight/postflight hooks;
- redaction и bounded input/output.

## Инварианты

- Проверки повторяются в Core на каждой операции, даже если UI уже проверял её.
- Абсолютный путь разрешается только через Core; относительный путь разрешается
  относительно канонического workspace anchor.
- Runtime context, reasoning и текст prompt не являются доказательством права.
- `approval_id` связан с неизменяемым canonical call и не переносится на другой
  task/session/tool/permission/scope/input.
- Отказ пользователя — terminal auditable event с причиной.
- Модельная оценка риска advisory и не может заменить hard policy.
- Секреты проходят через supervisor/DPAPI references, а не через renderer,
  prompt, лог или аргументы командной строки.

## Ограничения

- bounded size, concurrency и provider budget;
- timeout и cancellation с гарантированным terminal outcome;
- запрет unrestricted desktop control;
- запрет host-full-access режима и обязательного внешнего egress;
- hooks не могут обходить supervisor, sandbox или Core policy.

## Тестовый контур

- попытка изменить путь, scope, input или permission после approval;
- повторная отправка approval и stale approval;
- path traversal, symlink/reparse point и network/redirect policy;
- redaction secrets/PII в receipts и logs;
- cancellation до запуска, во время запуска и после результата;
- denied, unavailable и policy error как разные typed outcomes.

## Критерии готовности

- capability snapshot сохраняется и привязывается к run;
- опасное действие нельзя выполнить без действующего approval/policy;
- все path/network/tool checks находятся в Core;
- rejection, timeout и cancellation видны в execution ledger;
- секреты не попадают в UI, SQLite receipts и логи в открытом виде.

## Зависимости

Использует 01 для canonical call, receipt и terminal event. Может начинаться
параллельно с частью 01, но закрытие этого раздела блокирует 03–09.
