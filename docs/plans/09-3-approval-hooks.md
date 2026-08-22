# 09-3 — Approval lifecycle и preflight/postflight hooks

## Цель

Сделать подтверждение опасного действия одноразовым, auditable и связанным с
точно тем canonical call, который будет выполнен.

## Зависимости

### Блокирующие

- [09-1](09-1-capability-snapshot.md) для immutable snapshot/action binding;
- [09-2](09-2-core-policy-resolver.md) для preflight и recheck непосредственно
  перед effect;
- план 08 [`08-1`](08-1-ledger-contract.md) и
  [`08-2`](08-2-ledger-storage-and-recovery.md) для durable action, terminal
  event, receipt и unknown-outcome recovery.

### Опциональные

- новые UI/adapters могут использовать additive approval projection; старый
  compatibility shell продолжает получать bounded preview и typed result;
  отсутствие новой projection не даёт права выполнить action без Core gate.

## Что уже есть в коде

- durable approval intents со состояниями `pending`, `granted`, `denied`,
  `expired`, `claimed`, `lost` и action states `awaiting_approval`,
  `prepared`, `refused`, `succeeded`, `failed`, `cancelled`,
  `pending_recovery`, `quarantined`
  (`crates/evohime-receipts/src/runtime.rs`); отмена и неизвестный исход
  живут на уровне action (`cancelled`, `pending_recovery`/`quarantined` с
  `recovery_code`), а не отдельным состоянием intent;
- monotonic deadline с boot binding: expiry считается по `clock_boot_id` и
  monotonic ms, с fallback на wall clock только между boot-ами;
- атомарный claim с policy recheck (`claim_approval_checked`) и тесты на
  однократность claim, monotonic expiry и отказ при текущем deny.

Чего нет: обходной `claim_approval` без recheck остаётся публичным API и
используется в `crates/evohime-core/src/ipc_bridge.rs`;
`PermissionEngine` хранит собственные in-memory `approvals`/`audit`
(`crates/permissions/src/lib.rs`) как второй источник authority; в intent не
записываются `session_id`, snapshot hash, policy version и hook chain
version.

## Единый источник approval

Сохранить canonical call hash и preview derivation из `evohime-permissions`, но
свести dangerous execution к durable approval intent/receipt path
`evohime-receipts`. In-memory `PermissionEngine` approval records не должны
оставаться альтернативным источником authority после migration; legacy
callers либо получают durable intent, либо fail closed. Claim без policy
recheck удаляется или становится приватным: единственный поддерживаемый путь —
`claim_approval_checked`.

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

Durable state machine повторяет уже существующий словарь состояний
`receipt_approval_intents` и не вводит синонимов:

```text
pending -> granted | denied | expired | lost
granted -> claimed | expired | lost
claimed -> terminal action outcome (succeeded | failed | cancelled
           | pending_recovery/quarantined при неизвестном исходе)
```

Отмена пользователем не вводит состояние `cancelled` для intent: intent
переходит в `lost`, а `cancelled` фиксируется как terminal state action.
Новые состояния добавлять нельзя — расширение допускается только additive
миграцией CHECK-констрейнта вместе с mapping существующих записей.

`claimed` approval нельзя использовать повторно. Повторное approve/reject,
повторный resolve после expiry и повторная доставка IPC возвращают фактическое
сохранённое состояние и не создают side effect. Resolve проверяет
authenticated shell/session и существование approval; один только
`approval_id` из renderer не переносит право между task/session/run/action.

Перед dispatch Core атомарно:

1. загружает action и approval intent из durable store;
2. проверяет expiry/boot monotonicity и state;
3. пересчитывает policy и сравнивает task, session (после additive-колонки),
   run, action, tool,
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

- approve/reject/expiry/cancel, граница monotonic restart и атомарный claim;
- повторные resolve/delivery/execute идемпотентны и не дают второй effect;
- mismatch по task/session/run/action/tool/permission/scope/input/snapshot и
  policy version;
- cancellation до dispatch, во время выполнения и после результата;
- refusal reason как durable ledger/receipt event, включая stale и
  policy-denied;
- сбой hook, redaction, bounded input и попытка обхода через renderer,
  workflow или прямой вызов adapter;
- отсутствие второго пути claim: тест доказывает, что claim без policy recheck
  недоступен извне.

## Готово, когда

Пользователь подтверждает именно тот bounded action, который Core атомарно
claim-ит и выполняет; любое изменение call, scope, policy или snapshot делает
approval недействительным, а повторное сообщение не создаёт новый side effect.
