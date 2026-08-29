# План 90.0 — Runtime Stall Guard: static blocking-I/O detection и CI runtime anchors

Статус: предложено по [issue #70](https://github.com/rkfsociety/EvoHime/issues/70). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Runtime Stall Guard**: отдельный инженерный контур для обнаружения и предотвращения синхронных/долго блокирующих операций на async/runtime/UI-critical путях.

Система должна сочетать два разных механизма:

1. **Static discovery**: широкое обнаружение подозрительных мест для последующего review.
2. **Runtime regression anchors**: точечные CI-тесты, которые защищают реальные production paths от повторного появления блокирующего поведения.

Это не общий performance profiler и не попытка доказать отсутствие любого blocking code во всём проекте.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/runtime-stall-guard.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 63.0 — Composable Termination Conditions: first-class stop policies for agent and team runs.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Static detector
 -> candidate findings
 -> human/code review
 -> identify high-risk production path
 -> runtime anchor
 -> CI regression gate
```

Static finding является **кандидатом**, а не доказательством бага.

Runtime anchor защищает подтверждённый путь, а не пытается интерсептить всё подряд.

### Безопасность

- detector не исполняет найденный code автоматически;
- production watchdog не пишет raw secrets/arguments;
- runtime anchor использует fixtures, а не реальные пользовательские credentials;
- offload worker не получает больше capabilities только потому, что работа вынесена с async thread;
- cancellation/unknown-outcome semantics сохраняются при переносе операций в blocking pool.

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

- [ ] Есть machine-readable static blocking-risk report.
- [ ] Static finding отделён от подтверждённой runtime regression.
- [ ] Есть focused runtime anchors для нескольких Core-critical paths.
- [ ] CI блокирует подтверждённые blocking regressions.
- [ ] Rust async и Electron main-process paths имеют соответствующие проверки.
- [ ] Suppressions explicit, reviewable и fingerprint-based.
- [ ] Diagnostics не содержат sensitive payload.

## Ограничения и non-goals

- полный универсальный profiler;
- запрет любого synchronous кода в проекте;
- автоматическое переписывание найденного кода;
- wall-clock benchmark всей системы в каждом unit test;
- считать каждый static candidate багом;
- отдельный distributed tracing backend только ради этой функции.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#70 Runtime Stall Guard: static blocking-I/O detection и CI runtime anchors](https://github.com/rkfsociety/EvoHime/issues/70)
