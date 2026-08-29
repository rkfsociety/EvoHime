# План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation

Статус: предложено по [issue #21](https://github.com/rkfsociety/EvoHime/issues/21). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime единый **Execution Policy Profile** для shell/process-based tools: Core выбирает и фиксирует способ запуска процесса, ограничения ресурсов, доступ к workspace/network/environment и cleanup semantics отдельно от текста команды.

Сейчас security boundary не должна заканчиваться на проверке «разрешено ли вызвать shell». После разрешения ещё важно **где и с какими OS-level правами** реально запускается процесс.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/execution_policy_profiles.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./41-1-execution-policy-profiles.md)
- [Этап 2 — runtime-интеграция и recovery](./41-2-execution-policy-profiles.md)
- [Этап 3 — IPC, client projection и UI](./41-3-execution-policy-profiles.md)
- [Этап 4 — verification, release-evidence и закрытие](./41-4-execution-policy-profiles.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
Tool request
 -> Capability / Approval
 -> ExecutionPolicy resolution
 -> process spawn
 -> bounded I/O
 -> process-tree cleanup
 -> typed result
```

Команда не выбирает собственную sandbox policy.

### Безопасность

- command text не выбирает policy;
- environment deny-by-default;
- secrets не наследуются автоматически;
- canonical paths/reparse points проверяются;
- required sandbox не downgrade-ится молча;
- process tree controlled;
- network restriction считается гарантией только если backend реально умеет её обеспечить;
- imported workflow/skill не регистрирует новый execution backend;
- renderer не запускает процессы напрямую.

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

- [ ] Есть versioned ExecutionPolicyProfile.
- [ ] Shell/process tools запускаются только через resolved profile.
- [ ] Environment inheritance deny-by-default.
- [ ] Есть process-tree lifecycle и bounded output/timeouts.
- [ ] Workspace/network permissions представлены явно.
- [ ] Required sandbox profile fail-closed при недоступности.
- [ ] UI/trace показывает фактический resolved execution profile.
- [ ] Windows-first restricted backend исследован и реализован хотя бы для одного практичного режима.

## Ограничения и non-goals

- обещать абсолютную security sandbox на любой Windows-конфигурации;
- Docker как обязательная зависимость;
- запуск произвольных untrusted kernel drivers/VM;
- хранение credentials в shell env;
- отмена approvals;
- разрешение model/skill самостоятельно выбирать менее строгий profile.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#21 Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation](https://github.com/rkfsociety/EvoHime/issues/21)
