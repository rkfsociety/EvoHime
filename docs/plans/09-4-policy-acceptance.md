# 09-4 — Acceptance и security closure

## Цель

Доказать end-to-end, что capability, policy и approval boundaries работают для
реальных Core paths и не раскрывают секреты или неограниченный доступ.

## Зависимости

### Блокирующие

- 09-1, 09-2 и 09-3;
- план 08 и его replay/recovery/receipt acceptance;
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
- expired, denied, cancelled и already-claimed approval → distinct refusal,
  без повторного effect;
- изменение task/session/run/action, tool, permission, normalized path,
  input, scope, policy version или snapshot после approval → `denied`/
  `expired`/`policy_error` по контракту, без dispatch;
- traversal, absolute/UNC escape, symlink/reparse race, protected path и
  operation mismatch → hard deny;
- private/internal target, redirect/host/scheme change, DNS rebinding,
  timeout, payload, concurrency и provider budget → correct bounded outcome;
- parent/child snapshot escalation, missing adapter and unavailable worker →
  subset refusal или `unavailable`;
- cancellation до запуска, во время процесса и после return → correct
  cancellation/unknown-outcome recovery, never false successful mutation;
- secret/PII in input, preview, IPC, receipts, journal, logs, export and hook
  metadata → redacted/no raw value;
- repeated approval resolve, IPC replay and process restart → idempotent state,
  monotonic expiry and no blind retry.

## Release checks

- targeted tests:
  `cargo test -p evohime-permissions -p evohime-tool-runtime
  -p evohime-receipts -p evohime-local-storage -p evohime-core`;
- desktop IPC/projection negative tests, generated protocol check and real-Core
  approval E2E; compatibility suite remains an oracle for old clients;
- security review of Windows path/reparse/TOCTOU, SSRF/redirect, sandbox,
  supervisor secret and adapter boundaries;
- `cargo fmt --check`, `git diff --check`, `npm run check:protocol`,
  `npm run typecheck`, `npm test` from `desktop/evohime-electron`;
- SQLite migration/backup/rollback and supervisor restart/recovery smoke,
  including old database without snapshot columns and interrupted approval
  claim;
- no acceptance claim is based only on a nominal `ok`: inspect ledger/receipt,
  sentinel filesystem/network/process state and post-restart state.

## Закрытие

После прохождения всех критериев перенести фактический contract в
`docs/architecture.md`, подтверждённое состояние и конкретные тесты — в
`docs/current-state.md`, обновить `docs/development-plan.md` и
`docs/plans/README.md`, затем удалить исполняемые файлы плана 09 только task-only
коммитом после свежих проверок. До этого планы остаются в каталоге с явным
разделом о реализованном и недостающем поведении.
