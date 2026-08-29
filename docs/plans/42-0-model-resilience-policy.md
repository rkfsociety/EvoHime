# План 42.0 — Model Resilience Policy: retry, fallback и provider-safe request adaptation

Статус: предложено по [issue #22](https://github.com/rkfsociety/EvoHime/issues/22). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime отдельный **Model Resilience Policy** поверх текущего Model Gateway: transient provider/model failures обрабатываются bounded retry и, при разрешении policy, переключением на совместимый fallback model profile без потери contract semantics и без передачи provider-specific мусора между несовместимыми API.

Это не automatic model routing по качеству и не ContinuationPolicy всей задачи. Это локальная reliability policy одного model call.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/model-resilience-policy.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./42-1-model-resilience-policy.md)
- [Этап 2 — runtime-интеграция и recovery](./42-2-model-resilience-policy.md)
- [Этап 3 — IPC, client projection и UI](./42-3-model-resilience-policy.md)
- [Этап 4 — verification, release-evidence и закрытие](./42-4-model-resilience-policy.md)

## Зависимости

### Блокирующие

- План 39.0 — Structured Response Contract: schema-first ответы модели с provider/tool fallback.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 36.0 — Agent Benchmark Matrix: многократные model/strategy evals и regression tracking.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- fallback только на allowlisted ModelProfile;
- data residency/sensitivity проверяется заново;
- fallback не расширяет tool grants;
- canonical request пересобирается provider adapter-ом;
- credentials одного provider не передаются другому;
- provider-specific metadata не протекает между routes;
- retry budgets bounded;
- policy/user cancellation прерывает backoff.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть versioned ModelResiliencePolicy.
- [ ] Retry использует normalized error classes и bounded backoff.
- [ ] Fallback models представлены ModelProfile, а не raw strings.
- [ ] Compatibility проверяется до fallback call.
- [ ] Provider payload пересобирается из canonical request.
- [ ] Sensitivity/data residency может запретить fallback.
- [ ] Есть retry/fallback budgets и cancellation.
- [ ] Attempts видны в diagnostics/provenance.

## Ограничения и non-goals

- автоматический выбор «лучшей» модели по качеству;
- скрытая смена provider вопреки user/data policy;
- бесконечные retries;
- provider-specific exception types в Core policy;
- перенос credentials между providers;
- использование fallback как способа обойти unsupported security/tool constraints.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#22 Model Resilience Policy: retry, fallback и provider-safe request adaptation](https://github.com/rkfsociety/EvoHime/issues/22)
