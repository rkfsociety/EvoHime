# План 09 — Policy, capabilities и approval

## Цель

Сделать каждое внешнее действие проверяемым и ограниченным. Модель может
предложить действие, но не получает права самостоятельно менять filesystem,
network, процессы или секреты.

План 09 — это hardening существующих контуров и их сведение в единый Core-owned
контракт. Он не должен создавать второй execution runtime, второй approval
registry или переносить authority в renderer.

## Что уже есть в checkout

- `evohime-permissions` (`crates/permissions/`): typed permissions/modes, hard
  policy rules (`policy.rs`, `pattern.rs`), session overrides, scoped path
  grants и canonical exact-call approval identity;
- `evohime-tool-runtime` (`crates/tool-runtime/`): `ToolRegistry` с preflight,
  permission checks, bounded previews, `WorkspaceSandbox` (`sandbox.rs`),
  network capability и SSRF checks (`network_capability.rs`, `ssrf.rs`),
  cancellation;
- `evohime-receipts` (`crates/evohime-receipts/src/runtime.rs`): durable
  action/approval intent (`receipt_actions`, `receipt_approval_intents`),
  canonical call hash, monotonic expiry с boot binding, атомарный claim и
  claim-time policy recheck (`claim_approval_checked`);
- `run_policy`, model-request provenance, EventJournal и authenticated IPC;
- supervisor-owned provider/secret boundary. Секретные значения не являются
  capability и не должны попадать в snapshot, prompt, argv, renderer или log.

Это не означает, что план 09 уже выполнен. Чего нет:

- общего Core-owned capability snapshot и единого gate для всех effect paths:
  `PermissionEngine` вызывается из нескольких мест независимо
  (`crates/evohime-core/src/lib.rs`, `crates/tool-runtime/src/registry.rs`,
  `crates/tool-runtime/src/tools/shell.rs`);
- `PermissionEngine` всё ещё держит in-memory approval state
  (`approvals: HashMap<Uuid, ApprovalRecord>`) параллельно durable intent;
- рядом с `claim_approval_checked` остаётся `claim_approval` без policy
  recheck, то есть обходной путь claim;
- persisted `policy_decision` в receipts сегодня допускает только
  `allow`/`deny`/`approval_required`, поэтому расширенный словарь outcomes
  требует additive-миграции, а не переопределения существующих значений.

## Границы

Входит: versioned capability snapshot на run/action, immutable policy hash,
workspace и network scope, Core-owned resolver, durable approval lifecycle,
typed outcomes, preflight/postflight hooks, redaction и bounded
input/output.

Не входит: unrestricted desktop control, host-full-access режим, обязательный
внешний egress, модельная оценка риска как authority, прямой renderer/tool
доступ к workspace или обход supervisor/Core.

## Зависимости

### Блокирующие

- план 08 [`08-0-execution-ledger.md`](08-0-execution-ledger.md): устойчивые
  action/run identifiers, typed terminal events, execution linkage, receipt
  linkage и replay/recovery semantics;
- текущие `evohime-permissions`, `evohime-tool-runtime`, `evohime-receipts`,
  supervisor secret boundary и authenticated desktop IPC.

План 08 является зависимостью, а не уже существующим результатом: до его
завершения 09 нельзя принимать как durable ledger integration.

### Опциональные

- план 07-2 [`toolkit-catalog-lifecycle`](07-2-toolkit-catalog-lifecycle.md):
  если catalog metadata отсутствует, snapshot использует установленный
  manifest identity/hash и typed `unavailable` для неизвестного adapter;
- browser, voice и vision adapters из планов 13–15: до их появления snapshot
  содержит пустой adapter scope, а вызов получает `unavailable` и не имеет
  fallback на unrestricted access.

## Этапы

- [09-1 — capability snapshot и typed policy contract](09-1-capability-snapshot.md)
- [09-2 — Core resolver и operation checks](09-2-core-policy-resolver.md)
- [09-3 — approval lifecycle и hooks](09-3-approval-hooks.md)
- [09-4 — acceptance и security closure](09-4-policy-acceptance.md)

Порядок: 09-1 → 09-2 → 09-3 → 09-4.

## Готово, когда

Каждый effect path (`ToolRegistry`, terminal, workflow adapters и будущие
provider/MCP/browser adapters) вызывает один Core policy gate; dangerous action
без действующего policy/approval не запускается; approval нельзя перенести на
другой task/session/run/action/call/scope/snapshot; rejection, expiry,
cancellation и unknown outcome имеют durable typed linkage; секреты не
попадают в renderer, preview, receipts или logs открытым текстом.
