# 12-3 — Adversarial scenarios и reports

## Цель

Проверить policy, approval, recovery и provenance на негативных сценариях и
собрать объяснимый локальный report.

## Изменения

1. Добавить adversarial fixtures для invalid manifest, approval
   approve/reject/expiry, path traversal, capability escalation и secret
   redaction.
2. Добавить сценарии timeout, retry, cancellation, provider failure,
   duplicate IPC delivery, restart recovery и receipt/hash mismatch.
3. Формировать report с pass/fail/unknown/degraded, evidence links, source
   events, fixture hash, thresholds и attribution причины.
4. Считать reliability по repeated trials; judge signal показывать отдельно и
   не использовать как единственное security доказательство.
5. Ограничить report payload и обеспечить retention/redaction до export.

## Проверки

- expected denial и отсутствие side effect;
- error attribution для policy/provider/storage/unknown;
- repeated trials и стабильность reliability counters;
- report provenance и отсутствие secret/PII leakage.

## Готово, когда

Каждая деградация объяснима через исходные events и fixture, а adversarial
evaluation не меняет production state.
