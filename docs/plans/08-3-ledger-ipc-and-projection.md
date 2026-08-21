# 08-3 — IPC replay и Core projection

## Цель

Передать typed execution history через authenticated desktop IPC так, чтобы
Electron мог восстановить projection после reconnect, но не мог изменить
durable ledger или расширить permissions.

## Изменения

1. Добавить bounded protobuf `ExecutionEvent` projection с common correlation
   fields и typed oneof body; существующий generic event envelope оставить для
   backward compatibility.
2. Расширить replay response сведениями о requested/current Core revision,
   earliest available sequence и gap/stale condition.
3. Сохранить порядок sequence, ограничение frame size и duplicate suppression
   при reconnect.
4. В Electron main adapter преобразовывать IPC event в redacted projection;
   renderer получает только чтение и пользовательские approval decisions через
   штатный Core command path.
5. При смене Core instance/session очищать stale projection и запрашивать
   bounded replay либо full snapshot по существующему protocol flow.

## Проверки

- protocol generation и major-version compatibility;
- replay после reconnect во время running, approval и cancellation;
- gap/stale revision и full snapshot fallback;
- отсутствие дублей и сохранение исходного порядка;
- negative tests на попытку изменить action/tool/scope через IPC;
- Electron typecheck, adapter tests и real-Core E2E.

## Готово, когда

После reconnect пользователь видит ту же action card и terminal state, а
повтор доставки UI-команды не создаёт новый effect и не меняет журнал напрямую.
