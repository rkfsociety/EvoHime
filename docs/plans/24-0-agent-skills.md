# План 24.0 — Agent Skills registry и progressive disclosure

Статус: предложено по [issue #3](https://github.com/rkfsociety/EvoHime/issues/3).
Это обзорный план направления; он не устанавливает пакеты и не создаёт новую
security boundary.

## Цель

Добавить Core-owned реестр переиспользуемых `SKILL.md` packages. В обычный
model context попадает только компактный каталог metadata, а полный skill и
references загружаются on-demand после явного выбора. Skill описывает workflow и
правила, но все действия остаются в существующих tool/workflow, capability,
policy и approval контурах.

## Текущее состояние и граница изменения

В Core уже есть capability registry, role/capability manifests, intent/loadout
ограничения, provenance/audit и typed Electron IPC. Они остаются authority.
Skill metadata не расширяет grants, а `allowed-tools` и `required-capabilities`
могут только дополнительно сузить допустимый loadout.

Предполагаемые точки интеграции:

- новый `crates/evohime-core/src/skill_registry.rs` — discovery, parsing,
  precedence, validation, cache и provenance;
- существующие `capability_registry.rs`, `child_roles.rs` и policy gate —
  narrowing и запрет self-escalation;
- `crates/desktop-ipc/proto/evohime.desktop.proto` и `lib.rs` — catalog/load
  commands и bounded events;
- Electron main bridge и Settings/Operations surfaces — список, ошибки и
  loaded-skill trace;
- проектная документация/fixtures — sample native и compatibility packages.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./24-1-agent-skills.md)
- [Этап 2 — runtime-интеграция и recovery](./24-2-agent-skills.md)
- [Этап 3 — IPC, client projection и UI](./24-3-agent-skills.md)
- [Этап 4 — verification, release-evidence и закрытие](./24-4-agent-skills.md)

## Зависимости

### Блокирующие

- Core-owned capability registry и существующий tool/workflow approval path;
- workspace path canonicalization с запретом traversal/reparse/symlink escape;
- authenticated additive desktop IPC и bounded event/audit projection;
- доступная политика размеров, encoding и retention для локальных файлов.

### Опциональные

- план 23 TaskCheckpoint для фиксации selected skill refs между compactions;
- планы 25–28 для использования skills в Goal, continuation, child и kernel.

## Skill package и discovery

Поддержать package с `SKILL.md`, optional `references/`, `scripts/` и `assets/`.
Frontmatter минимум содержит `name` и `description`; optional fields включают
`version`, `compatibility`, `allowed-tools`, `required-capabilities`,
`disable-model-invocation`, `scope` и безопасные metadata. Unknown безопасные
поля дают warning, а неизвестные поля permissions/exec отвергаются или
игнорируются fail-closed.

Источники сканируются только в явных roots: `%APPDATA%/EvoHime/skills/`,
`<workspace>/.agents/skills/`, optional compatibility imports `.codex/skills/`
и `.claude/skills/`, а также bundled skills. Нативный и imported источник
отмечаются отдельно. Нормализовать путь до открытия файла, запретить escape,
symlink/reparse traversal, oversized files и executable helper auto-run.

Precedence фиксировать детерминированно: explicit session/user selection,
project-native, global EvoHime, compatibility-imported, bundled. Collision
создаёт warning/event с обеими provenance refs и не делает молчаливую замену.
Catalog ordering, hash и выбранный winner должны быть воспроизводимыми.

## Progressive disclosure и Core API

Catalog item содержит `skill_id`, name, description, scope, source kind, version,
content hash, capability summary и validation status. Полный `SKILL.md` не
попадает в prompt до on-demand load. `LoadSkill` повторно проверяет текущий
hash/path/size/source, возвращает bounded content или safe error, а references
читаются отдельной командой с лимитами. Cache инвалидируется при изменении
metadata/hash; stale load не выдаёт старое содержимое как актуальное.

Provenance сохраняет источник, scope, version, content hash, выбранную задачу и
event/trace ref. Renderer получает только metadata и разрешённые excerpts;
secret-looking content и raw executable scripts не передаются без отдельной
политики.

## Безопасность и выполнение

Skill считается недоверенной инструкцией. Он не читает credentials, не меняет
registry/permissions, не устанавливает зависимости и не запускает scripts лишь
потому, что они лежат в package. Если helper нужен, он оформляется как обычный
typed tool/workflow request через Core с прежними approval/audit правилами.
Imported skill не получает дополнительных capabilities, а `required-capabilities`
никогда не поднимают parent grants.

## UI и трассировка

В Settings/Operations добавить bounded список обнаруженных skills: scope,
source, version/hash, enabled/disabled, validation error и способ открыть
локальный package. В task trace показывать только id/version/hash загруженного
skill и причину выбора. Полный текст доступен только по явной локальной команде,
если это допускает policy.

## Этапы реализации

1. Зафиксировать `SkillMetadataV1`, frontmatter schema, roots, precedence,
   limits и threat cases.
2. Реализовать discovery/parser/path safety/hash/cache в Core с тестовыми
   fixtures для native/imported/bundled источников.
3. Подключить catalog/load IPC и provenance в task trace без изменения grants.
4. Добавить UI projection, collision/validation diagnostics и deterministic
   reload.
5. Провести security/eval tests на traversal, imported narrowing и oversized
   references; обновить canonical docs после реализации.

## Критерии готовности

- global/project/compatibility discovery работает с deterministic precedence;
- invalid frontmatter, collision, oversized file и path escape дают typed error;
- model получает metadata catalog, полный skill загружается только on-demand;
- cache/hash/provenance отражают фактический источник и версию;
- imported/native skill не расширяет permissions;
- scripts, package install и network download не выполняются автоматически;
- IPC/UI показывают bounded state и loaded-skill trace;
- проходят parser, collision, traversal, cache, capability narrowing,
  redaction и documentation gates.

## Не входит

Marketplace, загрузка произвольного кода, автоматическая установка зависимостей,
unrestricted skill runtime, замена tools/workflows или auto-run scripts.
