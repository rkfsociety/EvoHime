# План 28.0 — Persistent Analysis Kernel

Статус: предложено по [issue #9](https://github.com/rkfsociety/EvoHime/issues/9).
Kernel — bounded вычислительная среда, а не security authority и не второй
unrestricted shell/network/file-system boundary.

## Цель и архитектурный принцип

Дать агенту долговременное in-session состояние для больших JSON/CSV, логов,
AST, индексов и промежуточных вычислений между model turns и compaction. Схема:
`Model → Kernel → typed host request → Core → Capability/Policy/Approval →
Tool/Workflow/MCP`. Kernel не получает полномочий; side effects остаются в Core.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./28-1-persistent-analysis-kernel.md)
- [Этап 2 — runtime-интеграция и recovery](./28-2-persistent-analysis-kernel.md)
- [Этап 3 — IPC, client projection и UI](./28-3-persistent-analysis-kernel.md)
- [Этап 4 — verification, release-evidence и закрытие](./28-4-persistent-analysis-kernel.md)

## Зависимости

### Блокирующие

- канонический TaskCheckpoint contract из закрытого плана 23
  ([`../architecture.md`](../architecture.md)) для object refs, compaction и
  recovery;
- существующий Core capability/tool/workflow registry и approval/audit path;
- существующий ArtifactStore с sensitivity, size и parent-scope checks;
- supervisor/process lifecycle, Job Object/resource limit primitives и crash
  recovery (`crates/evohime-supervisor`, `crates/evohime-core` launch/recovery
  path);
- authenticated versioned IPC и Core-owned SQLite.

### Опциональные

- существующие canonical contracts Agent Skills и Persistent Goals — только для
  optional linkage/helpers, без расширения permissions;
- закрытый retained-child contract плана 27
  ([`../architecture.md`](../architecture.md)) для immutable read-only object refs;
  он обязателен только для child handoff, прочие kernel scenarios от mailbox не
  зависят;
- план 26 Continuation Policy для bounded repeated kernel executions.

## Контракты и host bridge

Ввести versioned `KernelHostRequestV1`: request/kernel/session ids, operation,
bounded args, optional requested capability, context refs и correlation id.
Core возвращает `KernelHostResponseV1` со status, inline/ref result,
sensitivity, provenance и typed error class. Core заново проверяет parent grants,
capability, policy, approval, limits и object refs при каждом request.

`KernelObjectRefV1` содержит id/kernel id, logical name, type hint, size,
sensitivity, `ephemeral|checkpointed`, optional content hash и timestamps.
Object registry Core-visible, но values и process memory не отправляются в
renderer. Большие результаты offload-ятся только в существующий
`evohime-local-storage::artifact_store::ArtifactStore`; kernel не создаёт
второй blob store и сохраняет parent/context/sensitivity checks этого
контракта.

## Runtime и isolation decision

До реализации выбрать и зафиксировать runtime/ABI и threat model. Базовый MVP
требует отдельный worker/process под Core/Supervisor lifecycle, bounded CPU/time,
memory, output, object count/size, host-request rate и idle/lifetime budgets,
ограниченный environment/working directory, отсутствие credentials и лишних
handles, hard reset при превышении, а также изоляцию падения kernel от Core.

Если полноценный Windows sandbox недоступен, runtime явно маркируется
`trusted_local_analysis` и по умолчанию не получает workspace, network или shell
доступа. Нельзя выдавать kernel произвольные пути. Core передаёт bounded inline
payload, read-only temp snapshot, ArtifactStore ref или typed file handle с
normalized path, mode, limit, expiry, parent scope и sensitivity.

Прямые subprocess/shell/network API в MVP запрещены. `cargo`, `git`, `npm` и
внешние запросы оформляются typed host request через существующий Core tool/MCP
path с обычными approvals.

## Persistence, compaction и recovery

Runtime state (variables, safe modules, helpers, parsed datasets, handles) отделён
от durable manifest. После restart rehydrate разрешён только для валидируемых
persisted objects/artifacts с runtime/package version match; arbitrary process
memory не обещается как durable.

Перед compaction TaskCheckpoint получает logical names, refs/hashes, completed
computations и pending host requests. Model видит compact catalog, а не raw
objects. Child получает отдельное state либо immutable selected refs; mutable
namespace не делится.

Lifecycle/audit events: start/stop/crash/reset, host request, capability
resolution, object create/delete, limit exceeded, runtime/package version.
Pure computation не логируется покомандно, но boundary effects полностью
видимы. Stale runtime/version invalidates refs safely.

## Packages, UI и diagnostics

Разрешить только fixed allowlisted runtime modules/libraries с pinned version и
manifest. Автоматические `pip install`, downloads и package mutation отсутствуют.
UI/diagnostics показывает active/stopped/crashed, runtime version, CPU/memory
bounded metrics, object count, persisted objects, last execution, reset и typed
error; sensitive values скрыты.

## Этапы реализации

1. Threat model, runtime choice, ABI/version, capability matrix и limit defaults.
2. Typed bridge/object registry, SQLite manifest, ArtifactStore integration и
   deterministic serialization/hash.
3. Worker lifecycle, environment/handle isolation, resource enforcement, hard
   reset и crash/recovery reconciliation.
4. Bounded execution API, host request validation, compaction/checkpoint refs,
   package manifest и diagnostics/UI projection.
5. Adversarial tests/evals, включая direct FS/network/shell attempts, secrets,
   object overflow, unknown outcome, child isolation и stale version.

## Владельцы и handoff

- contract/validation/hash: новый `crates/evohime-core/src/analysis_kernel.rs`;
- durable manifest/object metadata: новый
  `crates/evohime-local-storage/src/analysis_kernel_store.rs`, подключённый к
  `LocalDatabase` migration ladder;
- worker launch, limits и reset: существующие Core/supervisor lifecycle paths,
  с отдельным allowlisted supervisor launch command и отдельным Job Object;
- authenticated command/event projection: `crates/desktop-ipc/proto`,
  `crates/evohime-core/src/ipc_bridge.rs` и Electron main/preload;
- checkpoint/child handoff: существующие TaskCheckpoint и plan-27 selected-ref
  contracts. Kernel не владеет approval, capability, scheduler или provider
  authority.

Текущая live schema — v37 после закрытия плана 27. План 28 получает фактическую
следующую свободную additive revision (ожидаемо v38) на evidence freeze и не
предполагает номер заранее.

## Критерии готовности

- controlled worker/runtime и persistent in-session state существуют;
- typed Core host bridge — единственный side-effect path;
- нет unrestricted shell/network/credentials/file access;
- limits, crash isolation, reset и recovery проверены;
- object registry, sensitivity/provenance и TaskCheckpoint integration работают;
- child получает только selected immutable refs;
- UI/diagnostics bounded и не раскрывает raw values;
- проходят resource, security, restart, compaction, approval и package manifest
  tests.

Каждый критерий закрывается строкой evidence matrix этапа 28.4. Отсутствие
optional Goal/Continuation/backend integration даёт typed `unavailable` или
`degraded`, а не success.

## Не входит

Python-first переписывание Core, unrestricted OS permissions, auto package
installation, прямой shell/network, общий mutable kernel, authoritative
persistence или перенос scheduler/security в kernel.
