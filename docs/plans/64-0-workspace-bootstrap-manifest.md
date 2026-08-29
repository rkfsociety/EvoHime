# План 64.0 — Workspace Bootstrap Manifest: безопасная подготовка project environment перед agent run

Статус: предложено по [issue #44](https://github.com/rkfsociety/EvoHime/issues/44). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime **Workspace Bootstrap Manifest**: versioned project-level контракт, который описывает, как подготовить рабочее окружение перед первым agent run или после значимого изменения проекта.

Bootstrap нужен для типичных действий:

- установить/проверить зависимости;
- сгенерировать derived files;
- проверить наличие required tools;
- создать безопасные локальные config files из templates;
- выполнить project-specific initialization;
- прогреть build/cache, если пользователь это разрешил.

Ключевой принцип: repository-provided setup instructions считаются **недоверенным executable intent** и проходят Core validation, trust review, Execution Policy и approvals. Наличие файла в репозитории не является разрешением запустить его автоматически.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/workspace-bootstrap-manifest.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Зависимости

### Блокирующие

- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- План 47.0 — Skill Trust Pipeline: deterministic scanning, contextual review и quarantine перед активацией.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 44.0 — Tool Simulation Runtime: fixture/emulated dry-run без реальных side effects.
- План 55.0 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой.
- План 77.0 — Headless Core CLI: non-interactive agent/workflow runs для CI, scripts и NDJSON automation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- repository bootstrap content untrusted by default;
- first run/change hash requires review according to policy;
- commands проходят ExecutionPolicy;
- no full inherited secret environment;
- network explicit;
- arbitrary installer hint не исполняется автоматически;
- script file path/hash фиксируется;
- no overwrite local config by default;
- no automatic Git commit/add;
- imported workflow/skill не активирует bootstrap;
- bootstrap cannot expand agent grants;
- output redacted через sensitive-data policy.

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

- [ ] Есть versioned WorkspaceBootstrapManifest.
- [ ] Repository-provided bootstrap требует trust/review до исполнения.
- [ ] Все commands запускаются через ExecutionPolicy.
- [ ] Environment/secrets/network deny-by-default/explicit.
- [ ] Есть fingerprint/freshness cache, чтобы не запускать setup каждый turn.
- [ ] Manifest hash change инвалидирует прежний trust/result.
- [ ] Tool/dependency checks отделены от auto-install.
- [ ] Agent получает компактный environment summary, а не raw scripts.

## Ограничения и non-goals

- автоматический `curl | sh` по инструкции репозитория;
- полноценный универсальный package/environment manager;
- Docker как обязательная модель окружения;
- выдача bootstrap process всех secrets EvoHime;
- запуск setup hook на каждый turn;
- автоматический Git commit generated files;
- доверие setup script только потому, что repository уже был открыт.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#44 Workspace Bootstrap Manifest: безопасная подготовка project environment перед agent run](https://github.com/rkfsociety/EvoHime/issues/44)
