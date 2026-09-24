# План 185.0 — Core-owned behavioral simulation harness для multi-turn evaluation

Статус: active implementation contract. Исходный issue #161 удаляется после сохранения плана; план не означает, что функциональность уже реализована.

## Цель

Создать local-first deterministic harness для versioned multi-turn сценариев с scripted actor, synthetic environment и typed observation transitions; turns оцениваются plan 184, общие результаты принадлежат Agent Benchmark Matrix.

## Основание checkout

Tool Simulation Runtime перехватывает tool calls, работает с exact fixtures и отвергает Real mode, но его run state process-local. Workflow/child runtime, eval CLI и Benchmark Matrix остаются владельцами исполнения и агрегации; durable turn checkpoints требуют расширения существующих owners.

## Граница

Frozen scenario/actor/environment → existing Core agent runtime → Tool Simulation Runtime → committed typed turn observation → plan 184 trace assertions → scenario outcome → Benchmark Matrix.

## Изменяемые контракты

- Versioned scenario, scripted/canned actor, typed observation predicates, synthetic environment and outcome contract.
- Frozen run/turn identity, per-turn trace/evidence refs and bounded scenario metrics using existing persistence owners.

## Recovery и rollback

Продолжать только после committed turn при совпадении scenario/actor/environment/model/policy/seed hashes. Mid-turn ambiguity не переисполняется. Отключение harness не должно включать real tool fallback.

## Этапы

- [Этап 1 — scenario/actor/environment/run contracts](./185-1-core-behavioral-simulation-harness.md)
- [Этап 2 — turn runtime, persistence и recovery](./185-2-core-behavioral-simulation-harness.md)
- [Этап 3 — scenario evaluator, trace/benchmark и CLI/CI](./185-3-core-behavioral-simulation-harness.md)
- [Этап 4 — determinism/security/recovery evidence и closure](./185-4-core-behavioral-simulation-harness.md)

## Зависимости

### Блокирующие

- План 184: deterministic trace evaluator.
- Tool Simulation Runtime, ToolRegistry/policy/approval, workflow/child runtime и EventJournal.
- Agent Benchmark Matrix и evohime-eval как единственные aggregation/CI owners.

### Опциональные

- Model-driven actor — optional non-authoritative extension; scripted/canned actor остаётся MVP.
- Планы 176/179 могут потреблять suites, но не блокируют harness.

## Критерии готовности

- [ ] Scenario/run snapshot versioned, bounded, content-addressed и frozen до запуска.
- [ ] Scripted mode deterministic; transitions используют typed Core observations, не prose parsing.
- [ ] Каждый turn имеет simulation provenance; real network/effects запрещены, missing fixture не вызывает real fallback.
- [ ] Synthetic approval scoped только exact simulation run и не является production grant.
- [ ] Crash resume начинается только с committed turn при совпадении snapshot hashes; ambiguous turn не повторяется.
- [ ] Trace report plan 184 и scenario outcomes входят в Benchmark attempt; CI outputs bounded/reproducible.

## Non-goals

- Обязательный LLM simulator, production data/credentials или real mutations in CI.
- Второй agent/workflow/trace/benchmark engine или automatic agent promotion.

## Verification

Плановые gates: deterministic actor/turn unit checks → synthetic-effect integration → trace/benchmark CLI/CI → crash, cancellation, security and Windows fixtures.

## Release evidence

Сохранять scenario/actor/environment/run hashes, committed turn boundary, trace/outcome refs и bounded per-scenario diagnostics. Synthetic provenance явна; transcripts и provider logs не архивируются.

## Критерии выхода overview

- [ ] Plan 184 и Tool Simulation Runtime owners доступны до запуска; scripted MVP независим от model actor.
- [ ] Turn commit/recovery, no-real-fallback и Benchmark ownership определены до этапа 1.

## Исходная постановка

- [Issue #161 — Core-owned behavioral simulation harness для multi-turn evaluation](https://github.com/rkfsociety/EvoHime/issues/161); сохранённый номер исходной постановки, issue удаляется после публикации плана.
