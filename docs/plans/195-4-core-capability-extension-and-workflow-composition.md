# План 195.4 — verification, release evidence и закрытие

Статус: active implementation contract. Этап следует после 195.1–195.3.

## Scope и изменяемые контракты
1. Проверить descriptor/schema compatibility, trust/source validation, adapter availability и deprecation behavior.
2. Проверить workflow composition: deterministic graph hashes, typed node I/O, ArtifactStore provenance, dependency ordering, approval isolation, retry semantics и pinned versions.
3. Fault-injection: missing adapter, invalid/revoked trust, update during run, stale descriptor, incompatible schema, crash/restart и ambiguous mutation.
4. Проверить UI catalog/editor: no executable payloads, no grant escalation, accurate unavailable states, keyboard/accessibility and bundle security.
5. Проверить rollback через existing component transaction и backward compatibility для сохранённых recipes/runs.
6. Обновить architecture/current-state/release-evidence/plans README по подтверждённому поведению; удалить временные plan files 195 после closure.

## Зависимости
### Блокирующие
Завершённые 195.1–195.3, focused Core/workflow/IPC/renderer tests и CI.
### Опциональные
План 193 multi-agent coordination и план 194 background jobs UI.

## Recovery и rollback
План остаётся active при неподтверждённом trust, update, workflow recovery или security criterion. Rollback использует existing component updater; не удаляет descriptors/history/artifacts, пока их использует pinned workflow.

## Verification
Schema/trust tests → registry/adapter integration → workflow deterministic and recovery matrix → updater rollback → IPC/renderer/accessibility/static security → CI and Windows acceptance. Соблюдать локальные ограничения AGENTS.md.

## Release evidence
Schema/descriptor hashes, compatibility/trust matrix, graph hashes, artifact provenance, retry/approval results, update rollback refs, renderer and CI evidence. Не сохранять executable payloads, secrets и raw user data.

## Критерии выхода
- [ ] Declarative extension model is secure and backward-compatible.
- [ ] No duplicate registry, workflow engine or updater exists.
- [ ] Workflow runs pin exact capability contracts they execute.
- [ ] All security/recovery/release gates pass and canonical docs are updated.
