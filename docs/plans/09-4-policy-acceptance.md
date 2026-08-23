# 09-4 — Acceptance и security closure

## Цель

Доказать end-to-end, что capability, policy и approval boundaries работают для
реальных Core paths и не раскрывают секреты или неограниченный доступ.

## Зависимости

### Блокирующие

- [09-1](09-1-capability-snapshot.md), [09-2](09-2-core-policy-resolver.md) и
  [09-3](09-3-approval-hooks.md): acceptance проверяет собранный контракт, а не
  отдельные слои;
- реализованный план 08 и его replay/recovery/receipt acceptance
  (контракт — [`../architecture.md`](../architecture.md#core-owned-execution-ledger),
  подтверждённое состояние — [`../current-state.md`](../current-state.md));
- текущие authenticated desktop IPC, Electron adapter и compatibility suite.

### Опциональные

- browser/voice/vision/provider adapters: для отсутствующего adapter acceptance
  проверяет typed `unavailable` и отсутствие fallback side effect;
- evaluation/telemetry из планов 07 и 12: при отсутствии используются
  deterministic security fixtures этого этапа.

## Deterministic acceptance matrix

Каждый case проверяет не только decision, но и отсутствие/наличие sentinel
effect, durable action/receipt linkage и bounded redacted projection:

- read-only workspace operation с корректным snapshot → `allowed` и один
  terminal outcome;
- write, shell, git write, MCP/HTTP и workflow mutation без approval →
  `approval_required`, без side effect;
- expired, denied, cancelled и already-claimed approval → различимый refusal,
  без повторного effect;
- изменение task/session/run/action, tool, permission, normalized path,
  input, scope, policy version или snapshot после approval → `denied`/
  `expired`/`policy_error` по контракту, без dispatch;
- traversal, absolute/UNC escape, symlink/reparse race, protected path и
  operation mismatch → hard deny;
- private/internal target, redirect со сменой host/scheme, DNS rebinding,
  timeout, payload, concurrency и provider budget → корректный bounded
  outcome;
- эскалация parent/child snapshot, отсутствующий adapter и недоступный worker →
  отказ по subset или `unavailable`;
- cancellation до запуска, во время процесса и после return → корректный
  cancellation/unknown-outcome recovery, никогда не ложный успех mutation;
- secret/PII во входе, preview, IPC, receipts, journal, logs, export и hook
  metadata → redacted, без raw value;
- повторный resolve approval, replay IPC и рестарт процесса → идемпотентное
  состояние, monotonic expiry и отсутствие blind retry;
- отсутствие второго пути grant/claim: тест доказывает, что desktop IPC path
  (`ipc_bridge.rs`) больше не выдаёт approval мимо Core gate.

## Release checks

- targeted tests:
  `cargo test -p evohime-permissions -p evohime-tool-runtime
  -p evohime-receipts -p evohime-local-storage -p evohime-core`;
- негативные тесты desktop IPC/projection, generated protocol check и E2E
  approval против реального Core; compatibility suite остаётся oracle для
  старых клиентов;
- security review Windows path/reparse/TOCTOU, SSRF/redirect, sandbox,
  supervisor secret и adapter boundaries;
- `cargo fmt --check`, `git diff --check`, `npm run check:protocol`,
  `npm run typecheck`, `npm test` из `desktop/evohime-electron`;
- SQLite migration/backup/rollback и smoke рестарта supervisor с recovery,
  включая старую базу без snapshot- и `session_id`-колонок, записи со старым
  CHECK-словарём `policy_decision`, intents в состоянии `lost` и прерванный
  approval claim;
- проверка всех внутренних Markdown-ссылок и отсутствие ссылок на удаляемые
  файлы плана после closure;
- ни одно acceptance-утверждение не опирается только на номинальный `ok`:
  проверяются ledger/receipt, sentinel-состояние filesystem/network/процессов
  и состояние после рестарта.

## Закрытие

После прохождения всех критериев перенести фактический contract в
`docs/architecture.md`, подтверждённое состояние и конкретные тесты — в
`docs/current-state.md`, обновить `docs/development-plan.md` и
`docs/plans/README.md`, затем удалить исполняемые файлы плана 09 только task-only
коммитом после свежих проверок. До этого планы остаются в каталоге с явным
разделом о реализованном и недостающем поведении.
