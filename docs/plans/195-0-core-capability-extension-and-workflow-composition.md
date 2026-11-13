# План 195.0 — Core-owned capability extension contracts и workflow composition

Статус: active implementation contract. План использует существующие capability, skill, tool и workflow owners; динамическое исполнение произвольных plugins не входит в scope.

## Цель
Сделать возможности EvoHime модульными и композиционными: описывать их версионированными контрактами, проверять совместимость/permissions до исполнения, подключать только доверенные и установленные adapters, собирать повторно используемые workflows из существующих capabilities и показывать lifecycle расширений.

## Основание checkout
Уже существуют ToolRegistry/ToolManifest, SkillRegistry и trust pipeline, Capability Registry/Facade, context loadouts, Guided Capability Recipes, workflow runtime, ArtifactStore, permission/approval и component release manifest. План связывает эти owners общими versioned descriptors и composition contracts; не создаёт второй plugin manager, tool registry, skill registry, workflow engine или updater.

## Граница
- Capability descriptor — декларативные metadata/schema/requirements, совместимость, trust source, version и declared permissions.
- Реальное исполнение остаётся у заранее встроенного/установленного Core-owned adapter, прошедшего policy и sandbox.
- Skills остаются untrusted instructions, не исполняют код и не расширяют grants.
- Workflow composition строится на существующем workflow runtime и ArtifactStore; каждый node сохраняет provenance и approval boundary.
- Extension lifecycle переиспользует component manifest/updater; второго update channel нет.

## Изменяемые контракты
- Versioned capability descriptor и compatibility negotiation.
- Descriptor validation: identity, version, input/output schema hashes, dependencies, trust state, capability requirements и deprecation.
- Workflow composition references на capability ids/versions, typed inputs/outputs, dependencies, grants, retry semantics и evidence refs.
- Extension catalog projection: installed/available/disabled/incompatible/untrusted/unavailable.
- Policy decision и approval остаются Core-owned и не выводятся из manifest.

## Зависимости
### Блокирующие
ToolRegistry/ToolManifest, PermissionEngine, approval/receipts и sandbox; Capability Registry/Facade и Agent Context Loadouts; SkillRegistry/trust pipeline и plan 138; Guided Capability Recipes, workflow runtime, ArtifactStore/provenance; component manifest/update/recovery contract plan 144; existing IPC and release workflow.
### Опциональные
План 193 может потреблять compatibility descriptors для delegation; план 194 может отображать workflow/extension origin; план 180 может использовать descriptor intersection без передачи local grants.

## Recovery и rollback
Unknown/invalid descriptor не регистрируется. Missing adapter даёт typed unavailable; manifest presence не означает executable capability. Workflow snapshots pin descriptor/schema hashes; обновление extension не меняет запущенный workflow. Rollback использует existing component transaction/manifest, возвращает предыдущую совместимую версию и не удаляет workflow history/artifacts.

## Этапы
- [Этап 1 — descriptor, trust и compatibility contracts](./195-1-core-capability-extension-and-workflow-composition.md)
- [Этап 2 — Core registry integration и workflow composition](./195-2-core-capability-extension-and-workflow-composition.md)
- [Этап 3 — extension catalog и workflow builder UI](./195-3-core-capability-extension-and-workflow-composition.md)
- [Этап 4 — verification, release evidence и closure](./195-4-core-capability-extension-and-workflow-composition.md)

## Критерии готовности
- [ ] Единственный descriptor/registry owner определён и не дублирует ToolRegistry, Capability Registry, SkillRegistry или workflow runtime.
- [ ] Descriptor не предоставляет executable code, permissions, secrets или filesystem/network access.
- [ ] Compatibility/trust/schema checks выполняются Core до регистрации и каждого execution boundary.
- [ ] Workflow nodes pin descriptor/schema hashes и сохраняют provenance.
- [ ] Missing/incompatible/untrusted adapters отображаются typed unavailable.
- [ ] Existing component updater является единственным update lifecycle.
- [ ] UI отображает возможности и состав workflows без передачи authority renderer.
- [ ] Security, rollback, schema and release evidence tests подтверждены.

## Non-goals
Универсальная динамическая загрузка произвольных бинарников/скриптов; исполнение кода из skill/plugin manifest или marketplace packages; автоматическая установка/обновление без подтверждения и existing gates; новый plugin manager, package store, updater, tool registry или workflow scheduler; обход approval, sandbox, credentials policy или user consent.

## Verification и release evidence
Descriptor/schema validation → compatibility/trust matrix → workflow composition/deterministic execution → update rollback/failure injection → IPC/UI and CI acceptance. Хранить descriptor/schema hashes, adapter identity/version, trust and compatibility outcomes, workflow graph hash, provenance refs, update transaction refs и redacted CI results.

## Критерии выхода overview
- [ ] Declarative extensibility без arbitrary code execution.
- [ ] Существующие capability/skill/workflow/update owners остаются едиными.
- [ ] Старые workflow snapshots сохраняют pinned semantics при обновлении.
