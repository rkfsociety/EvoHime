# План 195.3 — extension catalog и workflow builder UI

Статус: active implementation contract. Этап следует после 195.1–195.2.

## Scope и изменяемые контракты
1. Добавить в существующий UI каталог capabilities/extensions с фильтрами по source, trust, compatibility и status. Не создавать отдельный plugin shell.
2. Показывать descriptor identity/version, назначение, required capabilities, dependencies, adapter availability, trust and compatibility state. Не показывать секреты и внутренние пути.
3. В существующий Guided Capability Recipes/Workflow Builder добавить композицию nodes из доступных descriptors: typed inputs/outputs, dependencies, optional branches, retry class и evidence requirements.
4. Preflight до запуска показывает missing/incompatible dependencies, grants, approvals и resource constraints. Renderer отображает Core result и не вычисляет policy самостоятельно.
5. Установка/обновление/disable доступны только через existing component lifecycle и user approvals. V1 может быть read-only для неподдерживаемых lifecycle actions.
6. Сохранить pinned version/hash и предупреждать, когда новый descriptor несовместим с сохранённым recipe; не менять его молча.
7. Добавить состояния empty/loading/unavailable/untrusted/incompatible/disabled и ошибки preflight.

## Зависимости
### Блокирующие
195.1–195.2; existing capability/recipe/workflow IPC and renderer patterns; component manifest/update and approval UI contracts.
### Опциональные
План 193 team workflow views; план 194 job status links.

## Recovery и rollback
Older clients ignore additive metadata/commands. If catalog endpoint is unavailable, UI shows unavailable, not empty. Stale descriptor revisions force refresh before save/run. Rollback removes new UI sections; saved workflow snapshots remain readable through existing runtime.

## Verification
- IPC schema/protocol compatibility.
- Renderer tests for catalog states, compatibility warnings, preflight, graph editing, pinned version and unavailable actions.
- Accessibility and keyboard tests for graph editor.
- Tests ensure UI cannot fabricate grants or bypass Core preflight.
- Bundle/static security checks confirm no executable plugin payloads.

## Release evidence
IPC tags/schema, catalog projection bounds, workflow editor tests, accessibility, preflight and redaction results, focused CI links.

## Критерии выхода
- [ ] Catalog reflects Core-owned descriptors and real adapter availability.
- [ ] Workflow builder composes only typed, available capabilities.
- [ ] Install/update actions use existing updater and approval gates.
- [ ] UI cannot grant permissions or run untrusted extensions.
