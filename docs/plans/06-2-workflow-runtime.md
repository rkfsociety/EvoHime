# План 06-2 — Durable runtime, Context Providers и интеграция с Core

## Цель

Подключить workflow graph к реальному Core execution path и сделать его
перезапускаемым, bounded и совместимым с текущими child/provenance/approval
контрактами.

## Зависимости

### Блокирующие

- [06-1](06-1-workflow-contract.md);
- SQLite migrations и storage transaction helpers;
- существующие child coordinator, `run_policy`, approval registry,
  cancellation и model-request provenance.

### Опциональные

- параллельный executor для независимых узлов. До его появления batch
  выполняется последовательно в стабильном порядке с теми же лимитами;
- расширенный RAG node. До его появления доступен read-only child/research
  node с текущим RAG API.

## Изменения

1. Добавить durable workflow run tables: immutable graph snapshot, node state,
   attempts, input/output references, approval state, lease, event sequence,
   cancellation marker и terminal outcome.
2. Сохранить state transitions атомарно: node не может перейти в `running`
   без valid parent run policy, а terminal event не может появиться без
   durable attempt/result marker.
3. Реализовать adapter layer:
   - `child` → typed child request/report;
   - `tool` → existing ToolRegistry and approval path;
   - `mcp_tool` → supervisor-owned trusted MCP session, Core ToolRegistry,
     bounded transport и тот же approval/receipt path;
   - `context_provider` → read-only Context Provider registry, bounded evidence,
     freshness/staleness gate и связь с Context Budget/RAG provenance;
   - `research` → existing bounded research/RAG path;
   - `approval` → pending approval registry;
   - `condition`/`transform` → deterministic Core-owned operations.
4. Реализовать bounded fan-out/fan-in. Независимые узлы могут выполняться
   параллельно только в пределах run policy, child budget и supervisor limits;
   fan-in принимает только validated reports с актуальной provenance.
   Stateful child capabilities и tools с side effects выполняются
   последовательно, если их adapter явно не объявляет безопасную
   concurrency-семантику.
5. Перед каждым effect повторно проверять graph hash, run policy, grants,
   selected capability, context allowlist и approval exact-call hash.
6. Реализовать recovery после Core restart: running nodes становятся
   `interrupted` или `unknown_outcome` по существующему provenance marker;
   blind retry запрещён.
7. Добавить bounded cancellation, timeout, retry/backoff и dead-letter для
   workflow-level failures, не смешивая их с child report status.
8. Публиковать typed durable events для timeline, replay и diagnostics.
   События получают устойчивую корреляцию `workflow_run_id`, `node_id`,
   `attempt_id`, `tool_call_id` и `model_request_id`; внешний tracing export
   остаётся optional и не является источником истины.

## Проверки

- миграции и rollback-safe startup recovery;
- crash injection до/после dispatch marker;
- fan-out/fan-in с перемешанным входным порядком и одинаковым результатом;
- retry только для разрешённых error classes;
- approval denial/pending, cancellation, timeout и dead-letter;
- проверка, что child не может поднять grants/budget или запустить nested child;
- MCP session restart, untrusted server rejection, tool allowlist и cancellation;
- Context Provider timeout, deleted source, stale evidence и unavailable source;
- повторный вызов stateful child capability не возникает из-за
  parallel tool calls или replay;
- `cargo test -p evohime-core -p evohime-local-storage`.

## Готово, когда

Один workflow run проходит через тот же authoritative Core path, что и обычная
задача: после рестарта он корректно возобновляется или становится безопасно
неопределённым, а ни один узел не выполняется в обход policy, approval,
provenance или receipts.
