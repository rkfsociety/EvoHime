# 10-4 — Acceptance и compatibility closure

## Цель

Подтвердить end-to-end границы IPC и provider adapters на restart, mismatch,
fallback и target switch сценариях.

## Deterministic acceptance fixtures

- совместимый CoreInfo и успешный handshake;
- major mismatch, unsupported feature и stale session;
- Core unavailable и reconnect/replay;
- provider unavailable с fallback без старого dispatch;
- worker capability/path/secret scope violation;
- прямой renderer IPC/HTTP call;
- смена workspace/provider/backend во время in-flight operation;
- stale projection и stale response после Core revision change;
- oversized frame, timeout и cancellation.

## Release checks

- Rust desktop IPC contract/version tests;
- Electron protocol generation, adapter, security и real-Core E2E tests;
- compatibility suite для transitional shell;
- `npm run check:protocol`, `npm run typecheck`, `npm test`, targeted Rust tests;
- `git diff --check`, packaging smoke и security review secret/target boundary.

## Закрытие

После прохождения критериев контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, а план 10 удаляется из
каталога только после полной проверки. Неполная реализация остаётся описанной
в плане и не помечается выполненной.
