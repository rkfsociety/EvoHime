# План 38.0 — Adaptive Tool Catalog: dynamic selection и deferred tool schemas

Статус: предложено по [issue #18](https://github.com/rkfsociety/EvoHime/issues/18). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Adaptive Tool Catalog**: вместо передачи модели полного описания всех доступных tools/MCP/integration actions на каждом turn Core формирует компактный каталог и выдаёт модели только релевантное подмножество полных tool schemas.

Главный принцип:

> tool discovery и tool authorization — разные вещи.

Selection может только **сузить** уже разрешённый набор capabilities. Он никогда не выдаёт новые права.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/adaptive-tool-catalog.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 24.0 — Agent Skills: registry, SKILL.md и progressive disclosure.
- План 33.0 — Integration Provider SDK: единый контракт auth, actions, webhooks и test fixtures.
- План 37.0 — Agent Middleware Pipeline: typed hooks вокруг model/tool execution.
- План 44.0 — Tool Simulation Runtime: fixture/emulated dry-run без реальных side effects.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

Инварианты:

- selection только сужает authorized set;
- selector model не может вернуть неизвестный stable id;
- full schema загружается только после validation selected id;
- provider-side search получает только tools из authorized set;
- disabled/missing-credential tools не появляются executable;
- tool description не является authority;
- catalog metadata не содержит secrets;
- child selection работает внутри child grant subset.

## План реализации

1. Зафиксировать versioned typed contract, state machine, provenance, limits,
   failure/unknown-outcome semantics и threat model; отдельно перечислить
   поля, которые могут быть предложены моделью, и authoritative Core evidence.
2. Реализовать Core validation и durable storage/event transitions. Миграция
   должна быть additive, транзакционной, с backup/recovery и deterministic
   serialization/hash там, где сущность versioned.
3. Подключить существующие registry/tool/workflow/provider/child контуры,
   повторные grant/policy/approval проверки и bounded retry/cancellation.
4. Добавить additive IPC, main/preload adapter и metadata-only renderer/UI;
   sensitive payload, raw prompt/output и credentials не передавать.
5. Провести focused unit/storage/integration/recovery/security/eval tests,
   обновить architecture/current-state только после фактической реализации
   и сохранить команду воспроизведения проверки.

## Критерии готовности из issue

- [ ] Есть compact tool catalog projection.
- [ ] Full schemas загружаются только для выбранных tools.
- [ ] Selection работает только внутри authorized capability set.
- [ ] Есть bounded max tool count и explicit fallback policy.
- [ ] Поддержан хотя бы один deterministic и один semantic/model selector.
- [ ] Provider-native deferred search является optional optimization.
- [ ] Selection cache имеет безопасную invalidation policy.
- [ ] Diagnostics показывают выбор и стоимость selector-а.

## Ограничения и non-goals

- автоматическая регистрация новых tools моделью;
- расширение grants через relevance selection;
- глобальный vector marketplace tools;
- передача всех full schemas в selector prompt;
- обязательная зависимость от provider-native tool search;
- замена SkillRegistry.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#18 Adaptive Tool Catalog: dynamic selection и deferred tool schemas](https://github.com/rkfsociety/EvoHime/issues/18)
