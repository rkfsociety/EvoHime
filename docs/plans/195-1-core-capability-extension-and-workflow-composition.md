# План 195.1 — descriptor, trust и compatibility contracts

Статус: active implementation contract. Этап следует после overview 195.0.

## Scope и изменяемые контракты
1. Инвентаризировать ToolManifest, Capability Registry/Facade, SkillRegistry, recipe descriptors и component manifest; определить canonical ownership каждого поля и исключить параллельные каталоги.
2. Спроектировать versioned descriptor со stable id, semantic version, descriptor hash, input/output schema hashes, declared capabilities, dependency refs, minimum Core/API version, trust/source state и lifecycle.
3. Разделить declared metadata и executable adapter availability. Descriptor не может сам регистрировать произвольный executable или менять permissions.
4. Определить compatibility states: compatible, upgrade_required, incompatible, missing_dependency, untrusted, disabled, unavailable.
5. Проверять signature/source trust по существующим supply-chain contracts; не изобретать локальную криптографию или второй signing authority.
6. Определить workflow node reference: exact descriptor/schema hash, typed input/output contract, dependency edges, idempotency/retry classification и provenance requirement.
7. Определить deprecation/removal semantics с backward compatibility для pinned workflow snapshots и historical runs.

## Зависимости
### Блокирующие
ToolManifest/ToolRegistry, Capability Registry/Facade, SkillRegistry/trust; PermissionEngine, approval/receipts, sandbox and component manifest/update.
### Опциональные
План 193 agent delegation descriptor intersection; план 180 remote agent capability intersection.

## Recovery и rollback
Unknown descriptor version/schema fails closed. Untrusted source остаётся видимым как metadata, но не становится executable. Existing workflows retain pinned descriptor hashes; automatic migration на newer versions запрещена. Rollback withdraws new descriptor projection and leaves current registry behavior unchanged.

## Verification
- Schema tests: stable ids, hash canonicalization, strict unknown-field policy.
- Compatibility matrix across Core/API and adapter versions.
- Trust tests for missing, invalid, expired and revoked provenance.
- Tests prove manifest cannot grant permissions or declare unimplemented adapters executable.
- Workflow snapshot tests preserve pinned descriptor/schema hashes.

## Release evidence
Descriptor schema version/hash, compatibility matrix, trust fixture hashes, adapter availability map и focused contract test results.

## Критерии выхода
- [ ] Descriptor is metadata-only and versioned.
- [ ] Executability is proven by a registered trusted adapter.
- [ ] No duplicate registry or trust root is introduced.
- [ ] Existing workflow snapshots remain stable.
