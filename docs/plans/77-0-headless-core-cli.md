# План 77.0 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation

Статус: предложено по [issue #57](https://github.com/rkfsociety/EvoHime/issues/57). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime официальный **Headless Core CLI** поверх тех же Core-owned contracts, которые использует desktop UI: запуск agent/workflow задач из терминала, CI и скриптов без renderer, с machine-readable NDJSON/JSON выводом, стабильными exit codes, resume/cancel/status и теми же security/approval/runtime semantics.

CLI не должен становиться вторым agent runtime. Это ещё один клиент EvoHime Core.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/headless-core-cli.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 30.0 — Workflow Package: переносимый import/export без секретов и с rebinding зависимостей.
- План 31.0 — Visual Workflow Builder: typed canvas, validation и live runtime inspection.
- План 32.0 — Conversational Workflow Composer: создание и правка workflow из естественного языка.
- План 59.0 — Incremental Change Protocol: safe requirement-delta pipeline для существующих репозиториев.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

```text
CLI
  -> versioned Core client/IPC
  -> EvoHime Core
  -> Agent/Workflow/Goal runtime
```

CLI:

- не открывает database напрямую;
- не запускает model/tool runtime самостоятельно;
- не обходит capability/approval policy;
- не читает credentials из Core storage;
- не дублирует recovery/event-log state.

Если Core service не запущен, CLI может либо безопасно запустить официальный supervisor/Core instance, либо завершиться typed ошибкой согласно выбранной deployment model.

### Безопасность

- CLI не является privileged backdoor в Core;
- все commands проходят те же IPC auth/session/policy checks;
- non-TTY approvals deny-by-default без explicit policy;
- secret args не логируются;
- workspace paths canonicalized;
- imported profiles/presets не расширяют grants;
- detached run остаётся Core-owned;
- JSON stream redaction-aware;
- `--json` не выводит raw hidden reasoning;
- Core process/database не управляются через undocumented raw commands.

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

- [ ] CLI является клиентом существующего Core, а не отдельным runtime.
- [ ] Есть interactive, one-shot и NDJSON modes.
- [ ] Agent/workflow run можно запускать без desktop renderer.
- [ ] Есть stable run ids, watch/cancel/resume/status.
- [ ] Non-interactive approval semantics fail-safe.
- [ ] CLI использует существующие profiles/budgets/checkpoints/event log.
- [ ] Exit codes и JSON schemas versioned/stable.
- [ ] Credentials и sensitive output не протекают через CLI boundary.

## Ограничения и non-goals

- второй standalone agent implementation;
- автоматическое blanket auto-approve для CI;
- прямой доступ CLI к Core DB;
- shell scripting API поверх внутренних Rust типов;
- обязательный TUI-клон desktop приложения;
- background daemon, отличный от существующего EvoHime supervisor/Core;
- хранение raw secrets в flags/config;
- удалённый multi-tenant SaaS CLI.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#57 Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation](https://github.com/rkfsociety/EvoHime/issues/57)
