# План 63.0 — Composable Termination Conditions: first-class stop policies for agent and team runs

Статус: предложено по [issue #43](https://github.com/rkfsociety/EvoHime/issues/43). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime first-class систему **компонуемых условий завершения** для agent/team/workflow run. Условие остановки должно быть отдельным типизированным объектом, а не разрозненной логикой внутри конкретного оркестратора.

Это дополняет `Continuation Policy`: continuation решает, можно/нужно ли делать следующий автономный цикл, а termination conditions формально определяют, когда конкретный run должен завершиться, приостановиться или считаться исчерпавшим лимит.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/composable-termination-conditions.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 25.0 — Persistent Goals: durable цели для долгих задач.
- План 26.0 — Continuation Policy: bounded autonomous loops и quality gates.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 62.0 — Team Resource Budget: shared cost envelope, per-role allocations и reserved verification budget.
- План 79.0 — Team Coordinator: capability-aware delegation, dynamic task routing и managerial validation.
- План 99.0 — Composable Termination Conditions: typed stop algebra для agent/team runs.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

Предлагаемый контракт:

```rust
trait TerminationCondition {
    fn evaluate(
        &mut self,
        event: &AgentRuntimeEvent,
        ctx: &TerminationContext,
    ) -> TerminationDecision;

    fn snapshot(&self) -> TerminationState;
    fn restore(&mut self, state: TerminationState) -> Result<()>;
    fn reset(&mut self);
}

enum TerminationDecision {
    Continue,
    Complete { reason: TerminationReason },
    Pause { reason: TerminationReason },
    Fail { reason: TerminationReason },
}
```

Условия должны быть **stateful**, persistable и replay-safe.

### Безопасность

- Агент не может сам увеличить лимит termination condition без разрешённой capability/policy.
- Child не может ослабить inherited hard limits.
- Security/approval boundaries имеют более высокий приоритет, чем пользовательские termination rules.
- Budget values нельзя брать из текста модели.

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

- [ ] Termination condition является versioned typed contract
- [ ] Есть минимум перечисленные built-ins
- [ ] Есть `Any`/`All` composition
- [ ] State восстанавливается после restart
- [ ] Trigger reason полностью аудируем
- [ ] UI показывает условия и причину остановки
- [ ] Autonomous continuation не может обойти сработавший hard stop

## Ограничения и non-goals

- Не заменять workflow state machine.
- Не переносить business logic в termination predicates.
- Не давать модели произвольный executable callback внутри Core.
- Не дублировать quality-gate orchestration из #6: здесь нужен общий контракт завершения.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#43 Composable Termination Conditions: first-class stop policies for agent and team runs](https://github.com/rkfsociety/EvoHime/issues/43)
