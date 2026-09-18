# План 179.3 — Evaluation, promotion и IPC

## Изменить

1. Normalize Workflow Optimization Lab candidates into validated profiles; Lab
   may search/evaluate but cannot write production-promoted registry state.
2. Extend Agent Benchmark Matrix identity with strategy hash and require same
   suite/policy/model profile for baseline/candidate comparisons. Holdout,
   security regression, cost/latency and evidence compatibility gate promotion.
3. Integrate plan 176 guided recipes with `PromptStrategyRef`/pins and prevent
   recipe forks from copying sensitive prompt assets without provenance.
4. Add authenticated additive IPC for list/get/compatibility/evidence/compare
   and explicit promote/supersede/disable; inspect historical selection reason.
5. Add bounded Electron projection in existing developer/operations/evaluation
   surfaces. No raw system prompt, reusable sensitive fragment or renderer
   promotion authority.

## Зависимости

### Блокирующие

- Plan 179.1–179.2, existing benchmark/optimization/evidence owners and
  authenticated IPC; plan 176 guided recipe contract.

### Опциональные

- UI comparison can initially be read-only; Core tests must cover explicit
  promotion independently.

## Проверка

Test candidate self-promotion rejection, frozen suite/holdout, security gate,
incompatible evidence, deterministic concurrent promotion conflict, protocol
compatibility, bounded projection and reconnect/replay without mutation.

## Rollback и evidence

Rejected candidates remain offline metadata; disable/supersede does not rewrite
historical run records. A failed IPC/UI gate leaves the Core registry disabled
or unchanged and keeps the plan active.
