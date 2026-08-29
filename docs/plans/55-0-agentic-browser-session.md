# План 55.0 — Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой

Статус: предложено по [issue #35](https://github.com/rkfsociety/EvoHime/issues/35). Это обзорный план направления;
реализация начинается после отдельного evidence review и уточнения текущих
контрактов. Закрытие issue означает перенос требований в план, а не готовность
функционала.

## Цель

Добавить в EvoHime Core-owned **Agentic Browser Session**: управляемую браузерную сессию для задач, где агенту нужно видеть и взаимодействовать с web UI, но без выдачи renderer/model прямого доступа к CDP, произвольному локальному браузерному профилю или unrestricted network socket.

Browser runtime должен давать модели стабильные typed references на страницу/элементы и проходить через обычные Core capabilities, approvals, execution/network policy и event logging.

## Текущее основание и граница

Новый контур должен оставаться Core-owned и typed. Renderer является только
проекцией; state, permissions, effects, approvals, recovery и SQLite остаются
под контролем Core. Существующие workflow, child, memory, ArtifactStore,
event-journal, provider и supervisor контракты не заменяются без отдельного
решения. Для durable state использовать additive transactional migration и
immutable/versioned записи; для внешних эффектов сохранять unknown outcome, а
не повторять side effect вслепую.

Кандидатная точка интеграции: `crates/evohime-core/src/agentic-browser-session.rs`,
а также соответствующий storage store, `crates/desktop-ipc/proto/evohime.desktop.proto`,
Electron main/preload bridge, bounded renderer projection и focused tests.
Имена файлов проверяются по live checkout на этапе реализации и не являются
заранее утверждённым API.

## Этапы направления

- [Этап 1 — Core-контракт, schema и storage](./55-1-agentic-browser-session.md)
- [Этап 2 — runtime-интеграция и recovery](./55-2-agentic-browser-session.md)
- [Этап 3 — IPC, client projection и UI](./55-3-agentic-browser-session.md)
- [Этап 4 — verification, release-evidence и закрытие](./55-4-agentic-browser-session.md)

## Зависимости

### Блокирующие

- Специфических межплановых blocking зависимостей нет; используется текущий Core/IPC фундамент проекта.
- действующие Core-owned capability/policy/approval, event journal, SQLite transaction/migration и authenticated IPC boundaries.

### Опциональные

- План 41.0 — Execution Policy Profiles: sandboxed shell/process runtime с Windows-first isolation.
- UI/diagnostics integration может быть добавлена после Core contract без изменения authority boundary.

## Короткая фиксация требований issue

### Контур/модель

В issue нет отдельного раздела с этим именем; требования остаются в полном тексте issue.

### Безопасность

- Core владеет session identity и tool routing;
- browser не расширяет grants;
- SSRF/private-network policy применяется на navigation/redirect/DNS resolution;
- isolated profile default;
- raw browser credentials не попадают в model/renderer trace;
- element refs scoped к session/page revision;
- arbitrary host file upload запрещён;
- download не выполняется;
- browser internal/file/custom schemes deny-by-default;
- renderer не получает direct CDP endpoint.

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

- [ ] Есть Core-owned BrowserSession lifecycle.
- [ ] Модель работает через typed browser tools и stable element refs.
- [ ] Refs имеют page revision и stale protection.
- [ ] Есть network/SSRF policy с private-address protection.
- [ ] Default browser profile isolated/ephemeral.
- [ ] Upload/download проходят Artifact/Core boundaries.
- [ ] Workbench может показывать безопасную live projection.
- [ ] Human takeover поддерживается без гонки с agent actions.

## Ограничения и non-goals

- stealth/anti-bot обход;
- CAPTCHA solving;
- использование личного browser profile без explicit opt-in;
- unrestricted CDP для модели/renderer;
- автоматический запуск downloads;
- обход сайтовых permission/security механизмов;
- полноценный replacement обычного пользовательского браузера.

Дополнительно обязательно: новая поверхность не расширяет capabilities,
не обходится через renderer или imported content, не превращает неизвестный
результат в успех и не добавляет внешний runtime/network без явного typed/policy
контракта. Документ считается выполненным только вместе с тестами,
`git diff --check` и обновлением канонической документации после реализации.

## Связанный issue

- [#35 Agentic Browser Session: sandboxed browser automation со stable refs и SSRF-защитой](https://github.com/rkfsociety/EvoHime/issues/35)
