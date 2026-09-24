# План 184.0 — Core-owned execution-trace evaluation и regression gates

Статус: active implementation contract. Исходный issue #160 удаляется после сохранения плана; план не означает, что функциональность уже реализована.

## Цель

Core нормализует фактически наблюдённые runtime events в versioned redacted ExecutionTrace и детерминированно проверяет tool sequence/args, approval/effect order, evidence-to-claim grounding, retry/recovery и bounded limits.

## Основание checkout

Agent Benchmark Matrix агрегирует attempts и сохраняет tool trace digest; evohime-eval и Core eval fixtures дают CLI/CI путь; EventJournal, execution ledger, tools, approvals и recovery уже имеют authoritative owners. Текущие fixture expectations не доказывают, что объявленная траектория совпала с наблюдённой.

## Граница

EventJournal/execution ledger → Core normalizer → immutable redacted trace → deterministic evaluator → bounded assertion report → existing Benchmark Matrix/evohime-eval.

## Изменяемые контракты

- Versioned trace, evaluation contract, typed assertions/results и redacted report.
- Совместимое расширение benchmark attempt evidence и существующей fixture schema; старый tool trace digest сохраняется.

## Recovery и rollback

Неполный/неизвестный trace никогда не pass. Frozen trace можно переоценивать идемпотентно. Rollback отключает новый evaluator/fixture version и оставляет текущий digest-only путь; raw trace и эффекты не воспроизводятся.

## Этапы

- [Этап 1 — trace contract, normalization и assertions](./184-1-core-execution-trace-evaluation.md)
- [Этап 2 — Core capture, evaluation lifecycle и recovery](./184-2-core-execution-trace-evaluation.md)
- [Этап 3 — evohime-eval, Benchmark Matrix и CI diagnostics](./184-3-core-execution-trace-evaluation.md)
- [Этап 4 — regressions, release evidence и closure](./184-4-core-execution-trace-evaluation.md)

## Зависимости

### Блокирующие

- EventJournal/effect ledger, ToolRegistry/runtime, approvals and recovery owners.
- Agent Benchmark Matrix как единственный suite/attempt/baseline owner.
- evohime-eval и versioned fixtures как единственный CI runner.

### Опциональные

- План 185 потребляет trace assertions для multi-turn сценариев.
- План 187 использует assertions для GUI regression fixtures.

## Критерии готовности

- [ ] Trace отражает observed events; fixture declarations сами по себе не дают pass.
- [ ] Versioned assertions проверяют порядок/подпоследовательность, canonical args, forbidden effects, approvals, evidence, retries и limits.
- [ ] Unknown/partial/malformed/unavailable/interrupted trace не даёт pass; effect до matching approval — hard failure.
- [ ] Benchmark Attempt получает hashes/refs и bounded per-assertion reason codes без raw trace payloads.
- [ ] Static/deterministic/real modes явные; несовместимые baseline contracts не сравниваются.
- [ ] Focused tests, fixture schema, CLI/CI diagnostics и canonical docs обновлены.

## Non-goals

- Второй benchmark/event/workflow/policy owner.
- Обязательный LLM judge, chain-of-thought, произвольный evaluator code, auto-promotion или полный replay debugger.

## Verification

Плановые gates: contract/hash/redaction tests → observed-trace integration с approvals/retries → evohime-eval/Benchmark/CI mode matrix → recovery and Windows checks. Проверки выполняются при реализации соответствующих этапов.

## Release evidence

Хранить contract/fixture versions, source run/attempt refs, trace/report hashes, bounded assertion outcomes и CI result; не сохранять raw prompts, args, reasoning или secrets.

## Критерии выхода overview

- [ ] Ownership, backward compatibility, failure semantics и stage dependencies согласованы до этапа 1.
- [ ] Каждый этап проверяет observed evidence; ни один не создаёт отдельный benchmark, trace или policy owner.

## Исходная постановка

- [Issue #160 — Core-owned execution-trace evaluation и regression gates](https://github.com/rkfsociety/EvoHime/issues/160); сохранённый номер исходной постановки, issue удаляется после публикации плана.
