# План 12 — Local telemetry и deterministic evaluation

## Цель

Получить локально воспроизводимую оценку качества, безопасности и стоимости
агента без обязательной отправки данных во внешний telemetry backend.

## Что уже есть в checkout

- execution ledger, signed receipts и model-request provenance;
- bounded Core event journal и replay;
- policy/approval/capability checks планов 09–10;
- отдельный план 07-4 для tool-focused telemetry.

План 12 задаёт общий evaluation/reporting слой и не дублирует tool-specific
cardinality или UI из плана 07-4.

## Границы

Входит: run/model/tool/result schema, token/cost/latency/retry metrics,
recorded inputs, replay, state predicates, adversarial scenarios, repeated
trials, report provenance и release gates.

Не входит: внешний telemetry backend, production side effects из evaluation,
LLM judge как единственное доказательство или сохранение секретов/raw prompt.

## Зависимости

### Блокирующие

- планы 08–11 для event/receipt linkage, policy, IPC и memory/RAG evidence;
- существующие Core event journal, redaction/retention и CI test harness.

### Опциональные

- 07-4 может поставлять tool-specific metrics; без него план использует
  bounded generic model/tool events;
- общий workflow/evaluation harness (`crates/evohime-core/src/evals.rs`,
  `tests/evals/`) может быть общим runner; если отдельная fixture-группа ещё
  не подключена, запускаются bounded standalone fixtures.

## Этапы

- [12-1 — telemetry schema и metrics](12-1-telemetry-schema.md)
- [12-2 — deterministic replay harness](12-2-evaluation-harness.md)
- [12-3 — adversarial scenarios и reports](12-3-adversarial-reports.md)
- [12-4 — release gates и acceptance](12-4-telemetry-acceptance.md)

Порядок: 12-1 → 12-2 → 12-3 → 12-4.

## Готово, когда

Любой report связан с исходными events и fixture, metrics работают локально,
release gate использует явные deterministic thresholds, а telemetry bounded,
redacted и retention-aware.
