# План 116.2 — Local Model Runtime Manager: runtime-интеграция и recovery

Статус: этап 2 для [плана 116.0](./116-0-local-model-runtime-manager.md); после [плана 116.1](./116-1-local-model-runtime-manager.md).

## Цель

Провести manager через `validate -> policy -> bounded operation -> typed result/event`
для hardware refresh, recommendation, verified artifact install, supervised runtime,
health gate, ModelProfile registration и bootstrap activation.

## Зависимости

### Блокирующие

- План 116.1 — contract, storage, trust и errors.
- Supervisor Job Object/process boundary, Execution Backend Registry, Model Gateway,
  Model Resilience Policy, Context Budget, cancellation/budget/audit и provenance.

### Опциональные

- Plans 36, 41, 46, 53 и 105; optional backend/benchmark/diagnostic surfaces дают
  typed unavailable/degraded result, а не implicit success.

## Реализация

1. Реализовать bounded hardware profiler с deterministic fixture adapter для CPU,
   RAM, accelerator/driver/runtime candidates и disk preflight. Реальный Windows
   probe не должен отдавать внешние identifiers или блокировать Core.
2. Реализовать artifact manager: Core-owned destination, bounded concurrency,
   expected size/hash, safe resumable download, cancellation, staging cleanup,
   streaming verification и atomic promotion. Unknown transport outcome не повторять
   вслепую; partial file не регистрировать.
3. Реализовать runtime registry/adapter для allowlisted supervised OpenAI-compatible
   loopback MVP: exact executable/version/hash, argv без shell, bounded environment,
   stdout/stderr, startup/load/probe timeouts, no credentials, Job Object cleanup.
   Protocol handshake обязан подтвердить model identity, capabilities и context.
4. Реализовать session manager и health gate; только после probe создать/обновить
   обычный local-managed ModelProfile. Перед каждым call повторить policy,
   capability, locality, budget и active-context checks.
5. Реализовать bootstrap: сначала compatible small descriptor, preferred download
   параллельно в рамках budget; health failure оставляет bootstrap active. Activation
   policy и purpose routing разрешают смену только на следующей eligible call, а
   strict run/conversation snapshot сохраняется.
6. Реализовать bounded resource ownership: max loaded models/reserved memory,
   idle unload, active-call leases и no eviction in-flight. Crashed/unknown runtime
   становится unavailable до новой проверки.
7. Реализовать recovery: restart повторно проверяет installed hash/metadata, не
   считает stale Ready/Loading живым, reconciles orphan process по доказанной
   identity, восстанавливает durable preference, а download resume допускает только
   безопасный path. Delete/update блокируются или откладываются при active session.

## Fault/recovery matrix

- crash до dispatch / после dispatch → unknown/reconciliation, без blind retry;
- timeout/cancel/duplicate/stale lease → отдельный typed outcome;
- wrong hash, disk full, interrupted download → cleanup, no Installed;
- executable/version/protocol mismatch → runtime unavailable;
- load OOM/probe failure → Failed с typed reason, bootstrap не теряется;
- restart/orphan/active eviction → no stale Ready и no in-flight termination.

## Критерии выхода

- [ ] Happy path выдаёт Ready только после всех health gates.
- [ ] Artifact promotion, cancellation, resume и failure outcomes воспроизводимы.
- [ ] Runtime process ограничен supervisor boundary и не видит secrets/workspace/tools.
- [ ] Managed profile использует active safe context и resilience policy.
- [ ] Bootstrap/switch/strict snapshot/resource eviction/restart доказаны tests.

## Не входит

Новый transport, arbitrary backend installation, cloud provider, renderer authority,
benchmark promise по названию GPU и automatic custom-code execution.

