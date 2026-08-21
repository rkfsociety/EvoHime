# 09-3 — Approval lifecycle и preflight/postflight hooks

## Цель

Сделать подтверждение опасного действия одноразовым, auditable и связанным с
точно тем canonical call, который будет выполнен.

## Изменения

1. Создавать bounded approval request с canonical call hash, action/tool IDs,
   permission, scope, safe preview, risk signal, expiry и policy version.
2. Реализовать одноразовые решения approve/reject/expire/cancel с атомарным
   consumption и typed terminal event.
3. Повторную отправку approval и stale approval обрабатывать идемпотентно,
   не создавая новый side effect.
4. Перед execution повторно сравнивать canonical call, input, tool, scope,
   permission и snapshot hash.
5. Добавить preflight/postflight hooks для audit, redaction и metrics; hooks не
   могут расширить capabilities или пропустить hard deny.
6. Публиковать rejection, timeout и cancellation в execution ledger плана 08.

## Проверки

- approve/reject/expiry и повторная отправка;
- exact-call mismatch и stale approval;
- cancellation до dispatch, во время выполнения и после результата;
- rejection с причиной как terminal auditable event;
- отсутствие обхода policy через hook или renderer IPC.

## Готово, когда

Пользователь подтверждает именно тот bounded action, который будет выполнен,
а любое изменение call или scope делает approval недействительным.
