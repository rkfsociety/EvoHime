# 11-1 — Typed memory lifecycle

## Цель

Зафиксировать versioned memory record и разделить scratch state, текущий
context и durable memory.

## Изменения

1. Ввести typed record с `type`, `scope`, `consent`, `provenance`, `confidence`,
   lifecycle state, TTL, source/evidence links и execution event references.
2. Разделить `scratch`, `session/context` и durable workspace/project memory;
   session notes автоматически истекают и не становятся persistent memory.
3. Проверять scope и consent до записи и до выдачи записи в context.
4. Thought или model output без validated evidence оставлять `unknown`/
   `pending`, а не превращать в факт или trusted memory.
5. Связать memory mutation с policy, approval и execution ledger; renderer
   получает только bounded metadata projection.

## Проверки

- schema/serialization round-trip и migration fixtures;
- scope/consent и cross-workspace isolation;
- TTL/lifecycle transitions и unknown fact handling;
- redaction secrets/PII до memory write;
- provenance linkage к event/action/observation.

## Готово, когда

Ни одна запись не появляется без scope, consent и provenance, а durable memory
невозможно создать простым model-generated assertion.
