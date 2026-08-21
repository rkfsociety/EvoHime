# 09-3 — Approval lifecycle и preflight/postflight hooks

## Цель

Сделать подтверждение опасного действия одноразовым, auditable и связанным с
точно тем canonical call, который будет выполнен.

## Зависимости

### Блокирующие

- 09-1 для immutable snapshot/action binding;
- 09-2 для preflight и recheck непосредственно перед effect;
- план 08 для durable action, terminal event, receipt и unknown-outcome
  recovery.

### Опциональные

- новые UI/adapters могут использовать additive approval projection; старый
  compatibility shell продолжает получать bounded preview и typed result;
  отсутствие новой projection не даёт права выполнить action без Core gate.

## Единый источник approval

Сохранить canonical call hash и preview derivation из `evohime-permissions`, но
свести dangerous execution к durable approval intent/receipt path
`evohime-receipts`. In-memory `PermissionEngine` approval records не должны
оставаться альтернативным источником authority после migration; legacy
callers либо получают durable intent, либо fail closed.

Approval request bounded и содержит:

- `approval_id`, task/session/run/action IDs;
- tool/action identity, permission, normalized scope, canonical input hash и
  effective snapshot hash;
- policy id/version/hash, bounded redacted preview и preview truncation flag;
- created/deadline timestamps с monotonic boot binding и bounded risk signal.

Preview строится из canonical input, redacts secret/PII before truncation и не
является самостоятельным grant. Approval подписывается/сохраняется вместе с
call/action binding; произвольный текст из renderer не может подменить preview
или hash.

## State machine и команды

Durable state machine:

```text
pending -> granted | denied | expired | cancelled
granted -> claimed
claimed -> terminal action outcome (succeeded/failed/unknown_outcome)
```

`claimed` approval нельзя использовать повторно. Повторное approve/reject,
повторный resolve после expiry и повторная доставка IPC возвращают фактическое
сохранённое состояние и не создают side effect. Resolve проверяет
authenticated shell/session и существование approval; один только
`approval_id` из renderer не переносит право между task/session/run/action.

Перед dispatch Core атомарно:

1. загружает action и approval intent из durable store;
2. проверяет expiry/boot monotonicity и state;
3. пересчитывает policy и сравнивает task, session, run, action, tool,
   permission, normalized scope, input/call hash, snapshot hash и policy
   version;
4. пишет pre-action receipt/ledger marker и claim;
5. только после успешной транзакции вызывает operation gate и effect path.

Mismatch, stale policy, expiry, denial или cancellation записываются как
typed refusal/terminal event и не переходят в успешную mutation. Crash до
marker, после marker и после return использует recovery плана 08; approval не
повторяется автоматически при `unknown_outcome`.

## Hooks

Hooks — только bounded Core-owned functions, не model/plugin/renderer code:

- `preflight` после canonicalization и до effect может записать redacted
  audit/metrics и отказать при policy/audit failure;
- `postflight` после terminal result записывает redacted outcome/metrics;
  его сбой не превращает уже совершённый effect в `denied`, а получает
  отдельный diagnostic event;
- каждый hook получает typed decision, action/snapshot hashes и redacted
  metadata, но не secret value/raw unrestricted input;
- hook не может расширить capabilities, изменить call/scope/hash, погасить
  чужой approval или пропустить hard deny. Hook chain version фиксируется в
  action/receipt.

## Проверки

- approve/reject/expiry/cancel, monotonic restart boundary и атомарный claim;
- repeated resolve/delivery/execute — идемпотентны и не дают второй effect;
- mismatch для task/session/run/action/tool/permission/scope/input/snapshot/
  policy version;
- cancellation до dispatch, во время выполнения и после результата;
- refusal reason как durable ledger/receipt event, включая stale и
  policy-denied;
- hook failure, redaction, bounded input and attempt to bypass via renderer,
  workflow or direct adapter.

## Готово, когда

Пользователь подтверждает именно тот bounded action, который Core atomically
claims and executes; любое изменение call, scope, policy или snapshot делает
approval недействительным, а повторное сообщение не создаёт новый side effect.
