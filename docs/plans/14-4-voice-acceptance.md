# 14-4 — Voice quality и acceptance

## Цель

Подтвердить realtime voice pipeline, privacy и quality gates на deterministic
fixtures.

## Deterministic acceptance fixtures

- 16 kHz preprocessing и quality fallback;
- endpointing, partial transcript и timestamp ordering;
- barge-in во время STT, LLM и TTS;
- backpressure, timeout, cancellation и worker crash;
- permission deny, retention expiry и forget;
- redaction transcript/audio metadata;
- engine degradation и provider unavailable;
- bounded CPU/GPU/memory/queue budgets;
- optional speaker cluster остаётся `unverified`.

## Release checks

- listener-contract, Core, ambient-storage и supervisor tests;
- voice IPC/worker integration и Electron projection tests;
- quality benchmark из плана 12 либо standalone offline fixtures;
- `cargo fmt --check`, `git diff --check`, `npm run typecheck`, `npm test`;
- packaging, licensing, privacy, egress и maintenance review.

## Закрытие

После прохождения quality/security gate контракт переносится в
`docs/architecture.md`, состояние — в `docs/current-state.md`, а план 14
удаляется только после подтверждения всех privacy и cleanup сценариев.
