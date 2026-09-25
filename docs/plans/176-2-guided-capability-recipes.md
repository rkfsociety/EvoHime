# План 176.2 — Guided Capability Recipes: preflight, run и recovery

## Зависимости

### Блокирующие

- [Core catalog и storage](./176-1-guided-capability-recipes.md).
- Exact existing `WorkflowRuntime` template/package binding, model/context
  provenance, tool policy, approval and background recovery owners.
- The recipe/run sidecar is committed with the existing workflow run before
  `spawn_workflow_drive`; it does not own lifecycle transitions.

### Опциональные

- Local model/hardware-fit and vector/RAG availability.

## Реализация

- Перед запуском выполнять bounded preflight для точного binding: required и
  optional capabilities, provider/model/context revisions, budgets,
  policy/approval gates и degraded paths. Статусы: `Ready`,
  `ReadyWithWarnings`, `Blocked`, `Unsupported`, `InvalidDefinition`.
  Preflight payload не содержит raw graph, prompt, credentials или outputs.
- Перед `start` повторно валидировать binding, grants, approval requirements и
  resolved revisions, чтобы устаревший preflight не обходил изменившуюся
  policy. Создать workflow snapshot и recipe sidecar idempotently/атомарно до
  запуска внешних effects; lifecycle и recovery остаются в
  `WorkflowRuntime`.
- Рецепт можно запускать только через уже существующий workflow graph/package
  и его зарегистрированные owners. Benchmark, model-compare, local-fit,
  structured-response и другие capability contracts не становятся
  исполняемыми workflow adapters автоматически; fixture-only результаты
  показываются только как fixture evidence, иначе binding получает
  `Unsupported`.
- Разделить `ReproduceExact`, `ReRunWithCurrentCompatible` и
  `ForkAndModify`. Exact повторно использует сохранённый workflow snapshot и
  доступные pinned provenance; если существенная revision недоступна или не
  была записана owner-ом, вернуть typed unavailable вместо `latest`.
  Compatible rerun явно перечисляет и сохраняет отличающиеся compatible
  revisions; policy/grants всё равно проходят свежую проверку.
- Текущие child/model и context/evidence owners не сохраняют revision pins в
  recipe run. До появления такого контракта `ReproduceExact` и
  `ReRunWithCurrentCompatible` должны отображаться отдельными typed
  `unavailable` вариантами со стабильными reason codes; не запускать повторно
  с текущим model/context под видом exact или compatible replay. Preflight
  перечисляет известные recipe/template/graph pins, отсутствующие dynamic
  owner revisions, graph budget, approval points и degraded paths.
- `ForkAndModify` передаёт безопасный workflow definition в существующий
  user-owned draft/package path. Fork — ещё не run; запуск требует нового
  preflight и не переносит secrets, grants или approval.
- Не создавать recipe lifecycle events, event cursor, lease, scheduler или
  status machine. Чтение статуса, cancellation и restart/recovery делегировать
  существующему workflow/background owner. Подключить existing workflow
  recovery до IPC startup; не скрывать ошибку recovery и не принимать команды
  при неизвестном восстановленном состоянии. UI не запрашивает generic events
  с `payload_json`.

## Критерии

- Recipe cannot grant capabilities, bypass approval, replace required tools or
  silently fall back to a different provider/model/context.
- Every accepted run has exactly one immutable recipe link and workflow
  snapshot before drive starts; duplicate requests return the same link.
- Run status/cancellation/recovery are read from the workflow owner after
  restart; Core invokes that recovery before accepting IPC, and no
  recipe-owned lifecycle can disagree with the run.
- Exact replay either uses the original pinned snapshot/provenance or returns a
  typed unavailable result; it never silently selects `latest`.
- Retrieved/tool text is data-not-instructions and cannot alter recipe policy.
- Recipe lifecycle/recovery is subordinate to the existing workflow and
  background execution owners; no parallel recipe lease or scheduler is added.

## Non-goals

Новый execution scheduler, hidden multi-agent memory and automatic workflow
publication.
