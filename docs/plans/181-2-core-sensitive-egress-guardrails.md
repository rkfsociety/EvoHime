# План 181.2 — Routing и final dispatch gate

## Изменить

1. Add classification pass after final context selection and before existing
   ModelGateway route resolution; combine task/profile, context labels and
   content-derived privacy with monotonic `max` semantics.
2. Extend `RoutingRequest`/route snapshot integration with typed destination
   metadata without moving route authority into guardrails. Re-evaluate privacy
   for each fallback candidate; weaker privacy fallback is rejected.
3. Add route-aware pre-dispatch gate after final serialized semantic payload is
   built and before auth/transport wrapping. Bind decision to payload, route,
   policy and detector hashes.
4. For transforms, produce a new payload/hash and rerun schema/size checks;
   provider/tool/remote-agent effect is possible only after terminal allow.
5. Integrate tool input/output and external-agent boundaries using the same
   typed destination contract; local trusted model semantics may differ from
   cloud/external but secrets retain stricter policy.

## Зависимости

### Блокирующие

- Plan 181.1, `ModelGateway::RoutingRequest`/routing policy and existing Core
  context/execution paths (`core_agent_context`/execution).

### Опциональные

- A2A-specific adapter integration can consume the same gate after plan 180;
  no new remote sanitizer or provider registry is permitted.

## Проверка

Routing tests prove PII raises privacy, strong upstream labels persist, eligible
high-privacy route passes, weaker fallback/stale provider metadata blocks, and
local trusted path preserves allowed PII without weakening secret policy.
Pre-dispatch tests mutate payload/route after scan, verify transform recheck,
approval invalidation and zero provider calls on detector overflow/error.

## Rollback и evidence

Keep current v1 gate as conservative fallback while integrating each dispatch
path. Never enable a path that lacks exact final scan evidence; fail closed with
typed reason instead of falling back to a weaker route.
