# 13-4 — Browser security acceptance

## Цель

Подтвердить browser capability end-to-end и закрыть отдельный security и
packaging review.

## Deterministic acceptance fixtures

- isolated contexts для двух runs;
- locator success/failure и actionability failure;
- navigation/redirect/download/timeout;
- SSRF/private IP/credential URL denial;
- click/type approval, rejection и cancellation;
- screenshot/DOM/trace redaction и provenance;
- browser crash, cleanup и supervisor recovery;
- replay без повторного side effect;
- disabled backend и invalid package manifest.

## Release checks

- targeted Rust tool-runtime/Core/browser tests;
- policy, approval, ledger, artifact и memory citation integration tests;
- package smoke, licensing/privacy/egress review;
- `cargo fmt --check`, `git diff --check`, `npm run typecheck`, `npm test`;
- evaluation fixtures из плана 12 или standalone offline equivalent.

## Закрытие

После прохождения security и packaging gate контракт переносится в
`docs/architecture.md`, состояние — в `docs/current-state.md`, а план 13
удаляется только после подтверждённого cleanup и отсутствия unrestricted
fallback.
