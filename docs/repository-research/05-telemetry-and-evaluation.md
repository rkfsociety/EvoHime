# 05. Telemetry и evaluation

## Цель

Получить локально воспроизводимую оценку качества, безопасности и стоимости
агента без обязательной отправки данных во внешний telemetry backend.

## Scope

- схема `run → model/tool → result/error`;
- token, cost, latency, timeout, cancellation и retry metrics;
- scenario, session, action, observation, final-state и reward;
- deterministic fixtures, recorded inputs, replay и state predicates;
- adversarial user tasks, policy/approval checks и error attribution;
- повторные trials и reliability metrics;
- report provenance и evidence links;
- разделение advisory judge signal и release gate.

## Инварианты

- Telemetry принадлежит Core и подчиняется redaction/retention policy.
- Evaluation не выполняет production side effects.
- LLM-as-a-Judge не является единственным доказательством качества или
  безопасности.
- Невоспроизводимый model output классифицируется как unknown, а не как факт.
- Все результаты связаны с версией Core, схемой, provider/model и fixture.

## Тестовый контур

- replay одинакового scenario с одинаковыми recorded inputs;
- state predicate и expected action trace;
- adversarial approval/permission/path cases;
- timeout, retry, cancellation и partial failure;
- отсутствие secret/PII leakage в traces и reports;
- повторные trials с подсчётом pass rate и reliability;
- повреждённый или неполный trace как typed diagnostic error.

## Критерии готовности

- любой report можно связать с исходными событиями и fixture;
- metrics не требуют внешнего сервиса;
- release gate использует deterministic checks и явные пороги;
- telemetry redacted, bounded и retention-aware;
- evaluation fixtures запускаются в CI/offline режиме.

## Зависимости

Требует 01–03. Для 08 vision/document worker этот раздел также является
обязательным источником benchmark и quality gates.
