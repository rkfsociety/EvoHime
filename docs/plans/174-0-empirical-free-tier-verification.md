# План 174.0 — Empirical Free-Tier Verification

Статус: active implementation contract. Исторический источник постановки:
issue #152; функционал этим документом не считается реализованным.

## Цель

Добавить post-provider-profile слой, который отделяет advertised free от
фактически бесплатного account/provider/model route. `FreeOnly` должен
разрешаться только при свежем, scoped и семантически подтверждённом evidence;
eligibility, reliability и quality остаются разными сигналами.

Текущий `free_provider_reliability_routing` содержит лишь минимальные
`ProviderProfile`, `FreeAccessState` и `ReliabilitySnapshot`; он не доказывает
activation, allowance kind, observed limits или valid completion. План
расширяет #125 и зависит от provider-profile основы #105, не создавая второй
gateway, circuit breaker или credential store.

## Архитектурная граница

~~~text
catalog candidate -> Core scoped evidence -> bounded synthetic probe/runtime observation
-> FreeOnly eligibility -> existing resilience/routing -> redacted explanation
~~~

Проверка использует только synthetic payload, отдельный quota/token budget и
существующую credential/network policy. Signup, community, phone, payment и
dashboard activation автоматически не выполняются.

## Этапы

- [Этап 1 — evidence contract, schema и storage](./174-1-empirical-free-tier-verification.md)
- [Этап 2 — probes, observations и invalidation](./174-2-empirical-free-tier-verification.md)
- [Этап 3 — routing, IPC и UI](./174-3-empirical-free-tier-verification.md)
- [Этап 4 — verification, release evidence и закрытие](./174-4-empirical-free-tier-verification.md)

## Зависимости

### Блокирующие

- Завершённый provider-profile contract в
  [`../architecture.md`](../architecture.md), credential boundary, Model
  Gateway и #125 reliability/circuit/fallback.
- Existing Core policy, network capability, redaction, SQLite migration/backup
  и authenticated IPC/replay primitives.

### Опциональные

- Provider-declared headers/usage and dynamic catalog; при их отсутствии
  evidence остаётся bounded `Unknown`, а не угадывает лимиты.

## Критерии готовности

- [ ] Evidence scoped минимум provider + credential binding + region + model.
- [ ] Advertised/observed/activation/allowance/freshness разделены и
  versioned; signup credit не masquerade как recurring free tier.
- [ ] Успех probe требует schema-valid semantic completion или корректный
  tool/reasoning-only response, а не HTTP 2xx.
- [ ] Limits имеют scope, unit, source, confidence и observed/reset time.
- [ ] Strict `FreeOnly` fail-closed при stale/unknown/paid/trial/activation
  required; runtime 402/403/429 и catalog changes умеют invalidation/update.
- [ ] Текущие catalog labels, `mode=free` и суффикс `:free` остаются только
  advisory filter/display metadata и никогда не удовлетворяют `FreeOnly` без
  свежего empirical evidence.
- [ ] Нет пользовательских данных, secrets, raw responses или payment/signup
  actions в verification path.
- [ ] После реализации contract/state/evidence перенесены в canonical docs,
  исторический issue #152 не является gate; после фактического завершения
  `174-*.md` удалены.

## Non-goals

Гарантировать постоянную бесплатность provider, выполнять activation/payment,
заменять reliability/quality оценку, строить central account telemetry или
делать automatic paid fallback без explicit policy.

## Источник постановки

- issue #152 Empirical Free-Tier Verification (исторический идентификатор)
