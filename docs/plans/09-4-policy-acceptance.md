# 09-4 — Acceptance и security closure

## Цель

Доказать, что capability и approval boundaries работают end-to-end и не
раскрывают секреты или неограниченный доступ.

## Deterministic acceptance fixtures

- разрешённый read-only tool с корректным snapshot;
- опасный tool без approval и с истёкшим approval;
- изменение path, scope, input, permission и snapshot после approval;
- path traversal, reparse point, protected path и network redirect;
- denied, unavailable и policy error как разные outcomes;
- timeout/cancellation на каждой стадии;
- secret/PII redaction в preview, receipts, logs и IPC;
- повторная доставка approval без повторного эффекта;
- parent/child capability subset и worker secret boundary.

## Release checks

- targeted Rust Core, permissions, storage и receipt tests;
- desktop IPC/projection negative tests и real-Core approval E2E;
- security review path/network/sandbox/secret boundaries;
- `cargo fmt --check`, `git diff --check`, `npm run check:protocol`,
  `npm run typecheck`, `npm test`;
- migration/rollback и supervisor recovery smoke.

## Закрытие

После прохождения критериев контракт переносится в `docs/architecture.md`,
подтверждённое состояние — в `docs/current-state.md`, а исполняемые файлы
плана 09 удаляются только после полной проверки. При неполном результате план
остаётся с явным разделом о реализованном и недостающем поведении.
