# План 176.4 — Guided Capability Recipes: verification и закрытие

## Зависимости

### Блокирующие

- [IPC, guided UI и fork](./176-3-guided-capability-recipes.md).
- Existing workflow/eval/RAG/tool/approval contracts, security policy, GitHub
  module workflows, module-router and canonical docs.

### Опциональные

- Deterministic provider/model fixtures for optional live integrations.

## Реализация

- Добавить automated tests для catalog/version immutability, preflight states,
  eight built-ins, exact workflow binding/grant subset, approval/denial,
  structured-output contract, RAG provenance и prompt-injection isolation.
- Проверить, что fixture-only model/prompt evaluation явно помечена fixture
  evidence, metadata-only model comparison не заявляется runnable, local-fit
  recipe не выдаёт evidence за право запуска, exact reproduction отвергает
  недоступные revisions, а fork strips secrets/grants и сохраняет provenance.
- Проверить атомарность workflow/recipe link до drive, idempotent retry,
  cancellation/restart через существующий workflow owner, missing-adapter
  отказ, bounded/redacted IPC/UI и отсутствие raw данных в recipe sidecar,
  recipe IPC и guided UI. UI сохраняет typed `Unsupported` как недоступный
  даже при противоречивом `ready` preflight; startup Core вызывает recovery
  существующего workflow owner до приёма IPC. Действующий workflow store
  сохраняет свои данные по прежнему контракту.
- Локальные tests/build/lint/smoke/acceptance не запускать. После push дождаться
  только CI затронутых модулей: `core.yml` при изменении Core/storage/IPC,
  `ui-bundle.yml` при изменении renderer/shared UI bundle, `shell-host.yml`
  только при изменении shell-host кода; `rustdoc.yml` — когда изменённые Rust
  API или docs подпадают под его paths. Проверить `module-router.yml` и не
  запускать `windows.yml`, native package или общий installer/package
  acceptance.
- Перенести contract/state/evidence в `docs/architecture.md`,
  `docs/current-state.md`, `docs/release-evidence.md` и
  `docs/development-plan.md`, проверить links и удалить `176-*.md` after real
  implementation closure.

## Критерии

- Все критерии обзора 176 подтверждены кодом и CI evidence.
- `git diff --check` проходит, task-only commit contains only related changes,
  historical issue #154 is not a closure gate.
- Fresh relevant module workflows succeeded for the pushed HEAD; no claim of
  native/package acceptance is made.

## Non-goals

Считать recipe documentation или model self-score доказательством успешного
runtime/quality результата.
