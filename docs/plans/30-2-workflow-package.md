# План 30.2 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей: runtime-интеграция и recovery

Статус: этап 2 для [плана 30.0](./30-0-workflow-package.md); после [плана 30.1](./30-1-workflow-package.md).

## Цель

Провести package operations через Core: bounded export и pipeline
`size/type -> parse -> format/version -> canonical hash -> security scan ->
dependency resolution -> capability compatibility -> credential rebinding plan
-> preview -> explicit commit`. Этот этап не запускает workflow и не создаёт
schedule/trigger.

## Зависимости

### Блокирующие

- План 30.1 — contract, validators, storage policy и errors.
- Existing `WorkflowGraph`/`WorkflowRegistry`, Core file boundary, transaction,
  audit и authenticated command path. Workflow run leases/retry не являются
  зависимостью package import.

### Опциональные

- Нет дополнительных межплановых зависимостей.

## Реализация

0. Загрузить stage-1 contract и проверить bounded input/file policy до parse:
   canonicalized path, allow-listed JSON extension, size limit и отсутствие
   traversal/archive extraction.
1. Реализовать Core export projection и canonical package hash; redaction идёт
   по sensitivity/portable metadata, не по поиску строк `token`.
2. Реализовать import resolution report со статусами `resolved`, compatible
   alternative, missing, version/schema mismatch, permission unavailable и
   requires user binding; неизвестное не подменять по похожему имени.
3. Реализовать preview как read-only результат. Только explicit commit после
   повторной policy/registry validation создаёт новую local workflow identity
   и immutable version; credential rebind сохраняет только local reference.
4. Зафиксировать atomic DB/file commit, idempotency по hash и recovery для
   crash до/после commit. При неясном результате возвращать unknown/reconcile,
   не повторять запись вслепую; повтор commit того же hash возвращает уже
   существующую local version.
5. Не создавать capability, schedule, trigger, run, lease, approval или
   external effect на parse/preview/import.

## Предметная декомпозиция

### Runtime vertical slice

- Entrypoint: `crates/evohime-core/src/workflow_package.rs` + Core command
  handler; service выполняет только `validate → resolve → preview → commit`.
- Export читает immutable definition, а commit создаёт новую local identity;
  running graph и existing registry entries не изменяются.
- Тесты: `workflow_package_runtime.rs` — preview без записи/эффекта,
  duplicate hash, missing/mismatch/rebind, crash до/после atomic commit,
  malformed/oversized input, traversal и unknown capability.

### Acceptance-to-runtime matrix

- `C02` — Export удаляет credentials/secrets/runtime-specific state. →
  доказать redacted projection и отсутствие package side effect.
- `C04` — Import выполняет validate/resolve/preview до записи. → журналировать
  bounded phase transitions, но commit разрешать только explicit action.

### Recovery contract

- Durable import transition/history восстанавливается replay/reconciliation;
  partial commit не создаёт вторую workflow version.
- Fault injection должна доказать отсутствие partial write, duplicate import,
  capability registration, schedule/trigger/run creation и secret leakage.

## Критерии выхода

- [ ] Happy path выдаёт typed result только после Core validation.
- [ ] Duplicate/mismatch/limit/denied/restart/unavailable имеют отдельные
  package outcomes.
- [ ] Unknown commit outcome не повторяется автоматически.
- [ ] Preview не создаёт durable/external effect, а commit атомарен.
- [ ] Recovery/fault-injection tests воспроизводимы.

## Не входит

Client authority, direct UI/storage access, security-policy weakening и необъявленный network/runtime.
