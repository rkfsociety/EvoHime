# План: Evaluation catalog и smoke-gates

Статус: draft для ревью.

## Цель

Создать повторяемую систему проверки Евы на трёх уровнях: быстрые unit/contract
tests, offline evaluation Core/model behavior и дешёвые smoke-gates после
сборки или запуска packaged desktop runtime.

## Политика

Smoke-тест не доказывает качество, а evaluation не заменяет security tests.
Новая версия не считается готовой, если обязательные deterministic gates не
пройдены или результат не имеет вердикта.

## Каталоги

Разместить fixtures в `tests/evals/` или другом согласованном каталоге с
versioned JSON schema:

- `tool-use.json` — корректная схема, один tool за шаг, retry после ошибки;
- `workspace-study.json` — list/read/search порядок и citations;
- `memory.json` — retrieve, confirm, conflict, forget и scope isolation;
- `approval.json` — pause, deny, approve, replay и exact-call hash;
- `recovery.json` — Core restart, cancellation, lease loss и resume;
- `rag.json` — answer grounded in selected chunks;
- `security.json` — prompt injection, secret redaction, sandbox escape;
- `routing.json` — local/cloud route, fallback и budget;
- `child-workflows.json` — handoff, typed report, reject malformed output.

## Формат сценария

Каждый case содержит `id`, `prompt`, workspace fixture, expected events,
required tool calls, forbidden tool calls, expected final state, budget limit и
assertions. Секреты и реальные пользовательские данные запрещены.

## Этапы

### 1. Schema и runner

- Ввести JSON schema и Rust loader с bounded counts/strings.
- Runner должен уметь mock model, deterministic tool registry и fixture DB.
- Каждый case сохраняет только redacted event trace, hashes и verdict.
- Различать `pass`, `fail`, `blocked`, `skipped` и `no_verdict`.

### 2. Offline evaluation

- Запускать на каждый Core change и отдельно на model/provider matrix.
- Проверять не только текст ответа, но и event sequence, policy decisions,
  citations, memory writes, approval state и tool arguments.
- Для model-dependent cases использовать thresholds, а для protocol/security
  cases — exact deterministic assertions.
- Новые production failures добавлять как regression fixtures.

### 3. Smoke-gates

- После `cargo build`, native package и installed launch выполнять быстрые
  проверки: Core reachable, authenticated IPC, one read-only task, one
  approval-required task и clean shutdown.
- Smoke должен иметь таймаут, cleanup и понятный failure artifact.
- Не считать зелёный packaging достаточным, если endpoint/Core не отвечает.

### 4. Promotion policy

- Gate A: compile, format/diff checks, unit and contract tests.
- Gate B: offline deterministic evaluation.
- Gate C: Windows package/startup/IPC/recovery smoke.
- Gate D: optional provider/model evaluation; failure не маскировать как pass.
- В CI публиковать summary с commit, model route, fixture version и counts.

## Критерии готовности

- каждый новый agent feature имеет хотя бы один fixture и regression case;
- failure можно воспроизвести локально одной командой;
- smoke проверяет реальный packaged Core, а не только mock;
- результаты различают skipped и passed;
- eval logs не содержат raw secrets, prompts или memory bodies.

## Зависимости

Использует существующие Rust/Electron/native checks, event replay, approval,
recovery и diagnostics. До реализации Agentic RAG/receipts можно начать с
tool-use, memory и security fixtures.
