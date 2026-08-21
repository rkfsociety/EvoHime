# 12-4 — Release gates и acceptance

## Цель

Подключить deterministic evaluation к offline/CI release gate и подтвердить
его границы.

## Deterministic acceptance fixtures

- успешный model/tool run с полным correlation chain;
- invalid manifest и policy denial;
- approval approve/reject/expiry;
- timeout, retry, cancellation и partial failure;
- restart recovery, duplicate delivery и hash mismatch;
- secret/PII redaction и bounded trace;
- одинаковый replay и repeated-trial reliability;
- повреждённый trace и typed unknown output;
- отсутствие production side effects.

## Release checks

- offline evaluation fixtures в CI с явными thresholds;
- targeted Rust telemetry/ledger/provenance tests;
- Electron report/projection tests;
- `cargo fmt --check`, `git diff --check`, `npm run typecheck`, `npm test`;
- security review retention, redaction, export и external telemetry opt-in.

## Закрытие

После прохождения gate контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, а план 12 удаляется
только после проверки всех fixtures и отсутствия production side effects.
